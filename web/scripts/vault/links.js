/* ==========================================================================
   Inkdown · 链接 / 标签索引
   纯函数层：只吃字符串，不碰 DOM、不碰 vault、不碰网络，Node 里可直接单测。

   数据流：

     vault.flatNotes() ─┐
     vault.readNote() ──┴→ entries[{ path, title, content }]
                              │
                              ▼  buildIndex(entries)
     ┌───────────────────────────────────────────────────────────┐
     │ byPath      path → { title, tags, links, headings, content }│
     │ backlinks   path → 指向它的笔记集合（反向链接）              │
     │ tags        tag  → 带这个标签的笔记集合                     │
     │ unresolved  target → 引用了不存在的笔记的笔记集合（悬空链接）│
     └───────────────────────────────────────────────────────────┘
                              │
       findBacklinks / findOutlinks → 侧栏列表；snippetAround → 行内摘要

   解析前先把「不该解析的地方」抹成等长空格（围栏代码、行内代码、HTML 注释），
   这样后续正则不用处理转义，行号与列位置也保持不变。
   ========================================================================== */

const FENCE_LINE = /^\s*(`{3,}|~{3,})(.*)$/;
const HEADING_LINE = /^\s{0,3}(#{1,6})\s+(.*?)(?:\s+#+)?\s*$/;
const ATX_LIKE = /^\s{0,3}#{1,6}(?:\s|$)/;
const HEX_COLOR = /^(?:[0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;
// # 后紧跟字母/数字/中文/_-/；前面必须是行首或空白
/* 标签边界：前面不能是字母/数字/下划线/斜杠/#。
   用"否定"而不是"必须是空白"，是因为中文里标签常常紧跟在全角标点后面：
   标签：#入门 #产品/Inkdown —— 冒号后面那个也必须算。 */
const TAG = /(?<![\p{L}\p{N}_/#\\])#([\p{L}\p{N}_/-]+)/gu;

/* ---------------- 预处理 ---------------- */

const spaces = (n) => ' '.repeat(n);

/** 统一换行；非字符串一律当空 */
function toLines(source) {
  if (typeof source !== 'string' || !source) return [];
  return source.replace(/\r\n?/g, '\n').split('\n');
}

/** 标出哪些行属于围栏代码（``` 与 ~~~，闭合围栏长度不短于开启） */
function fenceFlags(lines) {
  const flags = new Array(lines.length);
  let mark = '';
  let width = 0;

  for (let i = 0; i < lines.length; i++) {
    const m = FENCE_LINE.exec(lines[i]);

    if (!mark) {
      // 反引号围栏的 info 串里不能再出现反引号，否则只是行内代码
      if (m && !(m[1][0] === '`' && m[2].includes('`'))) {
        mark = m[1][0];
        width = m[1].length;
        flags[i] = true;
      } else {
        flags[i] = false;
      }
      continue;
    }

    flags[i] = true;
    if (m && m[1][0] === mark && m[1].length >= width && !m[2].trim()) mark = '';
  }

  return flags;
}

/** 抹掉一行里的行内代码，长度不变 */
function maskInline(line) {
  if (!line.includes('`')) return line;
  return line.replace(/(`+)[\s\S]*?\1/g, (hit) => spaces(hit.length));
}

/** 抹掉 HTML 注释（可跨行），没闭合就抹到末尾 */
function maskComments(text) {
  if (!text.includes('<!--')) return text;

  const out = [];
  let i = 0;
  while (i < text.length) {
    const start = text.indexOf('<!--', i);
    if (start < 0) {
      out.push(text.slice(i));
      break;
    }
    out.push(text.slice(i, start));
    const end = text.indexOf('-->', start + 4);
    const stop = end < 0 ? text.length : end + 3;
    out.push(text.slice(start, stop).replace(/[^\n]/g, ' '));
    i = stop;
  }
  return out.join('');
}

/** 一次扫描拿到：原始行、围栏标记、抹掉代码与注释的文本 */
function scan(source) {
  const lines = toLines(source);
  if (!lines.length) return { lines, fenced: [], masked: '' };

  const fenced = fenceFlags(lines);
  const maskedLines = lines.map((line, i) => (fenced[i] ? spaces(line.length) : maskInline(line)));
  return { lines, fenced, masked: maskComments(maskedLines.join('\n')) };
}

/* ---------------- 抽取 ---------------- */

/** [[Note]] / [[Note|别名]] / [[Note#标题]] / [[Note#标题|别名]] */
function collectLinks(text) {
  const out = [];
  if (!text || !text.includes('[[')) return out;

  for (const m of text.matchAll(/(!?)\[\[([^\[\]\n]+?)\]\]/g)) {
    // ![[x]] 是嵌入（图片 / 整篇笔记），不是链接
    if (m[1] === '!') continue;

    let inner = m[2];
    let alias = null;
    let heading = null;

    const bar = inner.indexOf('|');
    if (bar >= 0) {
      alias = inner.slice(bar + 1).trim() || null;
      inner = inner.slice(0, bar);
    }

    const hash = inner.indexOf('#');
    if (hash >= 0) {
      heading = inner.slice(hash + 1).trim() || null;
      inner = inner.slice(0, hash);
    }

    const target = inner.trim();
    // [[#标题]] 是本文内部跳转，没有跨笔记的边
    if (!target) continue;

    out.push({ target, alias, heading });
  }

  return out;
}

/** #标签：跳过标题行、纯数字、3/4/6/8 位十六进制色值 */
function collectTags(text) {
  const out = [];
  const seen = new Set();
  if (!text || !text.includes('#')) return out;

  for (const line of text.split('\n')) {
    if (ATX_LIKE.test(line)) continue; // ATX 标题整行不算标签

    for (const m of line.matchAll(TAG)) {
      const tag = m[1].replace(/^[/-]+|[/-]+$/g, '');
      if (!tag) continue;
      if (/^\d+$/.test(tag)) continue;
      if (HEX_COLOR.test(tag)) continue;

      const key = tag.toLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      out.push(tag);
    }
  }

  return out;
}

/** 标题文字里的行内标记压平，和 util.js 的 extractHeadings 对齐 */
function plainHeading(text) {
  return String(text)
    .replace(/!\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_, target, alias) => alias || target)
    .replace(/\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|([^\]]+))?\]\]/g, (_, target, alias) => alias || target)
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/`([^`]*)`/g, '$1')
    .replace(/[*_~]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

function collectHeadings(lines, fenced) {
  const out = [];

  for (let i = 0; i < lines.length; i++) {
    if (fenced[i]) continue;
    const m = HEADING_LINE.exec(lines[i]);
    if (!m) continue;

    const text = plainHeading(m[2]);
    if (text) out.push({ level: m[1].length, text, line: i });
  }

  return out;
}

/* ---------------- 路径 ---------------- */

function normalizePath(path) {
  if (typeof path !== 'string') return '';
  return path.trim().replace(/\\/g, '/').replace(/^\.\//, '').replace(/^\/+/, '');
}

function noteName(path) {
  return path.slice(path.lastIndexOf('/') + 1).replace(/\.md$/i, '');
}

/** 链接目标归一：去 .md、去 ./ 与多余斜杠，统一反斜杠 */
function normalizeTarget(target) {
  const raw = typeof target === 'string' ? target : '';
  return raw
    .trim()
    .replace(/\\/g, '/')
    .replace(/^\/+/, '')
    .replace(/^(?:\.\/)+/, '')
    .replace(/\/{2,}/g, '/')
    .replace(/\/+$/, '')
    .replace(/\.md$/i, '')
    .trim();
}

/** 文件名（小写）→ 同名的全部路径，按路径长度升序（最短的优先） */
function buildLookup(byPath) {
  const byName = new Map();
  const byExact = new Map();

  for (const path of byPath.keys()) {
    const lower = path.toLowerCase();
    byExact.set(lower, path);
    if (lower.endsWith('.md')) byExact.set(lower.slice(0, -3), path);

    const name = noteName(path).toLowerCase();
    const bucket = byName.get(name);
    if (bucket) bucket.push(path);
    else byName.set(name, [path]);
  }

  for (const bucket of byName.values()) {
    bucket.sort((a, b) => a.length - b.length || a.localeCompare(b));
  }

  return { byName, byExact };
}

/** 先按相对路径（目标含 / 时），再按文件名；同名全部命中 */
function resolveTarget(target, lookup) {
  const norm = normalizeTarget(target);
  if (!norm) return [];

  const lower = norm.toLowerCase();
  if (lower.includes('/')) {
    const exact = lookup.byExact.get(lower);
    if (exact) return [exact];
  }

  return lookup.byName.get(lower.slice(lower.lastIndexOf('/') + 1)) ?? [];
}

function addEdge(map, key, value) {
  const set = map.get(key);
  if (set) set.add(value);
  else map.set(key, new Set([value]));
}

/* ---------------- 对外 API ---------------- */

/** 抽取双链；忽略围栏代码、行内代码、HTML 注释 */
export function extractWikilinks(markdown) {
  return collectLinks(scan(markdown).masked);
}

/** 抽取 #标签；忽略代码、HTML 注释、ATX 标题、纯数字与十六进制色值 */
export function extractTags(markdown) {
  return collectTags(scan(markdown).masked);
}

/** 抽取标题；围栏代码里的 # 不算标题。line 从 0 开始，和预览的 data-line 对齐 */
export function extractHeadings(markdown) {
  const { lines, fenced } = scan(markdown);
  return collectHeadings(lines, fenced);
}

/**
 * 建索引。entries: [{ path, title, content }]
 * 每篇的 content 会留在 byPath 里，findBacklinks 用它生成摘要。
 */
export function buildIndex(entries) {
  const byPath = new Map();
  const backlinks = new Map();
  const tags = new Map();
  const unresolved = new Map();
  const list = Array.isArray(entries) ? entries : [];

  for (const entry of list) {
    if (!entry || typeof entry !== 'object') continue;

    const path = normalizePath(entry.path);
    if (!path) continue;

    const content = typeof entry.content === 'string' ? entry.content : '';
    const { lines, fenced, masked } = scan(content);

    byPath.set(path, {
      title: String(entry.title ?? '').trim() || noteName(path),
      tags: collectTags(masked),
      links: collectLinks(masked),
      headings: collectHeadings(lines, fenced),
      content,
    });
  }

  const lookup = buildLookup(byPath);

  for (const [source, info] of byPath) {
    for (const link of info.links) {
      const hits = resolveTarget(link.target, lookup);
      if (!hits.length) {
        addEdge(unresolved, link.target, source);
        continue;
      }
      // 同名多篇都算入链；自链接不算反向链接
      for (const target of hits) if (target !== source) addEdge(backlinks, target, source);
    }

    for (const tag of info.tags) addEdge(tags, tag, source);
  }

  return { byPath, backlinks, tags, unresolved };
}

/** 谁引用了这篇。snippets 是带上下文的单行摘要，最多 3 条 */
export function findBacklinks(index, path) {
  const out = [];
  const byPath = index?.byPath;
  const links = index?.backlinks;
  if (!(byPath instanceof Map) || !(links instanceof Map)) return out;

  const key = normalizePath(path);
  const sources = links.get(key);
  if (!sources || !sources.size) return out;

  const lookup = buildLookup(byPath);

  for (const source of sources) {
    const info = byPath.get(source);
    if (!info) continue;

    const snippets = [];
    const seen = new Set();
    for (const link of info.links) {
      if (!resolveTarget(link.target, lookup).includes(key)) continue;
      const text = snippetAround(info.content, link.target);
      if (!text || seen.has(text)) continue;
      seen.add(text);
      snippets.push(text);
      if (snippets.length >= 3) break;
    }

    out.push({ path: source, title: info.title, snippets });
  }

  return out;
}

/** 这篇指向谁。path 为 null 表示悬空链接；同名多处会各出一行 */
export function findOutlinks(index, path) {
  const out = [];
  const byPath = index?.byPath;
  if (!(byPath instanceof Map)) return out;

  const info = byPath.get(normalizePath(path));
  if (!info) return out;

  const lookup = buildLookup(byPath);
  const seen = new Set();

  for (const link of info.links) {
    const hits = resolveTarget(link.target, lookup);

    if (!hits.length) {
      const key = `${link.target}\u0000`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({ target: link.target, path: null, title: link.alias || link.target });
      continue;
    }

    for (const target of hits) {
      const key = `${link.target}\u0000${target}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({
        target: link.target,
        path: target,
        title: byPath.get(target)?.title || link.alias || link.target,
      });
    }
  }

  return out;
}

/** 命中词两侧各取 radius 个字符，压成一行；找不到返回空串 */
export function snippetAround(content, needle, radius = 60) {
  const text = typeof content === 'string' ? content : '';
  const word = typeof needle === 'string' ? needle.trim() : '';
  if (!text || !word) return '';

  const r = Number.isFinite(radius) && radius > 0 ? radius : 60;
  let at = text.indexOf(word);
  let len = word.length;

  if (at < 0) {
    // 大小写不一致时再试一次
    at = text.toLowerCase().indexOf(word.toLowerCase());
    if (at < 0) return '';
  }

  const from = Math.max(0, at - r);
  const to = Math.min(text.length, at + len + r);
  const body = text.slice(from, to).replace(/\s+/g, ' ').trim();
  if (!body) return '';

  return `${from > 0 ? '…' : ''}${body}${to < text.length ? '…' : ''}`;
}
