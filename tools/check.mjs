/* ==========================================================================
   Inkdown · 静态自检
   没有构建步骤，就没有编译器兜底。这个脚本只做三件确定性的事：

   1. 每个 ES 模块都能被解析（node --check）
   2. 每份样式表的花括号 / 注释是闭合的
   3. 同一份文件里没有重复的章节标题（分栏拖拽那次就是这么埋进去的）

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

/* ---------------- 3. 重复章节 ---------------- */

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

console.log(`Inkdown 自检 · ${jsFiles.length} 个模块 / ${cssFiles.length} 份样式\n`);

if (!failures.length) ok('语法与括号全部通过');
else for (const f of failures) console.log(`  \u2717 ${f}`);

for (const w of warnings) console.log(`  ! ${w}`);

console.log('');
if (failures.length) {
  console.log(`FAIL · ${failures.length} 项`);
  process.exit(1);
}
console.log(warnings.length ? `PASS · ${warnings.length} 条提醒` : 'PASS');
