/* ==========================================================================
   Inkdown · 信息视图
   当前笔记的统计、时间、路径。所有数据都从 ctx 拿，不直接读仓库。
   ========================================================================== */

import { h, fill } from '../core/dom.js';
import { PropList, PropGroup, PropRow } from '../ui/kit.js';
import { charCount, lineCount, absDate, extractHeadings } from '../util.js';

export const infoView = {
  id: 'info',
  label: '信息',

  mount(host, ctx) {
    const list = PropList();
    host.append(list);

    const refresh = () => {
      const note = ctx.getNote();
      if (!note) {
        fill(list, h('div', { class: 'c-inspector__empty' }, ctx.getEmptyLabel()));
        return;
      }

      const content = ctx.getContent();
      const chars = charCount(content);
      const minutes = Math.max(1, Math.round(chars / 400));

      fill(
        list,
        PropGroup({
          title: '统计',
          children: [
            PropRow({ label: '字数', value: chars.toLocaleString('zh-CN') }),
            PropRow({ label: '字符', value: content.length.toLocaleString('zh-CN') }),
            PropRow({ label: '行数', value: String(lineCount(content)) }),
            PropRow({ label: '标题', value: String(extractHeadings(content).length) }),
            PropRow({ label: '阅读时长', value: `约 ${minutes} 分钟` }),
          ],
        }),
        PropGroup({
          title: '时间',
          children: [
            PropRow({ label: '创建于', value: absDate(note.createdAt) }),
            PropRow({ label: '更新于', value: absDate(note.mtime) }),
          ],
        }),
        PropGroup({
          title: '属性',
          children: [
            PropRow({ label: '收藏', value: note.favorite ? '是' : '否' }),
            PropRow({ label: '格式', value: 'Markdown' }),
            PropRow({ label: '存储', value: '本地文件' }),
          ],
        }),
        PropGroup({
          title: '位置',
          children: [PropRow({ label: '路径', value: note.path, mono: true })],
        })
      );
    };

    return { refresh };
  },
};
