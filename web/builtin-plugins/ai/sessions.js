/* ==========================================================================
   AI 助手 · 按笔记隔离的会话
   每篇笔记一份消息列表，存 ctx.storage（落盘到插件目录的 data.json）。

   容量边界是写死的：单篇最多 40 条，全局最多 20 篇；超出后按 updatedAt
   做 FIFO 淘汰。聊天记录会越积越大，没有上限的话 data.json 迟早拖垮启动。
   ========================================================================== */

const KEY = 'sessions';
const MAX_MESSAGES = 40;
const MAX_NOTES = 20;

const watchers = new Set();

export function onSessionsChange(fn) {
  watchers.add(fn);
  return () => watchers.delete(fn);
}

function notify() {
  for (const fn of [...watchers]) {
    try {
      fn();
    } catch (err) {
      console.warn('[ai] 会话监听器抛错', err);
    }
  }
}

function isMessage(value) {
  return (
    value &&
    typeof value === 'object' &&
    typeof value.role === 'string' &&
    (value.role === 'user' || value.role === 'assistant') &&
    typeof value.content === 'string'
  );
}

function sanitizeMessage(value) {
  const msg = {
    id: String(value.id || ''),
    role: value.role,
    content: String(value.content || ''),
    at: Number(value.at) || Date.now(),
  };
  if (value.error) msg.error = String(value.error);
  return msg;
}

function readAll(ctx) {
  const raw = ctx.storage.get(KEY, {});
  return raw && typeof raw === 'object' && !Array.isArray(raw) ? raw : {};
}

function prune(all) {
  const entries = Object.entries(all)
    .filter(([path, session]) => Boolean(path) && session && Array.isArray(session.messages))
    .sort((a, b) => Number(b[1].updatedAt || 0) - Number(a[1].updatedAt || 0));

  const out = {};
  for (const [path, session] of entries.slice(0, MAX_NOTES)) out[path] = session;
  return out;
}

function writeAll(ctx, all) {
  ctx.storage.set(KEY, prune(all));
  ctx.storage.flush?.();
  notify();
}

export function getSession(ctx, path) {
  if (!path) return [];
  const session = readAll(ctx)[path];
  if (!session || !Array.isArray(session.messages)) return [];
  return session.messages
    .filter(isMessage)
    .map(sanitizeMessage)
    .slice(-MAX_MESSAGES);
}

/**
 * 覆盖一篇笔记的消息。空内容且没有 error 的助手占位消息不落盘 ——
 * 流式过程中关掉面板，不该留下一条空气泡。
 */
export function saveSession(ctx, path, messages) {
  if (!path) return;
  const all = readAll(ctx);
  const clean = (Array.isArray(messages) ? messages : [])
    .filter(isMessage)
    .filter((m) => m.role === 'user' || String(m.content || '').trim() || m.error)
    .map(sanitizeMessage)
    .slice(-MAX_MESSAGES);

  // 从中间的助手消息开始截断时，保证列表仍以用户发言开头
  while (clean.length && clean[0].role === 'assistant') clean.shift();

  if (!clean.length) {
    delete all[path];
  } else {
    all[path] = { messages: clean, updatedAt: Date.now() };
  }
  writeAll(ctx, all);
}

export function clearSession(ctx, path) {
  if (!path) return;
  const all = readAll(ctx);
  delete all[path];
  writeAll(ctx, all);
}

export function makeMessage(role, content) {
  const uuid = globalThis.crypto?.randomUUID?.();
  return {
    id: uuid || `ai-${Date.now()}-${Math.random().toString(16).slice(2)}`,
    role,
    content: String(content || ''),
    at: Date.now(),
  };
}

/** 命令面板「新建对话」用：拿到当前笔记路径 */
export function currentPath(ctx) {
  return ctx.editor.currentNote()?.path || '';
}
