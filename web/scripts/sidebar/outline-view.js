/* ==========================================================================
   Inkdown · 大纲视图
   只是右侧栏的一个视图：读当前笔记的标题，点一行跳过去，滚动时高亮当前小节。
   ========================================================================== */

import { h } from '../core/dom.js';
import { OutlineItem } from '../ui/kit.js';
import { extractHeadings } from '../util.js';

export const outlineView = {
  id: 'outline',
  label: '大纲',

  mount(host, ctx) {
    const list = h('div', { class: 'c-outline' });
    const empty = h('div', { class: 'c-inspector__empty' });
    host.append(list, empty);

    let items = [];
    let rows = [];
    let active = -1;
    let lock = 0;
    let raf = 0;

    const setActive = (idx) => {
      if (idx === active) return;
      active = idx;
      rows.forEach((row, i) => row.setActive(i === idx));
    };

    const refresh = () => {
      items = extractHeadings(ctx.getContent());
      const hasNote = Boolean(ctx.getNote());

      list.style.display = items.length ? '' : 'none';
      empty.style.display = items.length ? 'none' : '';
      if (!items.length) {
        empty.textContent = hasNote ? '没有可跳转的标题' : ctx.getEmptyLabel();
        rows = [];
        active = -1;
        return;
      }

      const keep = active;
      active = -1;

      rows = items.map((item, i) =>
        OutlineItem({
          label: item.text,
          level: item.level,
          active: false,
          onClick: () => {
            ctx.jumpToLine(item.line);
            lock = performance.now() + 650;
            setActive(i);
          },
        })
      );

      list.replaceChildren(...rows);
      setActive(Math.min(Math.max(keep, 0), items.length - 1));
    };

    const fromPreview = () => {
      const scroll = ctx.getPreviewScroll();
      const preview = ctx.getPreview();
      if (!scroll || !preview) return 0;

      const threshold = scroll.getBoundingClientRect().top + 76;
      let idx = 0;
      for (let k = 0; k < items.length; k++) {
        const el = preview.querySelector(`[data-line="${items[k].line}"]`);
        if (el && el.getBoundingClientRect().top < threshold) idx = k;
      }
      return idx;
    };

    const fromEditor = () => {
      const scrollEl = ctx.getEditorScroll ? ctx.getEditorScroll() : ctx.getEditor();
      const ed = ctx.getEditor();
      if (!scrollEl || !ed) return 0;
      const range = scrollEl.scrollHeight - scrollEl.clientHeight;
      if (range <= 1) return 0;
      const totalLines = ed.value.split('\n').length || 1;
      const line = (scrollEl.scrollTop / range) * totalLines;
      let idx = 0;
      for (let k = 0; k < items.length; k++) if (items[k].line <= line) idx = k;
      return idx;
    };

    const spy = () => {
      if (!ctx.isActive('outline') || !items.length) return;
      if (performance.now() < lock) return;
      if (raf) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        setActive(ctx.getMode() === 'edit' ? fromEditor() : fromPreview());
      });
    };

    const editorScroll = ctx.getEditorScroll ? ctx.getEditorScroll() : ctx.getEditor();
    const previewScroll = ctx.getPreviewScroll();
    editorScroll?.addEventListener('scroll', spy, { passive: true });
    previewScroll?.addEventListener('scroll', spy, { passive: true });

    return {
      refresh,
      setActive,
      destroy() {
        editorScroll?.removeEventListener('scroll', spy);
        previewScroll?.removeEventListener('scroll', spy);
      },
    };
  },
};
