// 生成「改前等价」副本：关掉交互降级、ResizeObserver 恢复为每次回调立即重建，
// 用同一套 measure.mjs 跑出可对比的数字。
import { spawn } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const temp = mkdtempSync(join(tmpdir(), 'inkdown-before-echo-'));
cpSync(join(ROOT, 'web'), temp, { recursive: true });

const p = join(temp, 'scripts/core/glass-lens.js');
let s = readFileSync(p, 'utf8');
s = s.replace(
  'export function beginGlassInteraction() {',
  'export function beginGlassInteraction() {\n  return; // 改前等价副本'
);
s = s.replace('new ResizeObserver(() => requestRefresh())', 'new ResizeObserver(() => refresh())');
writeFileSync(p, s);

const server = spawn('python', ['-m', 'http.server', '8898', '--directory', temp], { stdio: 'ignore' });
await new Promise((r) => setTimeout(r, 900));

const run = spawn(process.execPath, ['tools/perf/measure.mjs', 'before-echo', 'headed'], {
  cwd: ROOT,
  stdio: 'inherit',
  env: { ...process.env, PERF_URL: 'http://127.0.0.1:8898/index.html' },
});
const code = await new Promise((r) => run.on('exit', r));
server.kill();
try { rmSync(temp, { recursive: true, force: true }); } catch {}
process.exit(code);
