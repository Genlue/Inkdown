/* ==========================================================================
   Inkdown · 插件注册表
   插件能往宿主里塞五样东西：导航项、整页视图、右侧栏视图、命令、设置分区。
   注册表只管「记下来 + 通知宿主重画」，不认识 app.js，也不碰 DOM（设置分区除外）。

   两条规矩：
   1. 每个注册函数都返回 disposer —— 插件停用、激活失败回滚，都靠它原地撤销。
   2. 每一条都带 pluginId —— 宿主可以按插件批量撤销，不用插件自己记住注册过什么。

   宿主接入点在 connect()：refreshNav / addSidebarView / openPage。
   注册表不知道这些怎么实现，只在需要时喊一声。
   ========================================================================== */

import { h } from '../core/dom.js';

/** 五类注册的容器。顺序就是渲染顺序，导航项的最终顺序另看 order。 */
const store = {
  navItems: [],
  pages: [],
  sidebarViews: [],
  commands: [],
  settingsSections: [],
};

/** 宿主钩子；启动时由 app.js 填。没填就调注册函数 = 编程错误，直接抛 */
const adapter = {
  refreshNav: null,
  addSidebarView: null,
  openSidebar: null,
  openPage: null,
  closePage: null,
};

const watchers = new Set();

function notify() {
  // 导航是由注册表快照驱动的：注册表一变就让宿主重画一遍。
  // 放在 notify 里而不是每个注册函数里，撤销与批量回收也能走同一条路。
  if (typeof adapter.refreshNav === 'function') {
    try {
      adapter.refreshNav();
    } catch (err) {
      console.warn('[plugins] 重画导航失败', err);
    }
  }
  for (const fn of [...watchers]) {
    try {
      fn();
    } catch (err) {
      console.warn('[plugins] 注册表监听器抛错', err);
    }
  }
}

/** 订阅注册表变化（新增 / 撤销）。返回退订函数。 */
export function subscribe(fn) {
  watchers.add(fn);
  return () => watchers.delete(fn);
}

/** 接上宿主。只覆盖传进来的键，方便分阶段接入。 */
export function connect(hooks = {}) {
  Object.assign(adapter, hooks);
}

function requireAdapter(name, label) {
  if (typeof adapter[name] !== 'function') {
    throw new Error(`插件宿主还没就绪：无法${label}（缺少 ${name} 钩子）`);
  }
}

function checkId(kind, pluginId, spec) {
  if (!spec || !spec.id) throw new Error(`${kind} 需要 id（来自插件 ${pluginId}）`);
  if (!pluginId) throw new Error(`${kind}「${spec.id}」没有归属插件`);
}

function track(kind, pluginId, spec, teardown = null) {
  const entry = { kind, pluginId, spec, teardown };
  store[kind].push(entry);
  notify();
  return entry;
}

function closePageIfNeeded(entry) {
  if (entry.kind !== 'pages') return;
  try {
    adapter.closePage?.(entry.spec.id);
  } catch (err) {
    console.warn(`[plugins] 关闭页面「${entry.spec.id}」失败`, err);
  }
}

function disposerOf(entry) {
  const off = () => {
    const list = store[entry.kind];
    const i = list.indexOf(entry);
    if (i < 0) return;
    list.splice(i, 1);
    closePageIfNeeded(entry);
    try {
      entry.teardown?.();
    } catch (err) {
      console.warn(`[plugins] 撤销 ${entry.kind}「${entry.spec?.id}」失败`, err);
    }
    notify();
  };
  return off;
}

/* ---------------- 五类注册 ---------------- */

/**
 * 导航项。order 小的排前面（默认 100），同 order 按 label 排。
 * onClick 由插件决定做什么 —— 通常配合 page() 返回的 off.open()。
 */
export function addNavItem(pluginId, spec = {}) {
  checkId('导航项', pluginId, spec);
  if (!spec.label) throw new Error(`导航项「${spec.id}」需要 label（来自插件 ${pluginId}）`);
  if (typeof spec.onClick !== 'function') {
    throw new Error(`导航项「${spec.id}」需要 onClick（来自插件 ${pluginId}）`);
  }
  return disposerOf(track('navItems', pluginId, { order: 100, ...spec }));
}

/**
 * 整页视图。走 ViewHost 的 page 分支：mount(host, ctx) 只调一次，之后复用缓存。
 * 返回值多挂了一个 `.open()`，插件拿它在导航项里打开自己的页面：
 *   const page = ctx.ui.page({ id:'x', title:'X', mount })
 *   ctx.ui.navItem({ id:'x', label:'X', icon:'puzzle', onClick: () => page.open() })
 */
export function addPage(pluginId, spec = {}) {
  checkId('页面', pluginId, spec);
  if (typeof spec.mount !== 'function') throw new Error(`页面「${spec.id}」需要 mount(host, ctx)`);
  if (['settings', 'plugins', '__note__'].includes(spec.id)) {
    throw new Error(`页面 id「${spec.id}」与内置视图冲突，换一个 id`);
  }
  const off = disposerOf(track('pages', pluginId, spec));
  off.open = () => {
    try {
      requireAdapter('openPage', '打开插件页面');
      adapter.openPage(spec);
    } catch (err) {
      console.warn('[plugins] 打开页面失败', err);
    }
  };
  return off;
}

