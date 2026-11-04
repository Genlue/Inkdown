/* ==========================================================================
   Inkdown · CodeView
   代码文件的原生级展示视图
   ========================================================================== */

import { h } from '../core/dom.js';
import { Button, Badge, toast } from '../ui/kit.js';

export function createCodeView({ file, vault }) {
  const container = h('div', { class: 'ink-code-view' });

  let textContent = '';

  const header = h(
    'div',
    { class: 'ink-view-toolbar' },
    h(
      'div',
      { class: 'ink-view-toolbar__title' },
      h('span', { class: 'c-title' }, file.name),
      Badge({ label: (file.ext || 'CODE').toUpperCase() }),
      h('span', { class: 'ink-code-view__stats c-subtitle' }, '')
    ),
    h(
      'div',
      { class: 'ink-view-toolbar__actions' },
      Button({
        label: '复制内容',
        size: 'sm',
        variant: 'plain',
        icon: 'copy',
        onClick: async () => {
          try {
            await navigator.clipboard.writeText(textContent);
            toast('已复制到剪贴板');
          } catch {
            toast('复制失败');
          }
        },
      }),
      Button({
        label: '在系统中打开',
        size: 'sm',
        variant: 'plain',
        icon: 'external-link',
        onClick: () => vault.fs.openPath(file.path),
      })
    )
  );

  const stage = h('div', { class: 'ink-code-view__stage' });
  container.append(header, stage);

  const load = async () => {
    stage.replaceChildren(h('div', { class: 'ink-loading-state' }, '加载代码中...'));
    try {
      textContent = (await vault.readNote(file.path)) ?? '';
      const lines = textContent.split(/\r?\n/);
      const stats = header.querySelector('.ink-code-view__stats');
      if (stats) stats.textContent = `${lines.length} 行 · ${textContent.length} 字符`;

      const lineNums = h(
        'div',
        { class: 'ink-code-view__lines' },
        lines.map((_, i) => h('span', null, String(i + 1)))
      );

      const codeContent = h('pre', { class: 'ink-code-view__pre' }, h('code', null, textContent));

      const codeBody = h('div', { class: 'ink-code-view__body' }, lineNums, codeContent);
      stage.replaceChildren(codeBody);
    } catch (err) {
      stage.replaceChildren(h('div', { class: 'ink-empty-state' }, `加载代码失败: ${err.message}`));
    }
  };

  load();

  return container;
}
