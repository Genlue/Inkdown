/* ==========================================================================
   Inkdown · 插件模块加载器
   浏览器里没有模块解析器：`import './util.js'` 在 blob: URL 下找不到相对路径。
   所以把插件目录下的每个 .js 都单独做成 blob，再把源码里的相对说明符
   换成已经建好的 blob URL。

   顺序必须是「依赖先、自身后」—— 后序 DFS。
   环就建不出来（a 需要 b 的 URL，b 又需要 a 的），所以检测到环直接抛错，
   由宿主把错误记在插件上，不影响其它插件。

   只处理引号形式的相对说明符（'./x.js' / "../y.js" / import('./x.js')），
   不写完整 AST —— 插件源码里出现正则或字符串把路径写死的情况，本来也不该支持。
   ========================================================================== */

import { dirname, basename } from '../vault/fs.js';

// from '...' / import '...' / import('...') / export * from '...'
// 双引号、单引号都收；反引号（模板字符串）不收，动态拼接路径不在契约里
const SPEC_RE = /(?:\bfrom\s*|\bimport\s*\(?\s*)(['"])([^'"]+)\1/g;

/** 规范化相对路径：消掉 ./、../，统一成 a/b/c 形式 */
function collapse(path) {
  const out = [];
  for (const seg of String(path).replace(/\\/g, '/').split('/')) {
    if (!seg || seg === '.') continue;
    if (seg === '..') out.pop();
    else out.push(seg);
  }
  return out.join('/');
}

/** 说明符是否是相对路径；裸模块名在 blob 里无解，必须报错而不是悄悄跳过 */
const isRelative = (spec) => spec.startsWith('./') || spec.startsWith('../');

function resolveSpecifier(fromPath, spec, sourceDir, known) {
  if (!isRelative(spec)) {
    throw new Error(
      `插件引用了无法解析的模块「${spec}」（${basename(fromPath)}）：只支持 './' 或 '../' 开头的相对路径`
    );
  }
  const base = dirname(fromPath);
  const joined = collapse(base ? `${base}/${spec}` : spec);

  if (!joined.startsWith(`${sourceDir}/`)) {
    throw new Error(`插件引用了目录之外的模块「${spec}」（${basename(fromPath)}）`);
  }

  for (const candidate of [joined, `${joined}.js`, `${joined}/index.js`]) {
    if (known.has(candidate)) return candidate;
  }
  throw new Error(`找不到模块「${spec}」（${basename(fromPath)} 引用）`);
}

/** 读插件目录下所有 .js 的源码 */
async function readSources(vault, sourceDir) {
  const all = await vault.fs.walk();
  const prefix = `${sourceDir}/`;
  const paths = all
    .filter((e) => e.type === 'file' && e.path.startsWith(prefix) && /\.js$/i.test(e.path))
    .map((e) => e.path);

  const sources = new Map();
  await Promise.all(
    paths.map(async (path) => {
      sources.set(path, (await vault.fs.read(path)) ?? '');
    })
  );
  return sources;
}

/** manifest.main 可能写成 'main.js'、'./main.js' 或带子目录 */
function resolveEntry(sourceDir, main, sources) {
  const clean = String(main || 'main.js').replace(/^\.\//, '');
  const guess = collapse(`${sourceDir}/${clean}`);
  for (const candidate of [guess, `${guess}.js`]) {
    if (sources.has(candidate)) return candidate;
  }
  throw new Error(`找不到入口文件「${clean}」（manifest.main）`);
}

/**
 * 把一个插件目录编译成一棵 blob 模块树。
 *
 * @returns {Promise<{ entryUrl: string, urls: string[] }>}
 *   entryUrl 是入口模块的 blob URL；urls 是本次创建的全部 URL，
 *   卸载时必须逐个 revokeObjectURL。
 */
export async function buildPlugin({ vault, dir, main = 'main.js' }) {
  const sourceDir = collapse(dir);
  const sources = await readSources(vault, sourceDir);

  if (!sources.size) throw new Error('插件目录里没有 .js 文件');

  const entry = resolveEntry(sourceDir, main, sources);
  const urls = new Map(); // path -> blob URL
  const state = new Map(); // path -> 'visiting' | 'done'
  const stack = [];
  const created = [];

  const visit = (path) => {    const mark = state.get(path);
    if (mark === 'done') return;
    if (mark === 'visiting') {
      const at = stack.indexOf(path);
      const chain = [...stack.slice(at), path].map((p) => basename(p)).join(' → ');
      throw new Error(`插件存在循环依赖：${chain}`);
    }

    state.set(path, 'visiting');
    stack.push(path);

    const code = sources.get(path) ?? '';
    const deps = [];
    for (const m of code.matchAll(SPEC_RE)) {
      const spec = m[2];
      // 绝对 URL（http/data/blob）浏览器自己能解析，原样留着
      if (!isRelative(spec)) {
        if (/^[a-z]+:/i.test(spec)) continue;
        throw new Error(
          `插件引用了无法解析的模块「${spec}」（${basename(path)}）：只支持 './' 或 '../' 开头的相对路径`
        );
      }
      deps.push({ spec, target: resolveSpecifier(path, spec, sourceDir, sources) });
    }

    // 后序：先把依赖建完，本文件替换说明符时才有 URL 可填
    for (const dep of deps) visit(dep.target);

    const resolved = new Map(deps.map((d) => [`${d.spec}`, urls.get(d.target)]));
    const rewritten = code.replace(SPEC_RE, (whole, quote, spec) => {
      const url = resolved.get(spec);
      return url ? whole.replace(`${quote}${spec}${quote}`, `${quote}${url}${quote}`) : whole;
    });

    const url = URL.createObjectURL(new Blob([rewritten], { type: 'text/javascript' }));
    urls.set(path, url);
    created.push(url);

    stack.pop();
    state.set(path, 'done');
  };

  try {
    visit(entry);
  } catch (err) {
    // 已经建出来的 blob 必须当场撤销：宿主拿不到这棵树，没有别的地方能回收
    for (const url of created) {
      try {
        URL.revokeObjectURL(url);
      } catch {}
    }
    throw err;
  }

  return {
    entryUrl: urls.get(entry),
    urls: created,
    entryPath: entry,
  };
}

/**
 * 入口 URL 的缓存版本号。
 *
 * 用 #v= 而不是 ?v=：blob: URL 的查询串会参与资源键匹配，Chromium 里
 * `blob:...uuid?v=1` 会直接 404；片段（#）不进资源查找，但同样能让模块表
 * 认为这是一个新 URL。blob 本身每次 createObjectURL 就已经唯一，这步是双保险。
 */
export function cacheBust(url) {
  return `${url}#v=${Date.now()}`;
}
