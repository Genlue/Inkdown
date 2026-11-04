/* ==========================================================================
   AI 助手 · 设置与历史（整页）
   对话搬进右侧栏后，这个整页只保留两件事：
   - 连接设置（baseUrl / 模型 / Key / 温度 / 测试连接）；
   - 写入历史（插入与替换都会留一条整篇 before/after，可回退）。

   历史仍然沿用 history.js 的安全线：只回退当前打开的笔记，正文与记录里
   的 after 不一致时必须二次确认。
   ========================================================================== */

import {
  canStream,
  loadConfig,
  maskKey,
  saveConfig,
  testConnection,
} from './api.js';
import { loadHistory, onHistoryChange, restoreEntry } from './history.js';

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

function button(label, { variant = '', size = '', disabled = false, title = '', onClick } = {}) {
  const cls = ['c-btn', variant && `c-btn--${variant}`, size && `c-btn--${size}`]
    .filter(Boolean)
    .join(' ');
  return h(
    'button',
    { class: cls, type: 'button', title: title || null, disabled: disabled || null, onclick: onClick },
    label
  );
}

function field({ type = 'text', value = '', placeholder = '', min, max, step, width = '', onInput } = {}) {
  const input = h('input', {
    class: 'c-field__input selectable',
    type,
    value,
    placeholder,
    min,
    max,
    step,
    oninput: (event) => onInput?.(event.target.value),
  });
  const wrap = h('div', { class: 'c-field', style: width ? `width:${width}` : null }, input);
  wrap.input = input;
  return wrap;
}

function row(label, hint, control, stack = false) {
  return h(
    'div',
    { class: `c-row ${stack ? 'c-row--stack' : ''}`.trim() },
    h(
      'div',
      { class: 'c-row__label' },
      label,
      hint ? h('span', { class: 'c-row__hint' }, hint) : null
    ),
    control ? h('div', { class: 'c-row__control' }, control) : null
  );
}

function block(title, ...children) {
  return h(
    'section',
    { class: 'c-ai-block' },
    title ? h('div', { class: 'c-section' }, title) : null,
    h('div', { class: 'c-card' }, children)
  );
}

function formatTime(at) {
  const date = new Date(Number(at) || Date.now());
  if (Number.isNaN(date.getTime())) return '';
  const now = new Date();
  const pad = (n) => String(n).padStart(2, '0');
  const clock = `${pad(date.getHours())}:${pad(date.getMinutes())}`;
  if (date.toDateString() === now.toDateString()) return clock;
  return `${date.getMonth() + 1}月${date.getDate()}日 ${clock}`;
}

/** 整篇 before/after 里取第一处差异附近 40 字，作为历史摘要 */
function diffSummary(before, after) {
  const a = String(before || '');
  const b = String(after || '');
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i += 1;
  const start = Math.max(0, i - 20);
  const slice = b.slice(start, start + 40).replace(/\s+/g, ' ').trim();
  return slice || b.slice(0, 40).replace(/\s+/g, ' ').trim();
}

/* ---------------- 页面 ---------------- */

