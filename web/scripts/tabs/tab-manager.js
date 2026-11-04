/* ==========================================================================
   Inkdown · TabManager
   基于 Golden Gate Tabs 的多页面常驻、可关闭、可拖拽排序页签管理器
   ========================================================================== */

import { h } from '../core/dom.js';
import { Tabs } from '../ui/kit.js';

export class TabManager {
  constructor({ container, onSelect, onClose, onReorder }) {
    this.container = container;
    this.onSelect = onSelect;
    this.onClose = onClose;
    this.onReorder = onReorder;

    this.tabs = [];
    this.activeId = null;

    this.tabsComponent = Tabs({
      tabs: [],
      value: null,
      closable: true,
      draggable: true,
      className: 'ink-doc-tabs c-tabs--plain',
      onChange: (val) => {
        this.select(val);
      },
      onClose: (val) => {
        this.close(val);
      },
      onReorder: (newTabs) => {
        this.tabs = newTabs;
        this.onReorder?.(this.tabs);
      },
    });

    this.container.append(this.tabsComponent);
  }

  getTab(id) {
    return this.tabs.find((t) => t.value === id) || null;
  }

  getActiveTab() {
    return this.getTab(this.activeId);
  }

  setTabs(tabs, activeId = null) {
    this.tabs = Array.isArray(tabs) ? [...tabs] : [];
    this.activeId = activeId ?? (this.tabs[0]?.value || null);
    this.tabsComponent.setTabs(this.tabs);
    if (this.activeId) {
      this.tabsComponent.set(this.activeId);
    }
  }

  open({ id, label, icon = 'file-text', type = 'note', path = '' }) {
    const existing = this.getTab(id);
    if (existing) {
      this.select(id);
      return existing;
    }

    const tab = {
      value: id,
      label: label || '未命名',
      icon,
      type,
      path,
      title: label || path || id,
    };

    this.tabs.push(tab);
    this.tabsComponent.setTabs(this.tabs);
    this.select(id);
    this.onReorder?.(this.tabs);
    return tab;
  }

  select(id) {
    if (this.activeId === id) return;
    this.activeId = id;
    this.tabsComponent.set(id);
    this.onSelect?.(this.getTab(id));
  }

  close(id) {
    const idx = this.tabs.findIndex((t) => t.value === id);
    if (idx === -1) return;

    const [closedTab] = this.tabs.splice(idx, 1);
    this.tabsComponent.setTabs(this.tabs);

    if (this.activeId === id) {
      const nextTab = this.tabs[idx] || this.tabs[idx - 1] || null;
      this.activeId = nextTab ? nextTab.value : null;
      if (this.activeId) {
        this.tabsComponent.set(this.activeId);
      }
      this.onSelect?.(nextTab);
    }

    this.onClose?.(closedTab, this.getActiveTab());
    this.onReorder?.(this.tabs);
  }

  closeOthers(id) {
    const keep = this.tabs.find((t) => t.value === id);
    if (!keep) return;
    const removed = this.tabs.filter((t) => t.value !== id);
    this.tabs = [keep];
    this.tabsComponent.setTabs(this.tabs);
    this.select(id);
    for (const r of removed) {
      this.onClose?.(r, keep);
    }
    this.onReorder?.(this.tabs);
  }

  closeAll() {
    const removed = [...this.tabs];
    this.tabs = [];
    this.activeId = null;
    this.tabsComponent.setTabs(this.tabs);
    this.onSelect?.(null);
    for (const r of removed) {
      this.onClose?.(r, null);
    }
    this.onReorder?.(this.tabs);
  }

  updateLabel(id, newLabel) {
    const tab = this.getTab(id);
    if (tab) {
      tab.label = newLabel;
      tab.title = newLabel;
      this.tabsComponent.setTabs(this.tabs);
      this.tabsComponent.set(this.activeId);
    }
  }
}
