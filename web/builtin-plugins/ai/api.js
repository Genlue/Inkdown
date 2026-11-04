/* ==========================================================================
   AI 助手 · 请求层
   三件事：
   1. 读写连接设置（baseUrl / model / key / temperature）；
   2. 把「当前笔记 + 选区 + 引用笔记 + 多轮历史」组装成 chat 消息；
   3. 发一次流式请求 —— 桌面端走 Rust 的 ai_stream / ai_cancel，
      浏览器预览退化成一次性返回，再逐字交给 UI。

   桌面端为什么必须走 Rust：webview 里的 fetch 受 CORS 约束，而模型服务通常
   不会给 file:// 或 tauri:// 开跨域。Rust 侧只做转发与 SSE 解析，不碰仓库。
   ========================================================================== */

export const DEFAULT_CONFIG = {
  baseUrl: 'https://api.deepseek.com/v1',
  model: 'deepseek-chat',
  apiKey: '',
  temperature: 0.7,
};

/** 当前笔记的硬上限：超过就从尾部截断，UI 上明示「已截断」 */
export const NOTE_LIMIT = 6000;
/** 单篇 @ 引用笔记的上限 */
export const REF_LIMIT = 2000;

/**
 * 系统提示决定它是「通用助手」而不是「改写按钮」。
 * 刻意写清楚：可以回答、解释、发散去写，不要主动把用户按到润色模板里。
 */
const SYSTEM_PROMPT = [
  '你是 Inkdown 笔记应用右侧栏里的通用 AI 助手。你正和用户一起看一篇 Markdown 笔记，笔记内容会附在下面（可能只是一部分）。',
  '',
  '你不是“润色 / 精简 / 续写”按钮，也不要假设用户只想改文字。用户的请求可能是：',
  '- 问这篇笔记讲了什么、某个概念是什么意思；',
  '- 帮他起提纲、起草一段内容、对比两篇笔记、发散想法；',
  '- 也可能和当前笔记无关，就是随口问一个问题。',
  '',
  '回答要求：',
  '1. 用中文回答（用户要求别的语言时除外），观点明确，不绕圈子。',
  '2. 输出要能直接放进笔记：需要成段文字时用 Markdown（标题、列表、引用、代码块）；不要用“以下是……”“希望这对你有帮助”这类前言后语。',
  '3. 用户可能把你的一条回复一键插入或替换进笔记，所以不要写“改写后的内容如下”这种元话语，也不要把整段答案包进代码围栏（除非用户要的就是代码）。',
  '4. 上下文里有【用户当前选中的文字】时，那通常是这次要处理的重点；但用户没让你改写时，不要擅自只改它。',
  '5. 不确定的地方直说，不要编造笔记里没有的事实。',
].join('\n');

/* ---------------- 设置 ---------------- */

export function loadConfig(ctx) {
  const raw = ctx.storage.get('config', null);
  const data = raw && typeof raw === 'object' ? raw : {};
  return {
    baseUrl: String(data.baseUrl || DEFAULT_CONFIG.baseUrl),
    model: String(data.model || DEFAULT_CONFIG.model),
    apiKey: String(data.apiKey || ''),
    temperature: clampNumber(data.temperature, 0, 2, DEFAULT_CONFIG.temperature),
  };
}

/** 设置改动后，已经打开的对话面板要重新判断「有没有配置 Key」 */
const configWatchers = new Set();

export function onConfigChange(fn) {
  configWatchers.add(fn);
  return () => configWatchers.delete(fn);
}

function notifyConfig() {
  for (const fn of [...configWatchers]) {
    try {
      fn();
    } catch (err) {
      console.warn('[ai] 设置监听器抛错', err);
    }
  }
}

export function saveConfig(ctx, config) {
  ctx.storage.set('config', {
    baseUrl: config.baseUrl,
    model: config.model,
    apiKey: config.apiKey,
    temperature: config.temperature,
  });
  notifyConfig();
}

function clampNumber(value, min, max, fallback) {
  const n = Number(value);
  if (!Number.isFinite(n)) return fallback;
  return Math.min(max, Math.max(min, n));
}

export function maskKey(key) {
  const text = String(key || '');
  if (!text) return '';
  if (text.length <= 8) return `${text.slice(0, 2)}…`;
  return `${text.slice(0, 3)}…${text.slice(-4)}`;
}

/* ---------------- 上下文 ---------------- */

/** 从尾部截断，保留开头；返回是否截断过，供 UI 明示 */
export function truncateNote(content, limit = NOTE_LIMIT) {
  const text = String(content || '');
  if (text.length <= limit) return { text, truncated: false };
  return { text: text.slice(0, limit), truncated: true };
}

