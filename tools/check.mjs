/* ==========================================================================
   Inkdown · 静态与编译自检
   1. 检查四处版本号一致（package.json / ui/Cargo.toml / src-tauri/Cargo.toml / tauri.conf.json）
   2. 检查 ui/inkdown-app.css 的花括号、注释闭合，且禁止字面色值（#hex / rgb / hsl）
   3. 检查代码库内无残留 AI 模块或命令
   4. 执行 Rust 端 cargo check 与单元测试
   ========================================================================== */

import { readFileSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const failures = [];

const read = (rel) => readFileSync(join(ROOT, rel), 'utf8');
const readJson = (rel) => JSON.parse(read(rel));

/* ---------------- 1. 版本号一致 ---------------- */

const versions = [];
try {
  versions.push(['package.json', readJson('package.json').version]);
} catch (err) {
  failures.push(`package.json: 读取失败 (${err.message})`);
}

for (const rel of ['ui/Cargo.toml', 'src-tauri/Cargo.toml']) {
  try {
    const raw = read(rel);
    const pkg = raw.split(/^\[/m).find((c) => c.startsWith('package]')) || '';
    const m = pkg.match(/^version\s*=\s*"([^"]+)"/m);
    if (m) versions.push([rel, m[1]]);
    else failures.push(`${rel}: [package] 段缺少 version`);
  } catch (err) {
    failures.push(`${rel}: 读取失败 (${err.message})`);
  }
}

try {
  versions.push(['src-tauri/tauri.conf.json', readJson('src-tauri/tauri.conf.json').version]);
} catch (err) {
  failures.push(`src-tauri/tauri.conf.json: 读取失败 (${err.message})`);
}

const version = versions[0]?.[1] || '?';
if (versions.some(([, v]) => v !== version)) {
  const detail = versions.map(([f, v]) => `${f}=${v}`).join('，');
  failures.push(`版本号不一致：${detail}`);
}

/* ---------------- 2. CSS 完整性与纯令牌约束 ---------------- */

const cssPath = 'ui/inkdown-app.css';
const cssRaw = read(cssPath);
const openComments = (cssRaw.match(/\/\*/g) || []).length;
const closeComments = (cssRaw.match(/\*\//g) || []).length;
if (openComments !== closeComments) {
  failures.push(`${cssPath}: 注释未闭合 (/* ${openComments}, */ ${closeComments})`);
}

const cssBody = cssRaw.replace(/\/\*[\s\S]*?\*\//g, '');
let depth = 0;
for (const ch of cssBody) {
  if (ch === '{') depth += 1;
  else if (ch === '}') depth -= 1;
  if (depth < 0) {
    failures.push(`${cssPath}: 多余的 }`);
    break;
  }
}
if (depth > 0) {
  failures.push(`${cssPath}: 有 ${depth} 个 { 未闭合`);
}

cssBody.split('\n').forEach((line, i) => {
  if (/#[0-9a-f]{3,8}\b|rgba?\(|hsla?\(/i.test(line)) {
    failures.push(`${cssPath}:${i + 1} 出现字面色值，必须改用 GlassDesign var(--*) 令牌`);
  }
});

/* ---------------- 3. 确认已移除 AI ---------------- */

for (const rel of ['src-tauri/src/lib.rs', 'src-tauri/Cargo.toml', 'ui/src/lib.rs']) {
  const content = read(rel);
  if (/\bai_stream\b|\bai_cancel\b|\bAiRuns\b/i.test(content)) {
    failures.push(`${rel}: 检测到残留 AI 命令或结构`);
  }
}

/* ---------------- 4. Rust 检查与测试 ---------------- */

const run = (cmd, args) => {
  execFileSync(cmd, args, { cwd: ROOT, stdio: 'inherit' });
};

try {
  mkdirSync(join(ROOT, 'ui', 'dist'), { recursive: true });
  run('cargo', ['check', '-p', 'inkdown']);
  run('cargo', ['test', '-p', 'inkdown', '--lib']);
  run('cargo', ['check', '-p', 'inkdown-ui', '--target', 'wasm32-unknown-unknown']);
  run('cargo', ['test', '-p', 'inkdown-ui', '--lib']);
} catch (err) {
  failures.push(`Rust 编译或单元测试失败`);
}

console.log(`\nInkdown 自检 · v${version}`);
if (failures.length) {
  for (const f of failures) console.log(`  ✗ ${f}`);
  process.exit(1);
}
console.log('  ✓ 版本号、CSS 令牌、无 AI 残留、Rust 编译与单测全部通过\nPASS');
