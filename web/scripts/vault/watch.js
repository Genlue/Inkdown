/* ==========================================================================
   Inkdown · Vault Watch
   桌面端实时监听仓库目录。Rust 侧已经按 250ms 合并过一批事件，
   这里再压 150ms 是双保险：批量操作（比如解压一个文件夹）会让
   Rust 的窗口切出好几批，前端再合一次就能少刷几遍列表。

   浏览器里没有文件监听，connect 直接返回空实现，调用方不用分支。
   ========================================================================== */

const EVENT = 'vault://fs-change';
const JS_DEBOUNCE = 150;

/**
 * @param {object} opts
 * @param {object} opts.vault       当前仓库（保留给未来的按仓库过滤）
 * @param {boolean} opts.isDesktop  是否跑在 Tauri 里
 * @param {(payload: { paths: string[], kind: string, truncated?: boolean }) => void} opts.onChange
 * @returns {() => void} 解绑函数
 */
export function connectVaultWatch({ vault, isDesktop, onChange }) {
  if (!isDesktop || !vault) return () => {};

  const listen = globalThis.__TAURI__?.event?.listen;
  if (typeof listen !== 'function') return () => {};

  let timer = 0;
  let disposed = false;
  let unlisten = null;

  const handle = (event) => {
    const payload = event?.payload ?? {};
    clearTimeout(timer);
    timer = setTimeout(() => {
      if (!disposed) onChange?.(payload);
    }, JS_DEBOUNCE);
  };

  listen(EVENT, handle)
    .then((off) => {
      // 注册是异步的：还没挂上就被解绑时，拿到句柄立刻撤掉
      if (disposed) off?.();
      else unlisten = off;
    })
    .catch((err) => console.warn('[watch] 监听仓库失败', err));

  return () => {
    disposed = true;
    clearTimeout(timer);
    unlisten?.();
    unlisten = null;
  };
}
