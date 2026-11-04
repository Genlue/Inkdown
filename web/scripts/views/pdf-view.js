/* ==========================================================================
   Inkdown · PdfView
   PDF 文档的原生级展示视图
   ========================================================================== */

import { h } from '../core/dom.js';
import { Button, Badge } from '../ui/kit.js';

export function createPdfView({ file, vault }) {
  const container = h('div', { class: 'ink-pdf-view' });

  const header = h(
    'div',
    { class: 'ink-view-toolbar' },
    h(
      'div',
      { class: 'ink-view-toolbar__title' },
      h('span', { class: 'c-title' }, file.name),
      Badge({ label: 'PDF' })
    ),
    h(
      'div',
      { class: 'ink-view-toolbar__actions' },
      Button({
        label: '在系统中打开',
        size: 'sm',
        variant: 'plain',
        icon: 'external-link',
        onClick: () => vault.fs.openPath(file.path),
      })
    )
  );

  const stage = h('div', { class: 'ink-pdf-view__stage' });
  container.append(header, stage);

  const load = async () => {
    stage.replaceChildren(h('div', { class: 'ink-loading-state' }, '加载 PDF 中...'));
    try {
      const b64 = await vault.readBinary(file.path);
      if (!b64) {
        stage.replaceChildren(h('div', { class: 'ink-empty-state' }, '无法加载 PDF 文件'));
        return;
      }

      const dataUrl = b64.startsWith('data:') ? b64 : `data:application/pdf;base64,${b64}`;

      const iframe = h('iframe', {
        class: 'ink-pdf-view__frame',
        src: dataUrl,
        title: file.name,
      });

      stage.replaceChildren(iframe);
    } catch (err) {
      stage.replaceChildren(h('div', { class: 'ink-empty-state' }, `加载 PDF 失败: ${err.message}`));
    }
  };

  load();

  return container;
}
