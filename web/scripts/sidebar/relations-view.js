/* ==========================================================================
   Inkdown · 关联视图
   把一篇笔记放回它所在的网里：谁提到了我（反链）、我提到了谁（出链）、
   我身上有哪些标签、哪些链接是悬空的。

   数据来自 vault/links.js 建的全量索引，本视图只负责呈现与点击行为 ——
   不读文件、不解析 Markdown，索引什么时候重建由宿主决定。
   ========================================================================== */

import { h, fill } from '../core/dom.js';

const MAX_ROWS = 40;

function section(label, count, children, emptyText) {
  return h(
    'section',
    { class: 'ink-rel__section' },
    h(
      'header',
      { class: 'ink-rel__head' },
      h('span', { class: 'ink-rel__label' }, label),
      h('span', { class: 'ink-rel__count' }, String(count))
    ),
    children && children.length
      ? h('div', { class: 'ink-rel__body' }, ...children)
      : h('div', { class: 'ink-rel__empty' }, emptyText)
  );
}

function linkRow({ title, subtitle, icon, onClick, dangling = false }) {
  return h(
    'button',
    {
      class: 'ink-rel__row' + (dangling ? ' is-dangling' : ''),
      type: 'button',
      title: subtitle || title,
      onclick: onClick,
    },
    h('span', { class: 'ink-rel__row-title' }, title),
    subtitle ? h('span', { class: 'ink-rel__row-sub' }, subtitle) : null
  );
}

export const relationsView = {
  id: 'relations',
  label: '关联',

  mount(host, ctx) {
    const root = h('div', { class: 'ink-rel' });
    host.append(root);

    const refresh = () => {
      const note = ctx.getNote();
      if (!note) {
        fill(root, h('div', { class: 'c-inspector__empty' }, ctx.getEmptyLabel()));
        return;
      }

      if (ctx.isIndexing()) {
        fill(root, h('div', { class: 'ink-rel__hint' }, '正在建立链接索引…'));
        return;
      }

      const backs = ctx.getBacklinks();
      const outs = ctx.getOutlinks();
      const tags = ctx.getTags();
      const dangling = ctx.getUnresolved();

      const backRows = backs.slice(0, MAX_ROWS).map((b) =>
        linkRow({
          title: b.title,
          subtitle: b.snippets?.[0] || b.path,
          onClick: () => ctx.openPath(b.path),
        })
      );

      const outRows = outs.slice(0, MAX_ROWS).map((o) =>
        linkRow({
          title: o.title || o.target,
          subtitle: o.path ? o.path : '还没有这篇笔记',
          dangling: !o.path,
          onClick: () => ctx.openWikilink(o.target),
        })
      );

      const tagChips = tags.slice(0, MAX_ROWS).map((t) =>
        h(
          'button',
          {
            class: 'ink-rel__tag',
            type: 'button',
            title: `搜索 #${t.tag}`,
            onclick: () => ctx.filterBy(`#${t.tag}`),
          },
          h('span', { class: 'ink-rel__tag-name' }, `#${t.tag}`),
          h('span', { class: 'ink-rel__tag-count' }, String(t.count))
        )
      );

      const danglingRows = dangling.slice(0, MAX_ROWS).map((d) =>
        linkRow({
          title: d.target,
          subtitle: `被 ${d.count} 篇笔记引用`,
          dangling: true,
          onClick: () => ctx.openWikilink(d.target),
        })
      );

      fill(
        root,
        section(
          '反向链接',
          backs.length,
          backRows,
          '还没有其他笔记链接到这里'
        ),
        section('出链', outs.length, outRows, '这篇笔记里没有 [[双链]]'),
        section(
          '标签',
          tags.length,
          tagChips.length ? [h('div', { class: 'ink-rel__chips' }, ...tagChips)] : [],
          '没有 #标签'
        ),
        dangling.length ? section('悬空链接', dangling.length, danglingRows, '') : null
      );
    };

    return { refresh };
  },
};
