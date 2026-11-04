/* ==========================================================================
   Inkdown · ViewHost
   多视图调度宿主（协调 Markdown 编辑器、设置页、PDF、媒体、Office、代码等各种视图）
   ========================================================================== */

import { h, fill } from '../core/dom.js';
import { isMarkdown, basename } from '../vault/fs.js';
import { Empty } from '../ui/kit.js';
import { createMediaView } from './media-view.js';
import { createPdfView } from './pdf-view.js';
import { createCodeView } from './code-view.js';
import { createOfficeView } from './office-view.js';
import { createSettingsView } from './settings-view.js';

export class ViewHost {
  constructor({ container, vault, onFontChange, onThemeChange, onPickFolder, onOpenInExplorer, noteViewNode }) {
    this.onPickFolder = onPickFolder;
    this.onOpenInExplorer = onOpenInExplorer;
    this.container = container;
    this.vault = vault;
    this.onFontChange = onFontChange;
    this.onThemeChange = onThemeChange;
    this.noteViewNode = noteViewNode; // 现有的 Markdown 编辑器 DOM 节点

    this.activeTab = null;
    this.viewCache = new Map(); // tabId -> HTMLElement
    this.emptyNode = h('div', { class: 'ink-view-host__empty' });

    // 把现有的 Markdown 节点装入缓存池
    this.viewCache.set('__note__', this.noteViewNode);
  }

  showEmpty() {
    this.container.dataset.empty = '1';
    for (const [_, el] of this.viewCache) {
      el.style.display = 'none';
    }
    fill(
      this.emptyNode,
      Empty({
        icon: 'square-dashed',
        title: '没有打开的页面',
        desc: '从左侧文件树选择笔记或文件打开，或点击顶栏「设置」进行外观配置',
      })
    );
    this.emptyNode.style.display = '';
    if (!this.emptyNode.parentElement) {
      this.container.append(this.emptyNode);
    }
  }

  async switchView(tab, appContext = {}) {
    this.activeTab = tab;

    if (!tab) {
      this.showEmpty();
      return;
    }

    this.container.dataset.empty = '0';
    if (this.emptyNode) this.emptyNode.style.display = 'none';

    // 隐藏所有其他活跃 DOM，并安全暂停音视频
    for (const [key, el] of this.viewCache) {
      el.style.display = 'none';
      if (typeof el.onDeactivate === 'function') {
        try { el.onDeactivate(); } catch (err) { console.warn('[viewHost] onDeactivate error', err); }
      }
      const media = el.querySelectorAll ? el.querySelectorAll('video, audio') : [];
      media.forEach((m) => {
        if (!m.paused) {
          try { m.pause(); } catch (e) {}
        }
      });
    }

    const fileId = tab.id || tab.path || tab.value;
    const isMd = tab.type === 'note' || tab.type === 'markdown' || (!tab.type && isMarkdown(fileId));

    // 1. Markdown 笔记
    if (isMd) {
      this.noteViewNode.style.display = '';
      if (appContext.renderNote) {
        await appContext.renderNote(tab.path || fileId);
      }
      return;
    }

    // 2. 设置界面
    if (tab.type === 'settings') {
      let settingsEl = this.viewCache.get('settings');
      if (!settingsEl) {
        settingsEl = createSettingsView({
          vault: this.vault,
          onFontChange: this.onFontChange,
          onThemeChange: this.onThemeChange,
          onPickFolder: this.onPickFolder,
          onOpenInExplorer: this.onOpenInExplorer,
        });
        this.viewCache.set('settings', settingsEl);
        this.container.append(settingsEl);
      }
      settingsEl.style.display = '';
      return;
    }

    // 3. 其他非 Markdown 文件（PDF、图片、视频、音频、Office、代码等）
    let viewEl = this.viewCache.get(fileId);
    if (!viewEl) {
      const targetPath = tab.path || fileId;
      const file = this.vault.noteAt(targetPath) || {
        path: targetPath,
        name: tab.label || basename(targetPath),
        ext: targetPath.slice(targetPath.lastIndexOf('.') + 1).toLowerCase(),
        fileType: tab.type,
      };

      if (tab.type === 'pdf') {
        viewEl = createPdfView({ file, vault: this.vault });
      } else if (['image', 'video', 'audio'].includes(tab.type)) {
        viewEl = createMediaView({ file, vault: this.vault });
      } else if (tab.type === 'code') {
        viewEl = createCodeView({ file, vault: this.vault });
      } else if (['word', 'excel', 'ppt'].includes(tab.type)) {
        viewEl = createOfficeView({ file, vault: this.vault });
      } else {
        viewEl = createCodeView({ file, vault: this.vault });
      }

      this.viewCache.set(fileId, viewEl);
      this.container.append(viewEl);
    }

    viewEl.style.display = '';
  }

  destroyView(tabId) {
    const el = this.viewCache.get(tabId);
    if (el && tabId !== '__note__') {
      el.remove();
      this.viewCache.delete(tabId);
    }
  }
}