/** 右侧栏视图。真正的挂载由宿主完成，这里只负责记住并转发撤销。 */
export function addSidebarView(pluginId, spec = {}) {
  checkId('右侧栏视图', pluginId, spec);
  if (!spec.label) throw new Error(`右侧栏视图「${spec.id}」需要 label`);
  if (typeof spec.mount !== 'function') throw new Error(`右侧栏视图「${spec.id}」需要 mount(host, ctx)`);
  requireAdapter('addSidebarView', '注册右侧栏视图');
  const teardown = adapter.addSidebarView(spec);
  return disposerOf(track('sidebarViews', pluginId, spec, teardown));
}

/**
 * 展开右侧栏并切到某个视图。
 * 插件注册了侧栏视图后，靠它把用户带过去 —— 不用自己拼 DOM，也不用知道
 * 宿主把右侧栏藏在哪个开关后面。视图不存在返回 false。
 */
export function openSidebar(viewId) {
  requireAdapter('openSidebar', '打开右侧栏视图');
  return Boolean(adapter.openSidebar(viewId));
}

/**
 * 命令。进 Ctrl+P 列表。
 * scope: 'global'（默认）| 'editor' —— 'editor' 只在打开笔记时出现。
 */
export function addCommand(pluginId, spec = {}) {
  checkId('命令', pluginId, spec);
  if (!spec.title) throw new Error(`命令「${spec.id}」需要 title`);
  if (typeof spec.run !== 'function') throw new Error(`命令「${spec.id}」需要 run()`);
  return disposerOf(track('commands', pluginId, { scope: 'global', ...spec }));
}

/** 设置分区。真正的挂载在 mountSettingsSections()，设置页负责调它。 */
export function addSettingsSection(pluginId, spec = {}) {
  checkId('设置分区', pluginId, spec);
  if (!spec.title) throw new Error(`设置分区「${spec.id}」需要 title`);
  if (typeof spec.mount !== 'function') throw new Error(`设置分区「${spec.id}」需要 mount(host)`);
  return disposerOf(track('settingsSections', pluginId, spec));
}

/* ---------------- 读取 ---------------- */

const byOrder = (a, b) =>
  (a.spec.order ?? 100) - (b.spec.order ?? 100) ||
  String(a.spec.label || '').localeCompare(String(b.spec.label || ''), 'zh-CN');

/** 给 refreshNav() 用的一次性快照：纯 spec 数组，不带内部条目对象 */
export function snapshot() {
  return {
    navItems: [...store.navItems].sort(byOrder).map((e) => e.spec),
    pages: store.pages.map((e) => e.spec),
    sidebarViews: store.sidebarViews.map((e) => e.spec),
    commands: [...store.commands].map((e) => e.spec),
    settingsSections: store.settingsSections.map((e) => e.spec),
  };
}

export function commands() {
  return [...store.commands].map((e) => e.spec);
}

/** 按插件号批量撤销（逆序，后注册的先撤）。插件停用与激活失败回滚都走这里。 */
export function disposePlugin(pluginId) {
  for (const kind of Object.keys(store)) {
    const list = store[kind];
    for (let i = list.length - 1; i >= 0; i -= 1) {
      const entry = list[i];
      if (entry.pluginId !== pluginId) continue;
      list.splice(i, 1);
      closePageIfNeeded(entry);
      try {
        entry.teardown?.();
      } catch (err) {
        console.warn(`[plugins] 回收 ${kind}「${entry.spec?.id}」失败`, err);
      }
    }
  }
  notify();
}

/** 仅供调试 / 自检 */
export function stats() {
  return Object.fromEntries(Object.entries(store).map(([k, v]) => [k, v.length]));
}

/* ---------------- 设置分区挂载 ---------------- */

function sectionNode(spec) {
  const body = h('div', { class: 'c-card' });
  const wrap = h(
    'section',
    { class: 'l-stack l-stack--3' },
    h('div', { class: 'c-section' }, spec.title),
    body
  );
  try {
    spec.mount(body);
  } catch (err) {
    console.warn(`[plugins] 设置分区「${spec.id}」挂载失败`, err);
    body.append(h('div', { class: 'c-plugin-error' }, String(err?.message || err)));
  }
  return wrap;
}

/**
 * 挂载当前所有设置分区，并跟着注册表变化重画。
 * 只有分区集合真的变了才重建 —— 插件加一个导航项不该把设置页整段重挂一遍。
 * @param {HTMLElement} host 设置页末尾的容器
 * @returns {() => void} 退订
 */
export function mountSettingsSections(host) {
  let signature = null;
  const paint = () => {
    const list = store.settingsSections.map((e) => e.spec);
    const next = list.map((s) => s.id).join('|');
    if (next === signature) return;
    signature = next;
    host.replaceChildren(...list.map(sectionNode));
  };
  paint();
  return subscribe(paint);
}
