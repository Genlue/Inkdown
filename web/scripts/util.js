/* ==========================================================================
   Inkdown · 小工具
   ========================================================================== */

export const relFmt = new Intl.RelativeTimeFormat('zh-CN', { numeric: 'auto' });

export function relTime(ts) {
  const diff = Number(ts) - Date.now();
  const abs = Math.abs(diff);
  if (!Number.isFinite(diff)) return '';
  if (abs < 45 * 1000) return '刚刚';
  if (abs < 3600 * 1000) return relFmt.format(Math.round(diff / 60000), 'minute');
  if (abs < 86400 * 1000) return relFmt.format(Math.round(diff / 3600000), 'hour');
  if (abs < 7 * 86400 * 1000) return relFmt.format(Math.round(diff / 86400000), 'day');
  return absDate(ts);
}

export function absDate(ts) {
  const d = new Date(Number(ts));
  if (Number.isNaN(d.getTime())) return '';
  const pad = (n) => String(n).padStart(2, '0');
  return `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())} ${pad(d.getHours())}:${pad(
    d.getMinutes()
  )}`;
}

export const charCount = (text) => String(text ?? '').replace(/\s/g, '').length;

export const lineCount = (text) => String(text ?? '').split('\n').length;

/** 从 Markdown 里抽标题：跳过围栏代码，行号留给预览定位 */
export function extractHeadings(content) {
  const items = [];
  const lines = String(content ?? '').split('\n');
  let fence = '';

  lines.forEach((line, i) => {
    const f = line.match(/^\s*(`{3,}|~{3,})/);
    if (f) {
      const ch = f[1][0];
      if (!fence) fence = ch;
      else if (ch === fence) fence = '';
      return;
    }
    if (fence) return;

    const m = line.match(/^\s{0,3}(#{1,6})\s+(.*?)(?:\s+#+)?\s*$/);
    if (!m) return;

    const text = m[2]
      .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
      .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
      .replace(/[*_~`]/g, '')
      .trim();

    if (text) items.push({ level: m[1].length, text, line: i });
  });

  return items;
}

/** 行号 → 字符偏移 */
export function lineOffset(text, line) {
  const lines = String(text ?? '').split('\n');
  let offset = 0;
  for (let i = 0; i < line && i < lines.length; i++) offset += lines[i].length + 1;
  return offset;
}
