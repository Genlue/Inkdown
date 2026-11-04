/* ==========================================================================
   Inkdown · MediaView
   图片、视频、音频的原生级展示视图
   ========================================================================== */

import { h } from '../core/dom.js';
import { Button, IconButton, Badge } from '../ui/kit.js';

export function createMediaView({ file, vault }) {
  const container = h('div', { class: 'ink-media-view' });

  const header = h(
    'div',
    { class: 'ink-view-toolbar' },
    h(
      'div',
      { class: 'ink-view-toolbar__title' },
      h('span', { class: 'c-title' }, file.name),
      Badge({ label: (file.ext || 'MEDIA').toUpperCase() })
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

  const stage = h('div', { class: 'ink-media-view__stage' });
  container.append(header, stage);

  const load = async () => {
    stage.replaceChildren(h('div', { class: 'ink-loading-state' }, '加载中...'));
    try {
      const b64 = await vault.readBinary(file.path);
      if (!b64) {
        stage.replaceChildren(h('div', { class: 'ink-empty-state' }, '无法加载文件'));
        return;
      }

      const ext = (file.ext || '').toLowerCase();
      let mime = 'application/octet-stream';
      if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp'].includes(ext)) {
        mime = ext === 'svg' ? 'image/svg+xml' : ext === 'jpg' || ext === 'jpeg' ? 'image/jpeg' : `image/${ext}`;
      } else if (['mp4', 'webm', 'mov'].includes(ext)) {
        mime = ext === 'mov' ? 'video/quicktime' : `video/${ext}`;
      } else if (['mp3', 'wav', 'm4a', 'ogg', 'flac'].includes(ext)) {
        mime = ext === 'mp3' ? 'audio/mpeg' : ext === 'wav' ? 'audio/wav' : `audio/${ext}`;
      }

      const dataUrl = b64.startsWith('data:') ? b64 : `data:${mime};base64,${b64}`;

      if (file.fileType === 'image') {
        const img = h('img', {
          class: 'ink-media-view__img',
          src: dataUrl,
          alt: file.name,
        });
        const wrapper = h('div', { class: 'ink-media-view__img-wrap' }, img);

        const zoomBtn100 = Button({
          label: '100%',
          size: 'sm',
          variant: 'plain',
          onClick: () => {
            img.style.maxWidth = 'none';
            img.style.maxHeight = 'none';
            img.style.width = 'auto';
          },
        });
        const zoomFit = Button({
          label: '适应窗口',
          size: 'sm',
          variant: 'plain',
          onClick: () => {
            img.style.maxWidth = '100%';
            img.style.maxHeight = '100%';
            img.style.width = 'auto';
          },
        });

        header.querySelector('.ink-view-toolbar__actions').prepend(zoomFit, zoomBtn100);
        stage.replaceChildren(wrapper);
      } else if (file.fileType === 'video') {
        const video = h('video', {
          class: 'ink-media-view__video',
          src: dataUrl,
          controls: true,
          autoplay: false,
        });
        stage.replaceChildren(h('div', { class: 'ink-media-view__video-wrap' }, video));
      } else if (file.fileType === 'audio') {
        const audio = h('audio', {
          class: 'ink-media-view__audio',
          src: dataUrl,
          controls: true,
        });
        stage.replaceChildren(h('div', { class: 'ink-media-view__audio-wrap' }, audio));
      }
    } catch (err) {
      stage.replaceChildren(h('div', { class: 'ink-empty-state' }, `加载出错: ${err.message}`));
    }
  };

  load();

  return container;
}