/** 引用笔记同样从尾部截断 */
export function truncateRef(content, limit = REF_LIMIT) {
  const text = String(content || '');
  if (text.length <= limit) return { text, truncated: false };
  return { text: text.slice(0, limit), truncated: true };
}

/**
 * 组装 system 内容。
 * 笔记全文放 system 而不是塞进最后一条 user：这样多轮追问时模型始终知道
 * 「我们正在看哪篇笔记」，而用户气泡里只有他自己写的那句话。
 */
export function buildSystemContent({ note, noteText = '', noteTruncated = false, selection = '', refs = [] }) {
  const parts = [SYSTEM_PROMPT];

  if (note) {
    const body = String(noteText || '').trim();
    parts.push(
      `【当前笔记】《${note.title || note.path}》${note.path ? `（${note.path}）` : ''}` +
        (body ? `\n${body}` : '\n（用户暂时移除了笔记正文，只保留了标题）') +
        (noteTruncated ? `\n…（笔记超过 ${NOTE_LIMIT} 字，已从尾部截断）` : '')
    );
  }

  const selected = String(selection || '').trim();
  if (selected) parts.push(`【用户当前选中的文字】\n${selected}`);

  for (const ref of refs) {
    if (!ref?.text?.trim()) continue;
    parts.push(
      `【引用的另一篇笔记】《${ref.title || ref.path}》${ref.path ? `（${ref.path}）` : ''}` +
        (ref.truncated ? `\n（超过 ${REF_LIMIT} 字，已从尾部截断）` : '') +
        `\n${ref.text}`
    );
  }

  return parts.join('\n\n');
}

/**
 * 组装这次请求的 messages。
 * 历史只带最近 24 条，且丢掉空内容与出错消息；system 每轮重算，
 * 所以用户中途改了笔记 / 换了选区，下一轮就能反映出来。
 */
export function buildChatMessages({ note, noteText, noteTruncated, selection, refs, history, userText }) {
  const system = buildSystemContent({ note, noteText, noteTruncated, selection, refs });

  const past = (Array.isArray(history) ? history : [])
    .filter((m) => (m.role === 'user' || m.role === 'assistant') && String(m.content || '').trim())
    .slice(-24)
    .map((m) => ({ role: m.role, content: String(m.content) }));

  return [
    { role: 'system', content: system },
    ...past,
    { role: 'user', content: String(userText || '') },
  ];
}

/* ---------------- 请求 ---------------- */

function endpointOf(baseUrl) {
  const base = String(baseUrl || '').trim().replace(/\/+$/, '');
  if (!/^https?:\/\//i.test(base)) {
    throw new Error('baseURL 需要以 http:// 或 https:// 开头');
  }
  return `${base}/chat/completions`;
}

function headersOf(config) {
  const headers = { 'Content-Type': 'application/json' };
  if (config.apiKey) headers.Authorization = `Bearer ${config.apiKey}`;
  return headers;
}

function bodyOf(config, messages, { stream, maxTokens } = {}) {
  const payload = {
    model: config.model,
    messages,
    temperature: clampNumber(config.temperature, 0, 2, DEFAULT_CONFIG.temperature),
    stream: Boolean(stream),
  };
  if (maxTokens) payload.max_tokens = maxTokens;
  return JSON.stringify(payload);
}

