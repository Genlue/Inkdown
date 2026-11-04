// 静止态截图：对照「关闭交互降级/节流」的副本 与 当前代码
import { spawn } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { launch, goto, evaluate } from './cdp.mjs';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const OUT = fileURLToPath(new URL('./out/', import.meta.url));
mkdirSync(OUT, { recursive: true });

const temp = mkdtempSync(join(tmpdir(), 'inkdown-before-web-'));
cpSync(join(ROOT, 'web'), temp, { recursive: true });

// 把副本的「进入交互降级」变成空操作 —— 等价于改前：始终完整滤镜、每次尺寸变化立即重建
const lensPath = join(temp, 'scripts/core/glass-lens.js');
const src = readFileSync(lensPath, 'utf8');
const patched = src.replace(
  'export function beginGlassInteraction() {',
  'export function beginGlassInteraction() {\n  return; // 截图副本：关闭交互降级'
);
if (patched === src) throw new Error('副本补丁没打上');
writeFileSync(lensPath, patched);

const server = spawn('python', ['-m', 'http.server', '8898', '--directory', temp], { stdio: 'ignore' });
await new Promise((r) => setTimeout(r, 900));

const setup = `(async () => {
  document.documentElement.style.setProperty('--ink-list-w', '284px');
  const d = document.querySelector('.c-drawer.ink-recent-drawer');
  d.dataset.open = '1';
  d.style.setProperty('--dw-p', '1');
  await new Promise((r) => setTimeout(r, 700));
  const btn = [...document.querySelectorAll('button')].find((b) => b.getAttribute('aria-label') === '更多操作');
  btn.click();
  await new Promise((r) => setTimeout(r, 600));
  await document.fonts.ready;
  return true;
})()`;

const clips = `(() => {
  const rect = (sel) => {
    const el = document.querySelector(sel);
    if (!el) return null;
    const b = el.getBoundingClientRect();
    return { x: Math.round(b.left), y: Math.round(b.top), width: Math.round(b.width), height: Math.round(b.height) };
  };
  return {
    drawer: rect('.c-drawer.ink-recent-drawer .c-drawer__sheet'),
    capsule: rect('.c-topbar__breadcrumb'),
    menu: rect('.c-menu'),
  };
})()`;

async function capture(url, tag) {
  const h = await launch();
  try {
    await goto(h.cdp, url);
    await evaluate(h.cdp, setup);
    const c = await evaluate(h.cdp, clips);
    const out = {};
    for (const [name, clip] of Object.entries(c)) {
      if (!clip) throw new Error(`${tag} 找不到 ${name}`);
      const { data } = await h.cdp.send('Page.captureScreenshot', { format: 'png', clip: { ...clip, scale: 1 } });
      writeFileSync(join(OUT, `${tag}-${name}.png`), Buffer.from(data, 'base64'));
      out[name] = clip;
    }
    return out;
  } finally {
    await h.close();
  }
}

try {
  const before = await capture('http://127.0.0.1:8898/index.html', 'before');
  const after = await capture('http://127.0.0.1:8899/index.html', 'after');
  writeFileSync(join(OUT, 'clips.json'), JSON.stringify({ before, after }, null, 2));
  console.log(JSON.stringify({ before, after }, null, 2));
} finally {
  server.kill();
  try { rmSync(temp, { recursive: true, force: true }); } catch {}
}
