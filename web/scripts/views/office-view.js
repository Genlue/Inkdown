/* ==========================================================================
   Inkdown · OfficeView
   Office 文档 (Excel, Word, PPT) 的原生级展示视图
   ========================================================================== */

import { h } from '../core/dom.js';
import { Button, Badge, Card, Table } from '../ui/kit.js';

export function createOfficeView({ file, vault }) {
  const container = h('div', { class: 'ink-office-view' });

  const ext = (file.ext || '').toUpperCase();

  const header = h(
    'div',
    { class: 'ink-view-toolbar' },
    h(
      'div',
      { class: 'ink-view-toolbar__title' },
      h('span', { class: 'c-title' }, file.name),
      Badge({ label: ext })
    ),
    h(
      'div',
      { class: 'ink-view-toolbar__actions' },
      Button({
        label: '在外部 Office 中打开',
        size: 'sm',
        variant: 'primary',
        icon: 'external-link',
        onClick: () => vault.fs.openPath(file.path),
      })
    )
  );

  const stage = h('div', { class: 'ink-office-view__stage' });
  container.append(header, stage);

  const parseCsv = (text) => {
    const lines = text.split(/\r?\n/).filter((l) => l.trim() !== '');
    if (!lines.length) return { columns: [], rows: [] };
    const headerRow = lines[0].split(',').map((c) => c.trim().replace(/^["']|["']$/g, ''));
    const columns = headerRow.map((col, idx) => ({
      key: `col_${idx}`,
      label: col || `列 ${idx + 1}`,
      sortable: true,
    }));
    const rows = lines.slice(1).map((line, rowIdx) => {
      const cells = line.split(',').map((c) => c.trim().replace(/^["']|["']$/g, ''));
      const obj = { id: `row_${rowIdx}` };
      columns.forEach((c, idx) => {
        obj[c.key] = cells[idx] ?? '';
      });
      return obj;
    });
    return { columns, rows };
  };

  const load = async () => {
    stage.replaceChildren(h('div', { class: 'ink-loading-state' }, '正在加载文档...'));

    if (file.ext === 'csv') {
      try {
        const text = (await vault.readNote(file.path)) ?? '';
        const { columns, rows } = parseCsv(text);
        if (!columns.length) {
          stage.replaceChildren(h('div', { class: 'ink-empty-state' }, 'CSV 文件为空'));
          return;
        }
        const table = Table({
          columns,
          rows,
          dense: true,
          rowKey: 'id',
        });
        stage.replaceChildren(
          h(
            'div',
            { class: 'ink-office-view__table-wrap' },
            h('div', { class: 'ink-office-view__meta c-subtitle' }, `共 ${rows.length} 行 · ${columns.length} 列数据`),
            table
          )
        );
        return;
      } catch (err) {
        console.warn('CSV 解析失败', err);
      }
    }

    // DOCX / XLSX / PPTX 丰富展示卡片
    const iconName = file.fileType === 'word' ? 'file-text' : file.fileType === 'excel' ? 'table' : 'presentation';
    const typeLabel = file.fileType === 'word' ? 'Word 文档' : file.fileType === 'excel' ? 'Excel 电子表格' : 'PowerPoint 演示文稿';

    const card = Card({
      className: 'ink-office-card',
      children: [
        h(
          'div',
          { class: 'ink-office-card__body' },
          h('div', { class: 'ink-office-card__badge' }, Badge({ label: ext })),
          h('div', { class: 'c-title', style: { fontSize: '18px', marginTop: '12px' } }, file.name),
          h('div', { class: 'c-subtitle', style: { marginTop: '4px' } }, `${typeLabel} · 本地文件`),
          h(
            'div',
            { class: 'ink-office-card__actions', style: { marginTop: '20px' } },
            Button({
              label: `在系统 ${typeLabel} 软件中打开编辑`,
              variant: 'primary',
              icon: 'external-link',
              onClick: () => vault.fs.openPath(file.path),
            }),
            Button({
              label: '在文件资源管理器中定位',
              variant: 'plain',
              icon: 'folder',
              onClick: () => vault.fs.invoke?.('open_in_explorer', { path: file.path }),
            })
          )
        ),
      ],
    });

    stage.replaceChildren(h('div', { class: 'ink-office-view__card-wrap' }, card));
  };

  load();

  return container;
}
