/* ==========================================================================
   Inkdown · Markdown
   一个零依赖的 Markdown 渲染器。覆盖笔记场景够用的子集：
   标题 / 段落 / 粗体 / 斜体 / 删除线 / 行内代码 / 围栏代码 / 引用 /
   有序与无序列表（可嵌套）/ 任务列表 / 表格 / 分隔线 / 链接 / 图片。

   设计取舍：
   - 不做 HTML 直通。所有文本先转义，避免把笔记里的 <script> 当标签渲染。
   - 代码块交给 core/highlight.js 做离线高亮；未知语言直接转义。
   - 输出的是字符串，交给 innerHTML；调用方负责容器。
   ========================================================================== */

import { highlight, normalizeLang } from './core/highlight.js';

const ESCAPES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
const escapeHtml = (s) => String(s).replace(/[&<>"']/g, (c) => ESCAPES[c]);
/** 拦掉会执行脚本的协议。图片允许 data:image。 */
function safeUrl(url, { image = false } = {}) {
  const u = String(url).trim();
  if (/^(javascript|vbscript|file):/i.test(u)) return '#';
  if (/^data:/i.test(u) && !(image && /^data:image\//i.test(u))) return '#';
  return u;
}

/* ---------------- 行内 ---------------- */

const CODE_TOKEN = '\u0000';

function inline(text) {
  const codes = [];

  // 行内代码先抽出来，避免里面的 * 和 _ 被当成强调
  let s = String(text).replace(/`([^`\n]+)`/g, (_, code) => {
    codes.push(code);
    return `${CODE_TOKEN}${codes.length - 1}${CODE_TOKEN}`;
  });

  s = escapeHtml(s);

  // Obsidian 嵌入图片：![[image.png]] 或 ![[image.png|300]]
  s = s.replace(/!\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_, target, meta) => {
    const cleanTarget = target.trim();
    const widthStyle = meta && /^\d+$/.test(meta.trim()) ? ` style="max-width:${meta.trim()}px"` : '';
    const isRemote = /^(https?:|data:)/i.test(cleanTarget);
    return `<img class="ink-img" src="${isRemote ? cleanTarget : ''}" data-vault-src="${escapeHtml(cleanTarget)}" alt="${escapeHtml(cleanTarget)}" loading="lazy"${widthStyle}>`;
  });

  // 标准 Markdown 引用图片：![alt](src)
  s = s.replace(/!\[([^\]]*)\]\(([^)\s]+)\)/g, (_, alt, src) => {
    const cleanSrc = src.trim();
    const isRemote = /^(https?:|data:)/i.test(cleanSrc);
    return `<img class="ink-img" src="${isRemote ? safeUrl(cleanSrc, { image: true }) : ''}" data-vault-src="${escapeHtml(cleanSrc)}" alt="${alt}" loading="lazy">`;
  });

  // Obsidian 双链：[[note]] 或 [[note|别名]]
  s = s.replace(/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_, target, alias) => {
    const cleanTarget = target.trim();
    const label = alias ? alias.trim() : cleanTarget;
    return `<a class="ink-wikilink" href="#wikilink" data-target="${escapeHtml(cleanTarget)}" title="跳转到: ${escapeHtml(cleanTarget)}">${escapeHtml(label)}</a>`;
  });

  // 标准链接
  s = s.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, label, href) => {
    const url = safeUrl(href);
    const external = /^https?:/i.test(url);
    return `<a href="${url}"${external ? ' target="_blank" rel="noreferrer"' : ''}>${label}</a>`;
  });

  // 裸链接
  s = s.replace(/(^|[\s(])(https?:\/\/[^\s<)]+)/g, (m, pre, url) => {
    return `${pre}<a href="${url}" target="_blank" rel="noreferrer">${url}</a>`;
  });

  // 粗体 / 斜体 / 删除线
  s = s.replace(/\*\*(?=\S)([\s\S]*?\S)\*\*/g, '<strong>$1</strong>');
  s = s.replace(/__(?=\S)([\s\S]*?\S)__/g, '<strong>$1</strong>');
  s = s.replace(/~~(?=\S)([\s\S]*?\S)~~/g, '<del>$1</del>');
  s = s.replace(/(^|[^*\w])\*(?=\S)([^*\n]*?\S)\*/g, '$1<em>$2</em>');
  s = s.replace(/(^|[^_\w])_(?=\S)([^_\n]*?\S)_/g, '$1<em>$2</em>');

  // 把行内代码放回去
  s = s.replace(new RegExp(`${CODE_TOKEN}(\\d+)${CODE_TOKEN}`, 'g'), (_, i) => {
    return `<code>${escapeHtml(codes[Number(i)])}</code>`;
  });

  return s;
}

/* ---------------- 块级 ---------------- */

const isBlank = (line) => /^\s*$/.test(line);
const FENCE_OPEN = /^\s*(`{3,}|~{3,})\s*([^\s`]*)\s*$/;
const HEADING = /^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$/;
const RULE = /^\s{0,3}(?:(?:\*\s*){3,}|(?:-\s*){3,}|(?:_\s*){3,})$/;
const QUOTE = /^\s{0,3}>\s?/;
const LIST_ITEM = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/;
const SEPARATOR = /^\s*\|?[\s:|-]*-[\s:|-]*\|?\s*$/;

function isTableStart(lines, i) {
  if (!lines[i] || !lines[i].includes('|')) return false;
  const next = lines[i + 1];
  if (!next || !SEPARATOR.test(next)) return false;
  return /^[\s|:-]+$/.test(next) && next.includes('-');
}

function startsBlock(lines, i) {
  const line = lines[i];
  return (
    FENCE_OPEN.test(line) ||
    HEADING.test(line) ||
    RULE.test(line) ||
    QUOTE.test(line) ||
    LIST_ITEM.test(line) ||
    isTableStart(lines, i)
  );
}

function splitRow(line) {
  return line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((cell) => cell.trim());
}

function renderTable(lines, start) {
  const head = splitRow(lines[start]);
  const align = splitRow(lines[start + 1]).map((cell) => {
    const left = cell.startsWith(':');
    const right = cell.endsWith(':');
    if (left && right) return 'center';
    if (right) return 'right';
    if (left) return 'left';
    return '';
  });

  let i = start + 2;
  const rows = [];
  while (i < lines.length && !isBlank(lines[i]) && lines[i].includes('|')) {
    rows.push(splitRow(lines[i]));
    i++;
  }

  const cell = (tag, value, index) => {
    const a = align[index] ? ` style="text-align:${align[index]}"` : '';
    return `<${tag}${a}>${inline(value ?? '')}</${tag}>`;
  };

  const thead = `<thead><tr>${head.map((c, k) => cell('th', c, k)).join('')}</tr></thead>`;
  const tbody = rows.length
    ? `<tbody>${rows
        .map((r) => `<tr>${head.map((_, k) => cell('td', r[k], k)).join('')}</tr>`)
        .join('')}</tbody>`
    : '';

  return { html: `<div class="md-table"><table>${thead}${tbody}</table></div>`, next: i };
}

function renderList(lines, start, offset = 0) {
  const roots = [];
  const stack = [];
  let baseIndent = -1;
  let i = start;

  while (i < lines.length) {
    const line = lines[i];
    const m = line.match(LIST_ITEM);

    if (m) {
      const indent = m[1].replace(/\t/g, '    ').length;
      if (baseIndent < 0) baseIndent = indent;
      if (indent < baseIndent) break;

      const item = {
        indent,
        ordered: /^\d/.test(m[2]),
        text: m[3],
        line: offset + i,
        children: [],
      };

      while (stack.length && stack[stack.length - 1].indent >= indent) stack.pop();

      // 同一层换了列表类型（- ↔ 1.）就是另一张列表，不要并进上一个
      const siblings = stack.length ? stack[stack.length - 1].children : roots;
      if (siblings.length && siblings[0].ordered !== item.ordered) break;

      siblings.push(item);
      stack.push(item);
      i++;
      continue;
    }

    // 列表项的续行：缩进两个空格以上，接在上一行后面
    if (stack.length && !isBlank(line) && /^\s{2,}/.test(line) && !isTableStart(lines, i)) {
      stack[stack.length - 1].text += `\n${line.trim()}`;
      i++;
      continue;
    }

    // 空行之后还是同一层的列表项 → 列表没结束
    if (isBlank(line)) {
      let j = i + 1;
      while (j < lines.length && isBlank(lines[j])) j++;
      const nextItem = j < lines.length ? lines[j].match(LIST_ITEM) : null;
      if (nextItem && nextItem[1].replace(/\t/g, '    ').length >= baseIndent) {
        i = j;
        continue;
      }
    }

    break;
  }

  const renderItems = (items) => {
    const ordered = items[0]?.ordered;
    const tag = ordered ? 'ol' : 'ul';
    return `<${tag}>${items.map(renderItem).join('')}</${tag}>`;
  };

  const renderItem = (item) => {
    const task = item.text.match(/^\[([ xX])\]\s+([\s\S]*)$/);
    let body;

    if (task) {
      const checked = task[1].toLowerCase() === 'x' ? ' checked' : '';
      body = `<label class="md-task"><input type="checkbox"${checked} data-line="${item.line ?? ''}"><span>${inline(
        task[2]
      )}</span></label>`;
    } else {
      body = inline(item.text).replace(/\n/g, '<br>');
    }

    const kids = item.children.length ? renderItems(item.children) : '';
    return `<li${task ? ' class="md-li-task"' : ''}>${body}${kids}</li>`;
  };

  return { html: renderItems(roots), next: i };
}

function renderBlocks(lines, offset = 0) {
  const out = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (isBlank(line)) {
      i++;
      continue;
    }

    // 围栏代码
    const fence = line.match(FENCE_OPEN);
    if (fence) {
      const marker = fence[1][0];
      const lang = fence[2] || '';
      const close = new RegExp(`^\\s*${marker}{3,}\\s*$`);
      const buf = [];
      i++;
      while (i < lines.length && !close.test(lines[i])) {
        buf.push(lines[i]);
        i++;
      }
      i++; // 跳过闭合围栏
      const label = lang ? `<span class="md-code__lang">${escapeHtml(lang)}</span>` : '<span class="md-code__lang">TEXT</span>';
      const code = highlight(buf.join('\n'), lang);
      out.push(
        `<div class="md-code"><div class="md-code__header">${label}<button type="button" class="md-code__copy" aria-label="复制代码" title="复制代码">复制</button></div><pre><code${lang ? ` data-lang="${escapeHtml(normalizeLang(lang) || lang)}"` : ''}>${code}</code></pre></div>`
      );
      continue;
    }

    // 标题
    const heading = line.match(HEADING);
    if (heading) {
      const level = heading[1].length;
      // data-line 指向源码行号：右侧大纲靠它把标题和正文位置对上
      out.push(`<h${level} data-line="${i + offset}">${inline(heading[2])}</h${level}>`);
      i++;
      continue;
    }

    // 分隔线
    if (RULE.test(line)) {
      out.push('<hr>');
      i++;
      continue;
    }

    // 引用
    if (QUOTE.test(line)) {
      const start = i;
      const buf = [];
      while (i < lines.length && QUOTE.test(lines[i])) {
        buf.push(lines[i].replace(QUOTE, ''));
        i++;
      }
      out.push(`<blockquote>${renderBlocks(buf, offset + start)}</blockquote>`);
      continue;
    }

    // 表格
    if (isTableStart(lines, i)) {
      const { html, next } = renderTable(lines, i);
      out.push(html);
      i = next;
      continue;
    }

    // 列表
    if (LIST_ITEM.test(line)) {
      const { html, next } = renderList(lines, i, offset);
      out.push(html);
      i = next;
      continue;
    }

    // 段落
    const buf = [];
    while (i < lines.length && !isBlank(lines[i]) && (buf.length === 0 || !startsBlock(lines, i))) {
      buf.push(lines[i].trim());
      i++;
    }
    if (buf.length) out.push(`<p>${inline(buf.join('\n')).replace(/\n/g, '<br>')}</p>`);
    else i++;
  }

  return out.join('\n');
}

/** Markdown → HTML 字符串 */
export function renderMarkdown(source) {
  const text = String(source ?? '')
    .replace(/\r\n?/g, '\n')
    .replace(/\t/g, '    ');
  if (!text.trim()) return '';
  return renderBlocks(text.split('\n'));
}

/** 从正文里抽一行纯文本，用来做列表摘要 / 自动标题 */
export function plainExcerpt(source, limit = 90) {
  const lines = String(source ?? '')
    .replace(/\r\n?/g, '\n')
    .split('\n');

  for (const raw of lines) {
    const line = raw
      .replace(/^\s*#{1,6}\s+/, '')
      .replace(/^\s*>\s?/, '')
      .replace(/^\s*([-*+]|\d+[.)])\s+/, '')
      .replace(/^\s*\[[ xX]\]\s+/, '')
      .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
      .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
      .replace(/[*_~`]/g, '')
      .trim();
    if (line) return line.length > limit ? `${line.slice(0, limit)}…` : line;
  }
  return '';
}

/**
 * 异步解析并装载 Markdown 预览区引用的本地仓库图片。
 * 支持：![alt](assets/pic.png)、![[pic.png]] 等多种本地引用方式。
 */
export async function resolveMarkdownImages(container, vault, currentPath = '') {
  if (!container || !vault) return;
  const imgs = container.querySelectorAll('img[data-vault-src]');
  for (const img of imgs) {
    const raw = img.dataset.vaultSrc;
    if (!raw) continue;
    if (/^(https?:|data:|blob:)/i.test(raw)) {
      if (!img.src) img.src = raw;
      continue;
    }

    const clean = raw.replace(/\\/g, '/').replace(/^\/+/, '');
    const dir = currentPath ? currentPath.slice(0, currentPath.lastIndexOf('/')) : '';
    const candidates = [
      clean,
      dir ? `${dir}/${clean}` : null,
      `assets/${clean}`,
      clean.startsWith('assets/') ? clean : null,
    ].filter(Boolean);

    for (const cand of candidates) {
      if (await vault.fs.exists(cand)) {
        try {
          const b64 = await vault.readBinary(cand);
          if (b64) {
            const ext = cand.slice(cand.lastIndexOf('.') + 1).toLowerCase();
            const mime = ext === 'svg' ? 'image/svg+xml' : ext === 'jpg' || ext === 'jpeg' ? 'image/jpeg' : ext === 'gif' ? 'image/gif' : ext === 'webp' ? 'image/webp' : 'image/png';
            img.src = b64.startsWith('data:') ? b64 : `data:${mime};base64,${b64}`;
            break;
          }
        } catch (err) {
          console.warn('[markdown] 加载图片失败:', cand, err);
        }
      }
    }
  }
}
