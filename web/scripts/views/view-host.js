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
    this.pageMounted = new Set(); // 通用页面里已经跑过 mount 的 tabId
    this.pageControllers = new Map(); // tabId -> mount 返回的控制器
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

    // 3. 通用页面：tab.mount(host, ctx) 由调用方提供。
    //    插件页、未来的 AI 面板都走这一条，ViewHost 不需要为每个页面加分支。
    if (typeof tab.mount === 'function') {
      let pageEl = this.viewCache.get(fileId);
      if (!pageEl) {
        // l-page-inset：整页视图必须让出顶栏那一行，否则第一屏标题压在胶囊底下
        pageEl = h('div', { class: 'c-page-host l-page-inset' });
        this.viewCache.set(fileId, pageEl);
        this.container.append(pageEl);
      }
      pageEl.style.display = '';

      // mount 只跑一次：页面自己保留状态，切走再切回来不重新初始化
      if (!this.pageMounted.has(fileId)) {
        this.pageMounted.add(fileId);
        try {
          const ctl = await tab.mount(pageEl, appContext);
          if (ctl) this.pageControllers.set(fileId, ctl);
        } catch (err) {
          // 页面自己崩了也不能把整个文档区留白：留一张错误卡，
          // 插件停用时会 destroyView，下次启用再重新挂
          console.warn(`[viewHost] 页面 ${fileId} 挂载失败`, err);
          fill(
            pageEl,
            Empty({
              icon: 'x-circle',
              title: '页面加载失败',
              desc: String(err?.message || err),
            })
          );
        }
      }
      return;
    }

    // 4. 其他非 Markdown 文件（PDF、图片、视频、音频、Office、代码等）
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
      const ctl = this.pageControllers.get(tabId);
      try {
        ctl?.destroy?.();
      } catch (err) {
        console.warn('[viewHost] page destroy error', err);
      }
      this.pageControllers.delete(tabId);
      this.pageMounted.delete(tabId);
      el.remove();
      this.viewCache.delete(tabId);
    }
  }
}
