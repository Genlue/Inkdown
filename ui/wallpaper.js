/* --------------------------------------------------------------------------
   由 glass-design-host 提供的壳层与壁纸接线脚本。
   跑在 Tauri 里（window.__TAURI__ 存在）→ 在 <html> 上写 data-shell="host"，
   去掉浏览器预览用的 10px 外边距（.l-app inset: 0、方角、无投影）。
   -------------------------------------------------------------------------- */
const wp = document.getElementById('wallpaper');
if (wp && (!wp.dataset.source || wp.dataset.source === 'host')) {
  wp.dataset.source = 'demo';
}

if (window.__TAURI__) {
  document.documentElement.dataset.shell = 'host';
}
