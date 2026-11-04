/* ==========================================================================
   AI 助手 · 右侧栏对话面板
   常驻右栏，打开就能问：不需要先选文字，也不会把用户踢到整页。

   信息架构参考 Claudian 的侧栏对话（多轮、@ 引用笔记、斜杠命令、结果写回），
   但刻意不引入它的 CLI / agent / MCP / 多 provider 进程那一套 —— 这里只有
   「一次流式 chat 请求 + 一份按笔记隔离的会话」。

   关键取舍：
   - 当前笔记全文默认进上下文（6000 字上限，从尾部截断）；
   - 选区是「额外」上下文，不是入口：没有选区也能正常聊天；
   - 写回先算整篇 before/after，记进 history.js，失败可回退。
   ========================================================================== */

import {
  buildChatMessages,
  loadConfig,
  onConfigChange,
  streamChat,
  truncateNote,
  truncateRef,
} from './api.js';
import {
  getSession,
  makeMessage,
  onSessionsChange,
  saveSession,
} from './sessions.js';
import { recordRewrite } from './history.js';

/* ---------------- 斜杠命令 ---------------- */

const SLASH_COMMANDS = [
  {
    id: 'summary',
    label: '/总结',
    desc: '概括主旨与要点',
    template:
      '请总结当前这篇笔记：先用一句话概括主旨，再列出 3–5 个要点。用 Markdown 列表。',
  },
  {
    id: 'polish',
    label: '/润色',
    desc: '润色选中或整段文字',
    template:
      '请润色当前选中的文字（如果没有选中，就润色光标附近的段落）：保持原意与 Markdown 结构，让表达更通顺、准确。直接给出改好的文字。',
  },
  {
    id: 'translate',
    label: '/翻译',
    desc: '翻译选中内容',
    template:
      '请把当前选中的文字翻译成英文（如果没有选中，就翻译当前笔记的核心段落）。保留 Markdown 结构、代码与链接。',
  },
  {
    id: 'continue',
    label: '/续写',
    desc: '顺着当前内容往下写',
    template:
      '请顺着当前笔记的内容与语气继续写下去，保持人称、风格与 Markdown 结构。直接给出续写内容。',
  },
  {
    id: 'explain',
    label: '/解释',
    desc: '解释这段在说什么',
    template:
      '请解释当前选中的内容是什么意思；如果没有选中，就解释当前这篇笔记在讲什么。用清晰的中文，必要时举例。',
  },
  {
    id: 'outline',
    label: '/提纲',
    desc: '给这篇笔记列提纲',
    template:
      '请基于当前笔记帮我列一份提纲：层级清晰，用 Markdown 列表，必要时给每一节加一句话说明。',
  },
];

/* ---------------- 小工具 ---------------- */

function h(tag, props = {}, ...children) {
  const node = document.createElement(tag);

  for (const [key, value] of Object.entries(props || {})) {
    if (value === undefined || value === null || value === false) continue;
    if (key === 'class') node.className = value;
    else if (key === 'text') node.textContent = String(value);
    else if (key === 'dataset') Object.assign(node.dataset, value);
    else if (key.startsWith('on') && typeof value === 'function') {
      node.addEventListener(key.slice(2).toLowerCase(), value);
    } else if (value === true) node.setAttribute(key, '');
    else node.setAttribute(key, String(value));
  }

  const add = (child) => {
    if (child === undefined || child === null || child === false) return;
    if (Array.isArray(child)) {
      child.forEach(add);
      return;
    }
    node.append(child.nodeType ? child : document.createTextNode(String(child)));
  };
  children.forEach(add);

  return node;
}

function actButton(label, { title = '', disabled = false, onClick } = {}) {
  return h(
    'button',
    {
      class: 'c-ai-msg__act',
      type: 'button',
      title: title || label,
      disabled: disabled || null,
      onclick: onClick,
    },
    label
  );
}

function basenameOf(path) {
  const name = String(path || '').split('/').pop() || '';
  return name.replace(/\.md$/i, '');
}

