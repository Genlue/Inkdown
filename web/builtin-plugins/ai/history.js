/* ==========================================================================
   AI 写作助手 · 改写历史与回退
   每条记录保存的是「接受改写前 / 后的整篇正文」，所以回退不依赖 diff，
   也不依赖笔记之后有没有移动。上限 20 条，超出丢最旧。

   安全线：
   - 只回退到当前打开的那篇笔记（setContent 作用于当前编辑器）；
   - 当前正文 !== 记录里的 after 时，必须让用户确认「会覆盖这些改动」。
   ========================================================================== */

const KEY = 'history';
const LIMIT = 20;

/** 页面与命令共享同一个模块实例，改完历史后喊一声让页面重画 */
const watchers = new Set();

export function onHistoryChange(fn) {
  watchers.add(fn);
  return () => watchers.delete(fn);
}

function notify() {
  for (const fn of [...watchers]) {
    try {
      fn();
    } catch (err) {
      console.warn('[ai] 历史监听器抛错', err);
    }
  }
}

function isEntry(value) {
  return (
    value &&
    typeof value === 'object' &&
    typeof value.id === 'string' &&
    typeof value.path === 'string' &&
    typeof value.before === 'string' &&
    typeof value.after === 'string'
  );
}

export function loadHistory(ctx) {
  const raw = ctx.storage.get(KEY, []);
  if (!Array.isArray(raw)) return [];
  return raw.filter(isEntry).slice(0, LIMIT);
}

function save(ctx, list) {
  ctx.storage.set(KEY, list.slice(0, LIMIT));
  ctx.storage.flush?.();
  notify();
}

export function recordRewrite(ctx, entry) {
  const list = loadHistory(ctx);
  list.unshift({ ...entry, at: entry.at || Date.now() });
  save(ctx, list);
  return list[0];
}

function markReverted(ctx, id) {
  const list = loadHistory(ctx);
  const hit = list.find((item) => item.id === id);
  if (hit) hit.revertedAt = Date.now();
  save(ctx, list);
}

/* ---------------- 确认框 ---------------- */

/**
 * 宿主 ctx.ui.dialog 直接转 kit 的 Dialog：按钮由我们自己给。
 * 这里不 import kit —— 插件只能相对导入自己目录里的文件。
 */
function confirmRestore(ctx, entry) {
  return new Promise((resolve) => {
    let done = false;
    const finish = (value) => {
      if (done) return;
      done = true;
      resolve(value);
    };

    const cancel = document.createElement('button');
    cancel.type = 'button';
    cancel.className = 'c-btn c-btn--plain';
    cancel.textContent = '取消';
    cancel.addEventListener('click', () => {
      finish(false);
      ctl.close();
    });

    const ok = document.createElement('button');
    ok.type = 'button';
    ok.className = 'c-btn c-btn--danger';
    ok.textContent = '覆盖并回退';
    ok.addEventListener('click', () => {
      finish(true);
      ctl.close();
    });

    const ctl = ctx.ui.dialog({
      title: '这篇笔记之后又改过',
      desc:
        `《${entry.title || entry.path}》的正文与当时接受改写后的版本不一致。` +
        '回退会覆盖你在那之后做的改动，且无法撤销。',
      icon: 'rotate-ccw',
      tone: 'danger',
      actions: [cancel, ok],
      onClose: () => finish(false),
    });
  });
}

/* ---------------- 回退 ---------------- */

/**
 * 回退一条记录。
 * @returns {Promise<boolean>} 是否真的恢复了正文
 */
export async function restoreEntry(ctx, entry) {
  if (!entry) {
    ctx.ui.toast('没有可回退的改写', { icon: 'info' });
    return false;
  }

  const note = ctx.editor.currentNote();
  if (!note) {
    ctx.ui.toast('先打开一篇笔记再回退', { icon: 'info' });
    return false;
  }
  if (note.path !== entry.path) {
    ctx.ui.toast(`请先打开《${entry.title || entry.path}》再回退`, { icon: 'info' });
    return false;
  }

  if (note.content !== entry.after) {
    const ok = await confirmRestore(ctx, entry);
    if (!ok) return false;
  }

  ctx.editor.setContent(entry.before);
  markReverted(ctx, entry.id);
  ctx.ui.toast('已回退到改写前', { icon: 'rotate-ccw', duration: 2200 });
  return true;
}

/** 命令「AI: 回退上一次改写」走这一条 */
export async function restoreLatest(ctx) {
  const list = loadHistory(ctx);
  if (!list.length) {
    ctx.ui.toast('还没有可回退的改写', { icon: 'info' });
    return false;
  }
  const latest = list[0];
  if (latest.revertedAt) {
    ctx.ui.toast('最近一条改写已经回退过了', { icon: 'info' });
    return false;
  }
  return restoreEntry(ctx, latest);
}