function newId() {
  const uuid = globalThis.crypto?.randomUUID?.();
  return uuid || `ai-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function tauriBridge() {
  const tauri = globalThis.__TAURI__;
  if (!tauri?.core?.invoke || !tauri?.event?.listen) return null;
  return tauri;
}

/** 桌面端能否走 Rust 流。UI 用它决定提示文案 */
export function canStream() {
  return Boolean(tauriBridge());
}

function normalizeError(err) {
  const message = String(err?.message || err || '未知错误').trim();
  return message || '未知错误';
}

/* ---------------- 流式请求 ---------------- */

/**
 * 发起一次聊天请求。
 * @returns {Promise<string>} 完整结果；中途取消会抛错，调用方按「已取消」处理。
 */
export async function streamChat({ ctx, config, messages, onDelta, signal, test = false }) {
  const bridge = tauriBridge();
  if (bridge) {
    return desktopStream({ bridge, config, messages, onDelta, signal, test });
  }
  return browserStream({ ctx, config, messages, onDelta, signal, test });
}

async function desktopStream({ bridge, config, messages, onDelta, signal, test }) {
  const id = newId();
  const url = endpointOf(config.baseUrl);
  const body = bodyOf(config, messages, { stream: !test, maxTokens: test ? 8 : undefined });

  let text = '';
  let settled = false;
  let failure = '';
  let resolveDone;
  const done = new Promise((resolve) => {
    resolveDone = resolve;
  });

  let offs = [];
  const cleanup = async () => {
    for (const off of offs.splice(0)) {
      try {
        await off();
      } catch {}
    }
    signal?.removeEventListener?.('abort', onAbort);
  };

  const settle = async (message) => {
    if (settled) return;
    settled = true;
    failure = message || '';
    await cleanup();
    resolveDone();
  };

  const listen = async (name, handler) => {
    const off = await bridge.event.listen(name, (event) => {
      const payload = event?.payload || {};
      if (payload.id !== id) return;
      handler(payload);
    });
    offs.push(off);
  };

  function onAbort() {
    bridge.core.invoke('ai_cancel', { id }).catch(() => {});
    settle('已取消');
  }

  await listen('ai://delta', (payload) => {
    if (settled) return;
    const piece = String(payload.text || '');
    if (!piece) return;
    text += piece;
    onDelta?.(piece, text);
  });
  await listen('ai://error', (payload) => {
    settle(String(payload.message || 'AI 请求失败'));
  });
  await listen('ai://done', () => {
    settle('');
  });

  if (signal?.aborted) {
    onAbort();
    await done;
    throw new Error('已取消');
  }
  signal?.addEventListener?.('abort', onAbort, { once: true });

  try {
    await bridge.core.invoke('ai_stream', {
      id,
      url,
      headers: headersOf(config),
      body,
    });
  } catch (err) {
    await settle(normalizeError(err));
    await done;
    throw new Error(failure);
  }

  await done;
  if (signal?.aborted || failure === '已取消') throw new Error('已取消');
  if (failure) throw new Error(failure);
  return text;
}

function parseSseText(raw) {
  let out = '';
  for (const line of String(raw || '').split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('data:')) continue;
    const payload = trimmed.slice(5).trim();
    if (!payload || payload === '[DONE]') continue;
    try {
      out += extractText(JSON.parse(payload));
    } catch {}
  }
  return out;
}

function extractText(value) {
  if (!value || typeof value !== 'object') return '';
  const read = (node) => {
    if (typeof node === 'string') return node;
    if (Array.isArray(node)) {
      return node.map((item) => (typeof item === 'string' ? item : item?.text || '')).join('');
    }
    return '';
  };

  const first = Array.isArray(value.choices) ? value.choices[0] : null;
  if (!first) return read(value.content);
  return read(first.delta?.content) || read(first.message?.content) || read(first.text) || '';
}

async function browserStream({ ctx, config, messages, onDelta, signal, test }) {
  const url = endpointOf(config.baseUrl);
  const body = bodyOf(config, messages, { stream: false, maxTokens: test ? 8 : undefined });

  const response = await ctx.net.fetch(url, {
    method: 'POST',
    headers: headersOf(config),
    body,
    signal,
  });

  const raw = await response.text();
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}：${String(raw).slice(0, 200)}`);
  }

  let text = '';
  try {
    text = extractText(JSON.parse(raw));
  } catch {
    // 有些兼容实现即使 stream:false 也回 SSE，这里再兜一次
    text = parseSseText(raw);
  }
  if (!text) {
    throw new Error('模型没有返回正文');
  }

  // 浏览器里没有逐块读取的通道，用「逐字吐字」保持同一套 UI 体验
  const chars = [...text];
  const step = chars.length > 600 ? 3 : 1;
  let shown = '';
  for (let i = 0; i < chars.length; i += step) {
    if (signal?.aborted) throw new Error('已取消');
    const piece = chars.slice(i, i + step).join('');
    shown += piece;
    onDelta?.(piece, shown);
    await delay(10);
  }
  return text;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/* ---------------- 连接测试 ---------------- */

export async function testConnection({ ctx, config }) {
  if (!config.apiKey) {
    return { ok: false, message: '还没有填写 API Key' };
  }

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 20000);

  try {
    const messages = [
      { role: 'system', content: '你是一个连通性测试助手。' },
      { role: 'user', content: '只回复 ok' },
    ];
    await streamChat({
      ctx,
      config,
      messages,
      signal: controller.signal,
      test: true,
    });
    return { ok: true, model: config.model };
  } catch (err) {
    if (controller.signal.aborted) {
      return { ok: false, message: '连接超时（20 秒）' };
    }
    return { ok: false, message: normalizeError(err) };
  } finally {
    clearTimeout(timer);
  }
}