function formatCount(n) {
  return String(Math.max(0, Number(n) || 0)).replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

function newId() {
  const uuid = globalThis.crypto?.randomUUID?.();
  return uuid || `ai-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(String(text ?? ''));
    return true;
  } catch {
    const area = document.createElement('textarea');
    area.value = String(text ?? '');
    area.style.position = 'fixed';
    area.style.opacity = '0';
    document.body.append(area);
    area.select();
    const ok = document.execCommand?.('copy');
    area.remove();
    return Boolean(ok);
  }
}

/* ---------------- 面板 ---------------- */

/**
 * @param {HTMLElement} host 右侧栏视图容器
 * @param {object} ctx 插件上下文
 * @param {{openSettings: () => void}} deps 打不开宿主模块，回调由 main.js 注入
 */
export function createChatPanel(host, ctx, deps = {}) {
  let config = loadConfig(ctx);

  let notePath = '';
  let noteTitle = '';
  let noteBlock = { text: '', truncated: false };
  let noteTotal = 0;
  let noteEnabled = true;

  let messages = [];
  let refs = []; // { path, title, text, truncated }
  let selection = null; // { text, start, end }
  let selectionHidden = false;

  let streaming = false;
  let streamingMsg = null;
  let streamEl = null;
  let streamAbort = null;
  let runToken = 0;
  let writingSession = false;

  let menu = null; // { kind, start, query, items, index, loading }
  let menuToken = 0;
  let noteIndex = null;

  let destroyed = false;

  /* ---------- DOM ---------- */

  const head = h(
    'div',
    { class: 'c-ai-chat__head' },
    h('div', { class: 'c-ai-chat__title' }, 'AI'),
    h(
      'button',
      {
        class: 'c-ai-chat__gear',
        type: 'button',
        title: 'AI 设置与历史',
        'aria-label': 'AI 设置与历史',
        onclick: () => deps.openSettings?.(),
      },
      '⚙'
    )
  );

  const ctxBar = h('div', { class: 'c-ai-chat__ctx' });
  const flow = h('div', { class: 'c-ai-chat__flow' });

  const input = h('textarea', {
    class: 'c-ai-chat__input selectable',
    rows: '1',
    placeholder: '问点什么…',
    spellcheck: 'false',
    oninput: () => {
      syncHeight();
      updateComposer();
      detectTrigger();
    },
    onkeydown: (event) => onKeyDown(event),
  });

  const slashBtn = h(
    'button',
    {
      class: 'c-ai-chat__tool',
      type: 'button',
      title: '斜杠命令',
      'aria-label': '斜杠命令',
      onclick: () => insertTrigger('/'),
    },
    '/'
  );

  const atBtn = h(
    'button',
    {
      class: 'c-ai-chat__tool',
      type: 'button',
      title: '引用另一篇笔记',
      'aria-label': '引用另一篇笔记',
      onclick: () => insertTrigger('@'),
    },
    '@'
  );

  const sendBtn = h(
    'button',
    {
      class: 'c-btn c-btn--primary c-btn--sm c-ai-chat__send',
      type: 'button',
      onclick: () => (streaming ? stopStream() : send(input.value)),
    },
    '发送'
  );

  const pop = h('div', { class: 'c-ai-chat__pop', hidden: true });

  const composer = h(
    'div',
    { class: 'c-ai-chat__composer' },
    pop,
    h('div', { class: 'c-ai-chat__box' }, input),
    h(
      'div',
      { class: 'c-ai-chat__bar' },
      h('div', { class: 'c-ai-chat__tools' }, slashBtn, atBtn),
      sendBtn
    )
  );

  const root = h('div', { class: 'c-ai-chat' }, head, ctxBar, flow, composer);
  host.append(root);

  /* ---------- 会话与上下文 ---------- */

  function persist(path = notePath) {
    if (!path) return;
    writingSession = true;
    try {
      saveSession(ctx, path, messages);
    } finally {
      writingSession = false;
    }
  }

  function refreshConfig() {
    config = loadConfig(ctx);
    if (!streaming) renderMessages();
    updateComposer();
  }

  function readSelection() {
    let next = null;
    try {
      const sel = ctx.editor.getSelection();
      if (sel && String(sel.text || '').trim()) {
        next = { text: String(sel.text), start: sel.start, end: sel.end };
      }
    } catch {
      next = null;
    }

    if (!next) {
      selection = null;
      selectionHidden = false;
    } else if (
      !selection ||
      selection.text !== next.text ||
      selection.start !== next.start ||
      selection.end !== next.end
    ) {
      selection = next;
      selectionHidden = false;
    }

    renderContextBar();
    updateComposer();
  }

  /** 笔记切换 / 首次挂载：把面板状态对齐到当前编辑器 */
  function syncNote() {
    if (destroyed) return;

    const note = ctx.editor.currentNote();
    const path = note?.path || '';
    const content = String(note?.content || '');

    if (path !== notePath) {
      if (streaming) abortForSwitch();
      notePath = path;
      noteTitle = note?.title || '';
      messages = path ? getSession(ctx, path) : [];
      refs = [];
      noteEnabled = true;
      selection = null;
      selectionHidden = false;
      noteBlock = truncateNote(content);
      noteTotal = content.length;
      noteIndex = null;
      readSelection();
      renderMessages();
      scrollBottom(true);
      return;
    }

    noteTitle = note?.title || noteTitle;
    noteBlock = truncateNote(content);
    noteTotal = content.length;
    readSelection();
    renderContextBar();
  }

  /** 切换笔记时把上一段的流收掉：有内容就存，空占位就丢掉 */
  function abortForSwitch() {
    runToken += 1;
    streamAbort?.abort();
    streamAbort = null;

    if (streamingMsg) {
      streamingMsg.streaming = false;
      if (!String(streamingMsg.content || '').trim() && !streamingMsg.error) {
        messages = messages.filter((m) => m !== streamingMsg);
      }
    }
    streaming = false;
    streamingMsg = null;
    streamEl = null;
    persist();
  }

  /* ---------- 渲染：上下文条 ---------- */

  function contextChip({ label, title = '', extra = '', onRemove }) {
    return h(
      'span',
      { class: 'c-ai-ctx__chip', title: title || null },
      h('span', { class: 'c-ai-ctx__text' }, label),
      extra ? h('span', { class: 'c-ai-ctx__tag' }, extra) : null,
      h(
        'button',
        {
          class: 'c-ai-ctx__x',
          type: 'button',
          title: '移除',
          'aria-label': '移除',
          onclick: (event) => {
            event.preventDefault();
            onRemove?.();
          },
        },
        '×'
      )
    );
  }

  function renderContextBar() {
    ctxBar.replaceChildren();

    if (!notePath) {
      ctxBar.append(h('span', { class: 'c-ai-ctx__empty' }, '没有打开的笔记'));
      return;
    }

    let count = 0;

    if (noteEnabled) {
      ctxBar.append(
        contextChip({
          label: `当前笔记 · ${formatCount(noteTotal)} 字`,
          title: '当前笔记全文会随问题一起发送',
          extra: noteBlock.truncated ? '已截断' : '',
          onRemove: () => {
            noteEnabled = false;
            renderContextBar();
          },
        })
      );
      count += 1;
    } else {
      ctxBar.append(
        h(
          'button',
          {
            class: 'c-ai-ctx__add',
            type: 'button',
            title: '把当前笔记加回上下文',
            onclick: () => {
              noteEnabled = true;
              renderContextBar();
            },
          },
          '＋ 当前笔记'
        )
      );
      count += 1;
    }

    if (selection?.text && !selectionHidden) {
      ctxBar.append(
        contextChip({
          label: `选中 ${formatCount(selection.text.length)} 字`,
          title: selection.text.slice(0, 200),
          onRemove: () => {
            selectionHidden = true;
            renderContextBar();
          },
        })
      );
      count += 1;
    }

    for (const ref of refs) {
      ctxBar.append(
        contextChip({
          label: `引用 · ${ref.title || basenameOf(ref.path)}`,
          title: ref.path,
          extra: ref.truncated ? '截断' : '',
          onRemove: () => {
            refs = refs.filter((item) => item !== ref);
            renderContextBar();
          },
        })
      );
      count += 1;
    }

    if (!count) ctxBar.append(h('span', { class: 'c-ai-ctx__empty' }, '未添加上下文'));
  }

  /* ---------- 渲染：消息 ---------- */

  function gateNode() {
    return h(
      'div',
      { class: 'c-ai-chat__gate' },
      h('div', { class: 'c-ai-chat__gate-title' }, '先配置 AI 连接'),
      h(
        'div',
        { class: 'c-ai-chat__gate-desc' },
        '填入任意 OpenAI 兼容服务的 API Key，就能在这里直接提问。不需要先选文字。'
      ),
      h(
        'button',
        {
          class: 'c-btn c-btn--primary c-btn--sm',
          type: 'button',
          onclick: () => deps.openSettings?.(),
        },
        '去设置'
      )
    );
  }

  function emptyNode() {
    const examples = ['总结这篇', '帮我列个提纲', '这段能怎么改'];
    return h(
      'div',
      { class: 'c-ai-chat__empty' },
      h('div', { class: 'c-ai-chat__empty-title' }, '直接问就好'),
      h(
        'div',
        { class: 'c-ai-chat__empty-desc' },
        '不用先选文字。可以问这篇讲了什么、帮你起草、对比另一篇；输入 / 用快捷命令，@ 引用其他笔记。'
      ),
      h(
        'div',
        { class: 'c-ai-chat__examples' },
        ...examples.map((text) =>
          h(
            'button',
            {
              class: 'c-ai-chat__example',
              type: 'button',
              onclick: () => {
                input.value = text;
                syncHeight();
                updateComposer();
                input.focus();
              },
            },
            text
          )
        )
      )
    );
  }

  function noNoteNode() {
    return h(
      'div',
      { class: 'c-ai-chat__empty' },
      h('div', { class: 'c-ai-chat__empty-title' }, '先打开一篇笔记'),
      h(
        'div',
        { class: 'c-ai-chat__empty-desc' },
        '对话按笔记隔离：每篇笔记有自己的上下文和会话记录。'
      )
    );
  }

  function messageNode(msg, index) {
    const isUser = msg.role === 'user';
    const wrap = h('div', {
      class: `c-ai-msg c-ai-msg--${isUser ? 'user' : 'bot'}`,
      dataset: { id: msg.id, index: String(index) },
    });

    if (isUser) {
      wrap.append(h('div', { class: 'c-ai-msg__bubble' }, msg.content));
      return wrap;
    }

    const md = h('div', { class: 'c-ai-msg__md' });

    if (msg.error) {
      md.append(h('div', { class: 'c-ai-msg__error' }, `生成失败：${msg.error}`));
    } else if (streaming && msg.streaming) {
      paintInto(md);
      streamEl = md;
    } else {
      md.innerHTML = ctx.ui.markdown(msg.content) || '<p class="c-ai-chat__muted">（空回复）</p>';
    }

    wrap.append(md);

    const acts = h('div', { class: 'c-ai-msg__acts' });
    const hasContent = Boolean(String(msg.content || '').trim());

    acts.append(
      actButton('插入到光标', {
        title: '把这条回复插入到编辑器光标处',
        disabled: streaming || !hasContent || !notePath,
        onClick: () => writeBack(msg, 'insert'),
      }),
      actButton('替换选区', {
        title: selection?.text ? '用这条回复替换当前选中的文字' : '先在笔记里选中一段文字',
        disabled: streaming || !hasContent || !notePath || !selection?.text,
        onClick: () => writeBack(msg, 'replace'),
      }),
      actButton('复制', {
        disabled: !hasContent,
        onClick: async () => {
          const ok = await copyText(msg.content);
          ctx.ui.toast(ok ? '已复制' : '复制失败', {
            icon: ok ? 'copy' : 'x-circle',
            tone: ok ? '' : 'danger',
            duration: 1600,
          });
        },
      }),
      actButton('重试', {
        title: '用同样的提问重新生成',
        disabled: streaming,
        onClick: () => retry(msg),
      })
    );

    const write = h('div', { class: 'c-ai-msg__write', hidden: true });
    wrap.append(acts, write);
    writeEls.set(msg.id, write);
    return wrap;
  }

  const writeEls = new Map();

  function renderMessages() {
    if (destroyed) return;
    writeEls.clear();
    const keepBottom = isNearBottom();
    flow.replaceChildren();

    const hasKey = Boolean(config.apiKey.trim());
    if (!hasKey) {
      flow.append(gateNode());
      if (!messages.length) {
        updateComposer();
        return;
      }
    }

    if (!notePath) {
      flow.append(noNoteNode());
      updateComposer();
      return;
    }

    if (!messages.length) {
      if (hasKey) flow.append(emptyNode());
    } else {
      messages.forEach((msg, index) => flow.append(messageNode(msg, index)));
    }

    updateComposer();
    if (keepBottom) scrollBottom(true);
  }

  function isNearBottom() {
    return flow.scrollHeight - flow.scrollTop - flow.clientHeight < 90;
  }

  function scrollBottom(force = false) {
    if (force || isNearBottom()) flow.scrollTop = flow.scrollHeight;
  }

  function paintInto(el) {
    const text = String(streamingMsg?.content || '');
    if (!text) {
      el.innerHTML =
        '<span class="c-ai-chat__thinking">正在思考</span><span class="c-ai-chat__caret" aria-hidden="true"></span>';
      return;
    }
    el.innerHTML = `${ctx.ui.markdown(text)}<span class="c-ai-chat__caret" aria-hidden="true"></span>`;
  }

  function paintStream() {
    if (!streamEl) return;
    const stick = isNearBottom();
    paintInto(streamEl);
    if (stick) scrollBottom(true);
  }

  function updateComposer() {
    const hasKey = Boolean(config.apiKey.trim());
    const canType = hasKey && Boolean(notePath);

    input.disabled = !canType || streaming;
    slashBtn.disabled = !canType || streaming;
    atBtn.disabled = !canType || streaming;

    input.placeholder = !hasKey
      ? '配置 API Key 后即可对话'
      : !notePath
        ? '先打开一篇笔记'
        : streaming
          ? '正在生成…（可点「停止」）'
          : '问点什么…（Enter 发送）';

    if (streaming) {
      sendBtn.textContent = '停止';
      sendBtn.className = 'c-btn c-btn--danger c-btn--sm c-ai-chat__send';
      sendBtn.disabled = false;
    } else {
      sendBtn.textContent = '发送';
      sendBtn.className = 'c-btn c-btn--primary c-btn--sm c-ai-chat__send';
      sendBtn.disabled = !canType || !input.value.trim();
    }
  }

  function syncHeight() {
    input.style.height = 'auto';
    input.style.height = `${Math.min(160, Math.max(34, input.scrollHeight))}px`;
  }

  /* ---------- 发送 / 停止 ---------- */

  async function send(raw) {
    const text = String(raw || '').trim();
    if (!text || streaming || destroyed) return;

    if (!config.apiKey.trim()) {
      ctx.ui.toast('先配置 API Key', { icon: 'info' });
      return;
    }
    if (!notePath) {
      ctx.ui.toast('先打开一篇笔记', { icon: 'info' });
      return;
    }

    const note = ctx.editor.currentNote();
    if (!note || note.path !== notePath) {
      syncNote();
      if (!notePath) return;
    }
    if (!note || note.path !== notePath) return;

    const path = notePath;
    const userMsg = makeMessage('user', text);
    const botMsg = makeMessage('assistant', '');
    botMsg.streaming = true;

    messages.push(userMsg, botMsg);
    input.value = '';
    syncHeight();
    closeMenu();
    updateComposer();

    const history = messages.slice(0, -2);
    const token = ++runToken;
    const controller = new AbortController();
    streamAbort = controller;
    streaming = true;
    streamingMsg = botMsg;

    renderMessages();
    scrollBottom(true);

    // 上下文在「按下发送」这一刻快照：请求过程中用户改了选区，不该影响这一轮
    const payload = {
      note: { path: note.path, title: note.title || noteTitle },
      noteText: noteEnabled ? noteBlock.text : '',
      noteTruncated: noteEnabled && noteBlock.truncated,
      selection: selectionHidden ? '' : selection?.text || '',
      refs: refs.map((ref) => ({
        path: ref.path,
        title: ref.title,
        text: ref.text,
        truncated: ref.truncated,
      })),
      history,
      userText: text,
    };

    try {
      const full = await streamChat({
        ctx,
        config,
        messages: buildChatMessages(payload),
        signal: controller.signal,
        onDelta: (piece, all) => {
          if (token !== runToken) return;
          botMsg.content = all;
          paintStream();
        },
      });

      if (token !== runToken) return;
      botMsg.content = full;
      botMsg.streaming = false;
      streaming = false;
      streamingMsg = null;
      streamEl = null;
      streamAbort = null;
      persist(path);
      renderMessages();
      scrollBottom(true);
    } catch (err) {
      if (token !== runToken) return;
      const message = String(err?.message || err);
      botMsg.streaming = false;
      streaming = false;
      streamingMsg = null;
      streamEl = null;
      streamAbort = null;

      if (message === '已取消') {
        botMsg.cancelled = true;
        if (!botMsg.content.trim()) botMsg.content = '（已停止）';
      } else {
        botMsg.error = message;
        ctx.ui.toast(`AI 请求失败：${message}`, {
          icon: 'x-circle',
          tone: 'danger',
          duration: 3200,
        });
      }

      persist(path);
      renderMessages();
    }
  }

  function stopStream() {
    if (!streaming) return;
    runToken += 1;
    streamAbort?.abort();
    streamAbort = null;

    if (streamingMsg) {
      streamingMsg.streaming = false;
      streamingMsg.cancelled = true;
      if (!streamingMsg.content.trim()) streamingMsg.content = '（已停止）';
    }

    streaming = false;
    streamingMsg = null;
    streamEl = null;
    persist();
    renderMessages();
  }

  function retry(msg) {
    if (streaming) return;
    const index = messages.findIndex((item) => item.id === msg.id);
    if (index < 0) return;

    let userIndex = index - 1;
    while (userIndex >= 0 && messages[userIndex].role !== 'user') userIndex -= 1;
    if (userIndex < 0) return;

    const prompt = messages[userIndex].content;
    messages = messages.slice(0, userIndex);
    persist();
    renderMessages();
    send(prompt);
  }

  /* ---------- 写回 ---------- */

  function writeBack(msg, mode) {
    if (streaming) return;
    const text = String(msg.content || '').trim();
    if (!text) return;

    const note = ctx.editor.currentNote();
    if (!note) {
      ctx.ui.toast('先打开一篇笔记', { icon: 'info' });
      return;
    }

    const sel = ctx.editor.getSelection();
    const before = note.content;
    let after = '';

    if (mode === 'replace') {
      if (!String(sel.text || '').trim()) {
        ctx.ui.toast('先在笔记里选中一段文字', { icon: 'info' });
        return;
      }
      after = before.slice(0, sel.start) + text + before.slice(sel.end);
      ctx.editor.replaceSelection(text);
    } else {
      // insertText 在存在选区时会替换选区：after 必须按同一语义算，否则历史对不上
      after = before.slice(0, sel.start) + text + before.slice(sel.end);
      ctx.editor.insertAtCursor(text);
    }

    if (after === before) {
      ctx.ui.toast('正文没有变化', { icon: 'info' });
      return;
    }

    recordRewrite(ctx, {
      id: newId(),
      path: note.path,
      title: note.title || notePath,
      before,
      after,
      at: Date.now(),
      instruction: mode === 'replace' ? '对话 · 替换' : '对话 · 插入',
    });

    const hint = writeEls.get(msg.id);
    if (hint) {
      hint.hidden = false;
      hint.textContent = '已写入 · 可回退';
      // 提示在最后一条消息下面，可能正好压在输入框边缘：滚到底让它完整露出来
      requestAnimationFrame(() => {
        flow.scrollTop = flow.scrollHeight;
      });
    }
    ctx.ui.toast(mode === 'replace' ? '已替换选区，可回退' : '已插入到光标，可回退', {
      icon: 'check',
      duration: 2000,
    });
  }

  /* ---------- 斜杠 / @ 菜单 ---------- */

  function detectTrigger() {
    if (!menuHostOpen()) return;
    const pos = input.selectionStart;
    const before = input.value.slice(0, pos);

    const slash = before.match(/(^|\s)\/([^\s/]*)$/);
    const at = before.match(/(^|\s)@([^\s@]*)$/);

    const slashStart = slash ? slash.index + slash[1].length : -1;
    const atStart = at ? at.index + at[1].length : -1;

    if (slashStart >= 0 && slashStart >= atStart) {
      openMenu('slash', slash[2], slashStart);
      return;
    }
    if (atStart >= 0) {
      openMenu('at', at[2], atStart);
      return;
    }
    closeMenu();
  }

  function menuHostOpen() {
    const hasKey = Boolean(config.apiKey.trim());
    return hasKey && Boolean(notePath) && !input.disabled;
  }

  function openMenu(kind, query, start) {
    const token = ++menuToken;

    if (kind === 'slash') {
      const items = SLASH_COMMANDS.filter((item) =>
        `${item.label} ${item.desc}`.toLowerCase().includes(String(query || '').toLowerCase())
      );
      if (!items.length) {
        closeMenu();
        return;
      }
      menu = { kind, start, query, items, index: 0, loading: false };
      paintMenu();
      return;
    }

    menu = { kind, start, query, items: [], index: 0, loading: true };
    paintMenu();

    loadNoteIndex()
      .then((all) => {
        if (token !== menuToken || !menu || menu.kind !== 'at') return;
        const q = String(query || '').toLowerCase();
        const items = all
          .filter((item) => item.path !== notePath)
          .filter(
            (item) => !q || item.title.toLowerCase().includes(q) || item.path.toLowerCase().includes(q)
          )
          .slice(0, 8);
        menu.items = items;
        menu.loading = false;
        menu.index = 0;
        paintMenu();
      })
      .catch(() => {
        if (token !== menuToken || !menu || menu.kind !== 'at') return;
        menu.items = [];
        menu.loading = false;
        paintMenu();
      });
  }

  async function loadNoteIndex() {
    if (noteIndex) return noteIndex;
    const all = await ctx.vault.walk();
    noteIndex = (Array.isArray(all) ? all : [])
      .filter((entry) => entry.type === 'file' && /\.md$/i.test(entry.path))
      .map((entry) => ({ path: entry.path, title: basenameOf(entry.path) }));
    return noteIndex;
  }

  function paintMenu() {
    pop.replaceChildren();
    if (!menu) {
      pop.hidden = true;
      return;
    }

    if (menu.loading) {
      pop.append(h('div', { class: 'c-ai-chat__pop-note' }, '正在查找笔记…'));
      pop.hidden = false;
      return;
    }

    if (!menu.items.length) {
      pop.append(
        h(
          'div',
          { class: 'c-ai-chat__pop-note' },
          menu.kind === 'at' ? '没有匹配的笔记' : '没有匹配的命令'
        )
      );
      pop.hidden = false;
      return;
    }

    menu.items.forEach((item, index) => {
      const isSlash = menu.kind === 'slash';
      pop.append(
        h(
          'button',
          {
            class: `c-ai-chat__pop-item${index === menu.index ? ' is-active' : ''}`,
            type: 'button',
            onmousedown: (event) => event.preventDefault(),
            onclick: () => chooseMenu(item),
          },
          h(
            'span',
            { class: 'c-ai-chat__pop-main' },
            h('span', { class: 'c-ai-chat__pop-title' }, isSlash ? item.label : item.title),
            h(
              'span',
              { class: 'c-ai-chat__pop-desc' },
              isSlash ? item.desc : item.path
            )
          )
        )
      );
    });
    pop.hidden = false;
  }

  function closeMenu() {
    if (!menu && pop.hidden) return;
    menu = null;
    menuToken += 1;
    pop.hidden = true;
    pop.replaceChildren();
  }

  function chooseMenu(item) {
    if (!menu) return;
    if (menu.kind === 'slash') applySlash(item);
    else applyReference(item);
  }

  function applySlash(item) {
    const start = menu.start;
    const end = input.selectionStart;
    input.value = input.value.slice(0, start) + item.template + input.value.slice(end);
    const caret = start + item.template.length;
    input.setSelectionRange(caret, caret);
    closeMenu();
    syncHeight();
    updateComposer();
    input.focus();
  }

  async function applyReference(item) {
    const start = menu.start;
    const end = input.selectionStart;
    input.value = input.value.slice(0, start) + input.value.slice(end);
    const caret = start;
    input.setSelectionRange(caret, caret);
    closeMenu();
    syncHeight();
    updateComposer();
    input.focus();

    if (refs.length >= 3) {
      ctx.ui.toast('最多引用 3 篇笔记', { icon: 'info' });
      return;
    }
    if (refs.some((ref) => ref.path === item.path)) {
      ctx.ui.toast('这篇笔记已经在引用里了', { icon: 'info' });
      return;
    }

    try {
      const raw = await ctx.vault.read(item.path);
      const body = truncateRef(raw);
      refs.push({
        path: item.path,
        title: item.title,
        text: body.text,
        truncated: body.truncated,
      });
      renderContextBar();
    } catch (err) {
      ctx.ui.toast(`读取《${item.title}》失败：${String(err?.message || err)}`, {
        icon: 'x-circle',
        tone: 'danger',
      });
    }
  }

  function insertTrigger(char) {
    if (input.disabled) return;
    input.focus();
    const start = input.selectionStart;
    const end = input.selectionEnd;
    input.value = input.value.slice(0, start) + char + input.value.slice(end);
    input.setSelectionRange(start + 1, start + 1);
    syncHeight();
    updateComposer();
    detectTrigger();
  }

  function onKeyDown(event) {
    if (menu) {
      if (event.key === 'ArrowDown') {
        event.preventDefault();
        menu.index = (menu.index + 1) % Math.max(1, menu.items.length);
        paintMenu();
        return;
      }
      if (event.key === 'ArrowUp') {
        event.preventDefault();
        menu.index =
          (menu.index - 1 + Math.max(1, menu.items.length)) % Math.max(1, menu.items.length);
        paintMenu();
        return;
      }
      if (event.key === 'Enter' || event.key === 'Tab') {
        if (!menu.loading && menu.items.length) {
          event.preventDefault();
          chooseMenu(menu.items[menu.index]);
          return;
        }
      }
      if (event.key === 'Escape') {
        event.preventDefault();
        closeMenu();
        return;
      }
    }

    if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      send(input.value);
    }
  }

  /* ---------- 事件 ---------- */

  const offSelection = ctx.editor.onSelectionChange(() => readSelection());

  const offOpen = ctx.events.on('note:open', () => {
    noteIndex = null;
    syncNote();
  });

  const offSave = ctx.events.on('note:save', (payload) => {
    if (!payload || payload.path !== notePath) return;
    noteBlock = truncateNote(payload.content);
    noteTotal = String(payload.content || '').length;
    renderContextBar();
  });

  const offConfig = onConfigChange(() => refreshConfig());

  const offSessions = onSessionsChange(() => {
    if (writingSession || streaming || destroyed) return;
    const fresh = notePath ? getSession(ctx, notePath) : [];
    messages = fresh;
    renderMessages();
  });

  const onDocDown = (event) => {
    if (!pop.hidden && !root.contains(event.target)) closeMenu();
  };
  document.addEventListener('pointerdown', onDocDown, true);

  /* ---------- 启动 ---------- */

  requestAnimationFrame(() => {
    if (destroyed) return;
    syncNote();
    renderMessages();
    scrollBottom(true);
    syncHeight();
  });

  renderContextBar();
  renderMessages();
  updateComposer();
  syncHeight();

  return {
    refresh() {
      if (!streaming) syncNote();
    },
    destroy() {
      if (destroyed) return;
      destroyed = true;
      runToken += 1;
      streamAbort?.abort();
      streamAbort = null;

      // 停用插件时如果正在生成：partial 也要落盘，别让刚问出去的问题白丢
      if (streaming && streamingMsg) {
        streamingMsg.streaming = false;
        if (!String(streamingMsg.content || '').trim() && !streamingMsg.error) {
          messages = messages.filter((m) => m !== streamingMsg);
        }
        persist(notePath);
        streaming = false;
        streamingMsg = null;
        streamEl = null;
      }

      offSelection?.();
      offOpen?.();
      offSave?.();
      offConfig?.();
      offSessions?.();
      document.removeEventListener('pointerdown', onDocDown, true);
      root.remove();
    },
  };
}

/** 供命令面板/调试用：当前面板可用的快捷命令清单 */
export function slashCommands() {
  return SLASH_COMMANDS.map((item) => ({ ...item }));
}
