/* ==========================================================================
   Inkdown · 静态自检
   没有构建步骤，就没有编译器兜底。这个脚本只做确定性的事：

   1. 每个 ES 模块都能被解析（node --check）
   2. 每份样式表的花括号 / 注释是闭合的
   3. 同一份文件里没有重复的章节标题（分栏拖拽那次就是这么埋进去的）
   4. 三处版本号一致（package.json / Cargo.toml / tauri.conf.json）

   用法：npm run check
   ========================================================================== */

import { readFileSync, writeFileSync, mkdtempSync, rmSync, readdirSync, statSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, relative, extname, dirname, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const WEB = join(ROOT, 'web');

const walk = (dir, out = []) => {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) walk(full, out);
    else out.push(full);
  }
  return out;
};

const files = walk(WEB);
const jsFiles = files.filter((f) => extname(f) === '.js');
const cssFiles = files.filter((f) => extname(f) === '.css');

const failures = [];
const warnings = [];

/* ---------------- 1. 模块语法 ---------------- */

// node --check 只认 .mjs/.cjs 之外的显式模块类型，复制一份再查
const tmp = mkdtempSync(join(tmpdir(), 'inkdown-check-'));
for (const file of jsFiles) {
  const rel = relative(ROOT, file);
  const copy = join(tmp, `${rel.replace(/[\\/]/g, '_')}.mjs`);
  try {
    writeFileSync(copy, readFileSync(file));
    execFileSync(process.execPath, ['--check', copy], { stdio: 'pipe' });
  } catch (err) {
    failures.push(`${rel}: 语法错误\n${String(err.stderr || err.message).trim()}`);
  }
}
rmSync(tmp, { recursive: true, force: true });

/* ---------------- 2. 样式表闭合 ---------------- */

const stripComments = (text) => text.replace(/\/\*[\s\S]*?\*\//g, '');

for (const file of cssFiles) {
  const rel = relative(ROOT, file);
  const raw = readFileSync(file, 'utf8');

  const open = (raw.match(/\/\*/g) || []).length;
  const close = (raw.match(/\*\//g) || []).length;
  if (open !== close) failures.push(`${rel}: 注释没有闭合（/* ${open} 个，*/ ${close} 个）`);

  const body = stripComments(raw);
  let depth = 0;
  let broke = false;
  for (const ch of body) {
    if (ch === '{') depth += 1;
    else if (ch === '}') depth -= 1;
    if (depth < 0) {
      failures.push(`${rel}: 多了一个 }`);
      broke = true;
      break;
    }
  }
  if (!broke && depth !== 0) failures.push(`${rel}: 还有 ${depth} 个 { 没闭合`);
}

/* ---------------- 2.5 未定义令牌 + 颜色兜底 ---------------- */

// 最难查的一类：令牌压根没定义，兜底又是写死的深色值。
// 浅色主题下整块发黑 —— Ctrl+P 面板和 Ctrl+F 查找栏都栽在这上面。
// 只报「从未定义过」的令牌；有定义的令牌带兜底（比如 --tl-rim, #f2001b）是正常的。

const definedTokens = new Set();
for (const file of cssFiles) {
  for (const m of readFileSync(file, 'utf8').matchAll(/(--[a-z0-9-]+)\s*:/g)) {
    definedTokens.add(m[1]);
  }
}

// 运行时由 JS 写进去的令牌（主题光学参数、栏宽、层级深度…）不在 CSS 里定义，不算缺失
for (const file of jsFiles) {
  const text = readFileSync(file, 'utf8');
  for (const m of text.matchAll(/setProperty\(\s*['"](--[a-z0-9-]+)['"]/g)) definedTokens.add(m[1]);
  // 模板字符串里拼出来的声明（theme.js 的 --lg-coating-color 就长这样）
  for (const m of text.matchAll(/(--[a-z0-9-]+)\s*:/g)) definedTokens.add(m[1]);
}

for (const file of cssFiles) {
  const rel = relative(ROOT, file);
  const lines = stripComments(readFileSync(file, 'utf8')).split('\n');
  lines.forEach((line, i) => {
    for (const m of line.matchAll(/var\((--[a-z0-9-]+)\s*,\s*([^)]*(?:\([^)]*\)[^)]*)*)\)/g)) {
      const [, name, fallback] = m;
      if (definedTokens.has(name)) continue;
      if (!/#[0-9a-f]{3,8}\b|rgba?\(|hsla?\(/i.test(fallback)) continue;
      warnings.push(`${rel}:${i + 1} 用到未定义的令牌 ${name}，兜底是写死的颜色`);
    }
  });
}

/* ---------------- 3.5 版本号一致 ---------------- */

// 版本号散在三处，改一处忘两处是最常见的低级错。
// 规则（README「版本策略」）：功能更新只动第三位，minor / major 需要所有者明确许可。
const versionSources = [];
const readJson = (rel) => JSON.parse(readFileSync(join(ROOT, rel), 'utf8'));

try {
  versionSources.push(['package.json', readJson('package.json').version]);
} catch (err) {
  failures.push(`package.json: 读不出 version（${err.message}）`);
}

try {
  const cargo = readFileSync(join(ROOT, 'src-tauri/Cargo.toml'), 'utf8');
  // 只看 [package] 段：依赖里也有一堆 version
  const pkg = cargo.split(/^\[/m).find((chunk) => chunk.startsWith('package]')) || '';
  const m = pkg.match(/^version\s*=\s*"([^"]+)"/m);
  if (m) versionSources.push(['src-tauri/Cargo.toml', m[1]]);
  else failures.push('src-tauri/Cargo.toml: [package] 段没有 version');
} catch (err) {
  failures.push(`src-tauri/Cargo.toml: 读取失败（${err.message}）`);
}

try {
  versionSources.push(['src-tauri/tauri.conf.json', readJson('src-tauri/tauri.conf.json').version]);
} catch (err) {
  failures.push(`src-tauri/tauri.conf.json: 读不出 version（${err.message}）`);
}

const version = versionSources[0]?.[1] || '?';
const drifted = versionSources.filter(([, v]) => v !== version);
if (drifted.length) {
  const detail = versionSources.map(([f, v]) => `${f}=${v}`).join('，');
  failures.push(`版本号不一致：${detail}`);
}

/* ---------------- 4. 重复章节 ---------------- */

for (const file of jsFiles) {
  const rel = relative(ROOT, file);
  const text = readFileSync(file, 'utf8');
  const seen = new Map();
  for (const m of text.matchAll(/\/\* -{8,} (.+?) -{8,}/g)) {
    const title = m[1].trim();
    const line = text.slice(0, m.index).split('\n').length;
    if (seen.has(title)) warnings.push(`${rel}: 章节「${title}」重复（第 ${seen.get(title)} 行与第 ${line} 行）`);
    else seen.set(title, line);
  }
}

/* ---------------- 输出 ---------------- */

const ok = (label) => console.log(`  \u2713 ${label}`);

console.log(`Inkdown 自检 · v${version} · ${jsFiles.length} 个模块 / ${cssFiles.length} 份样式\n`);

if (!failures.length) ok('语法、括号、版本全部通过');
else for (const f of failures) console.log(`  \u2717 ${f}`);

for (const w of warnings) console.log(`  ! ${w}`);

console.log('');
if (failures.length) {
  console.log(`FAIL · ${failures.length} 项`);
  process.exit(1);
}
console.log(warnings.length ? `PASS · ${warnings.length} 条提醒` : 'PASS');