export function createSettingsPage(host, ctx) {
  const config = loadConfig(ctx);

  let history = loadHistory(ctx);
  let testing = false;
  let testState = 'idle'; // idle | testing | ok | fail
  let testMessage = '';
  let disposed = false;

  const root = h('div', { class: 'c-ai-page' });
  const body = h('div', { class: 'c-ai-page__body' });
  root.append(body);
  host.append(root);

  /* ---------- 顶部 ---------- */

  const statusBadge = h('span', { class: 'c-badge' }, '未配置');
  const hero = h(
    'header',
    { class: 'c-ai-hero' },
    h(
      'div',
      { class: 'c-ai-hero__row' },
      h('div', { class: 'c-ai-hero__title' }, 'AI 设置与历史'),
      statusBadge
    ),
    h(
      'div',
      { class: 'c-ai-hero__sub' },
      '对话在右侧栏的「AI」标签里：打开就能问，不需要先选文字。这一页只放连接设置和可回退的写入记录。'
    ),
    h(
      'div',
      { class: 'c-ai-hero__actions' },
      button('打开对话', {
        variant: 'primary',
        size: 'sm',
        onClick: () => ctx.ui.openSidebar('ai.chat'),
      }),
      h('span', { class: 'c-ai-muted' }, canStream() ? '桌面端走 Rust 流式' : '浏览器预览按一次性返回逐字显示')
    )
  );

  /* ---------- 连接 ---------- */

  const baseField = field({
    value: config.baseUrl,
    placeholder: 'https://api.deepseek.com/v1',
    onInput: (value) => {
      config.baseUrl = value;
      persistConfig();
      markUntested();
    },
  });

  const modelField = field({
    value: config.model,
    placeholder: 'deepseek-chat',
    onInput: (value) => {
      config.model = value;
      persistConfig();
      markUntested();
    },
  });

  const keyField = field({
    type: 'password',
    value: '',
    placeholder: config.apiKey ? `已保存 ${maskKey(config.apiKey)}` : 'sk-…',
    onInput: (value) => {
      config.apiKey = value.trim();
      persistConfig();
      updateConnection();
      markUntested();
    },
  });

  const keyHint = h('span', {}, '');

  const tempField = field({
    type: 'number',
    value: String(config.temperature),
    min: '0',
    max: '2',
    step: '0.1',
    width: '96px',
    onInput: (value) => {
      const n = Number(value);
      config.temperature = Number.isFinite(n) ? Math.min(2, Math.max(0, n)) : config.temperature;
      persistConfig();
    },
  });

  const testBtn = button('测试连接', {
    size: 'sm',
    onClick: () => runTest(),
  });

  const testLine = h('div', { class: 'c-ai-status' }, '还没有测试过连接。');

  const connectionCard = block(
    '连接',
    row('baseURL', 'OpenAI 兼容接口的根地址，末尾不写 /chat/completions', baseField, true),
    row('模型', '例如 deepseek-chat / gpt-4o-mini', modelField, true),
    row('API Key', keyHint, keyField, true),
    row('温度', '0 更稳定，1 更发散', tempField),
    h('div', { class: 'c-ai-test' }, testBtn, testLine),
    h(
      'div',
      { class: 'c-ai-warn' },
      '设置存在 .inkdown/plugins/ai/data.json，API Key 以明文写入。别把这个仓库同步到公开位置。'
    )
  );

  /* ---------- 历史 ---------- */

  const historyList = h('div', { class: 'c-ai-history' });
  const historyCard = block('写入历史', historyList);

  body.append(hero, connectionCard, historyCard);

  /* ---------- 状态 ---------- */

  const persistConfig = () => saveConfig(ctx, config);

  function markUntested() {
    if (testState === 'ok' || testState === 'fail') {
      testState = 'idle';
      testMessage = '';
    }
    updateConnection();
  }

  function updateConnection() {
    const hasKey = Boolean(config.apiKey.trim());
    if (testState === 'ok') {
      statusBadge.textContent = '连接正常';
      statusBadge.className = 'c-badge c-badge--accent';
    } else if (testState === 'fail') {
      statusBadge.textContent = '连接异常';
      statusBadge.className = 'c-badge c-badge--danger';
    } else if (hasKey) {
      statusBadge.textContent = '已配置';
      statusBadge.className = 'c-badge';
    } else {
      statusBadge.textContent = '未配置';
      statusBadge.className = 'c-badge';
    }

    keyHint.textContent = hasKey
      ? `已保存 ${maskKey(config.apiKey)}。重新输入会覆盖，清空输入框会删除。`
      : '还没有保存 Key。';
    keyField.input.placeholder = hasKey ? `已保存 ${maskKey(config.apiKey)}` : 'sk-…';

    testBtn.disabled = testing || !hasKey || !config.baseUrl.trim() || !config.model.trim();

    if (testState === 'testing') {
      testBtn.textContent = '正在测试…';
      testLine.textContent = '正在请求模型…';
      testLine.dataset.tone = '';
    } else if (testState === 'ok') {
      testBtn.textContent = '重新测试';
      testLine.textContent = testMessage || '连接正常';
      testLine.dataset.tone = 'ok';
    } else if (testState === 'fail') {
      testBtn.textContent = '重试';
      testLine.textContent = testMessage || '连接失败';
      testLine.dataset.tone = 'danger';
    } else {
      testBtn.textContent = '测试连接';
      testLine.textContent = hasKey ? '还没有测试过连接。' : '填写 API Key 后可测试。';
      testLine.dataset.tone = '';
    }
  }

  async function runTest() {
    if (testing) return;
    testing = true;
    testState = 'testing';
    testMessage = '';
    updateConnection();

    const res = await testConnection({ ctx, config });

    testing = false;
    if (res.ok) {
      testState = 'ok';
      testMessage = `连接正常 · 模型 ${res.model}`;
    } else {
      testState = 'fail';
      testMessage = `连接失败：${res.message}`;
    }
    updateConnection();
  }

  /* ---------- 历史列表 ---------- */

  function renderHistory() {
    if (disposed) return;
    history = loadHistory(ctx);
    historyList.replaceChildren();

    if (!history.length) {
      historyList.append(
        h(
          'div',
          { class: 'c-ai-empty' },
          h('div', { class: 'c-ai-empty__title' }, '还没有写入记录'),
          h(
            'div',
            { class: 'c-ai-empty__desc' },
            '在对话里点「插入到光标」或「替换选区」后，这里会保留整篇前后稿，随时可以回退。'
          )
        )
      );
      return;
    }

    for (const entry of history) {
      const reverted = Boolean(entry.revertedAt);
      const restore = button(reverted ? '已回退' : '回退', {
        size: 'sm',
        variant: reverted ? 'plain' : '',
        disabled: reverted,
        title: reverted ? '这条记录已经回退过了' : '恢复写入前的正文',
        onClick: async () => {
          await restoreEntry(ctx, entry);
        },
      });

      historyList.append(
        h(
          'div',
          { class: `c-ai-history__item${reverted ? ' is-reverted' : ''}` },
          h(
            'div',
            { class: 'c-ai-history__main' },
            h(
              'div',
              { class: 'c-ai-history__meta' },
              h('span', { class: 'c-ai-history__time' }, formatTime(entry.at)),
              h('span', { class: 'c-ai-history__title' }, entry.title || entry.path),
              h('span', { class: 'c-ai-badge' }, entry.instruction || '写入')
            ),
            h('div', { class: 'c-ai-history__summary' }, diffSummary(entry.before, entry.after))
          ),
          restore
        )
      );
    }
  }

  const offHistory = onHistoryChange(() => renderHistory());

  updateConnection();
  renderHistory();

  return {
    destroy() {
      if (disposed) return;
      disposed = true;
      offHistory?.();
      root.remove();
    },
  };
}
