/* ==========================================================================
   Inkdown · UI WASM 构建脚本
   将 inkdown-ui 编译为 wasm32-unknown-unknown 并打包到 ui/dist/
   ========================================================================== */

import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const UI_DIR = join(ROOT, 'ui');
const DIST_DIR = join(UI_DIR, 'dist');
const PKG_DIR = join(DIST_DIR, 'pkg');

const isDev = process.argv.includes('--dev');
const profile = isDev ? 'debug' : 'release';

const run = (cmd, args) => {
  execFileSync(cmd, args, { cwd: ROOT, stdio: 'inherit' });
};

console.log(`[build-ui] 编译 inkdown-ui (${profile})…`);
const cargoArgs = ['build', '-p', 'inkdown-ui', '--target', 'wasm32-unknown-unknown'];
if (!isDev) cargoArgs.push('--release');
run('cargo', cargoArgs);

const wasmPath = join(ROOT, 'target', 'wasm32-unknown-unknown', profile, 'inkdown_ui.wasm');
if (!existsSync(wasmPath)) {
  throw new Error(`找不到编译产物：${wasmPath}`);
}

rmSync(PKG_DIR, { recursive: true, force: true });
mkdirSync(PKG_DIR, { recursive: true });

console.log('[build-ui] 生成 wasm-bindgen 胶水…');
run('wasm-bindgen', [
  '--target',
  'web',
  '--no-typescript',
  '--remove-name-section',
  '--remove-producers-section',
  '--out-dir',
  PKG_DIR,
  wasmPath,
]);

for (const file of ['index.html', 'wallpaper.js', 'inkdown-app.css']) {
  cpSync(join(UI_DIR, file), join(DIST_DIR, file));
}
if (existsSync(join(UI_DIR, 'assets'))) {
  cpSync(join(UI_DIR, 'assets'), join(DIST_DIR, 'assets'), { recursive: true });
}

console.log(`[build-ui] 完成 → ${DIST_DIR}`);
