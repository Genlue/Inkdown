/* ==========================================================================
   Inkdown · 右侧栏视图注册表
   右侧栏只是容器。任何视图只要提供 { id, label, mount } 就能挂进去：
   大纲、属性、反链、AI 面板都是同一层的东西。

   mount(host, ctx) 返回一个控制器：
     { refresh?(), setActive?(idx), destroy?() }
   ctx 由宿主提供（见 app.js），视图不要直接摸全局状态。
   ========================================================================== */

const views = new Map();

export function registerSidebarView(view) {
  if (!view || !view.id || !view.label || typeof view.mount !== 'function') {
    throw new Error('sidebar view 需要 { id, label, mount(host, ctx) }');
  }
  views.set(view.id, view);
  return () => views.delete(view.id);
}

export function unregisterSidebarView(id) {
  views.delete(id);
}

export function getSidebarViews() {
  return [...views.values()];
}

export function getSidebarView(id) {
  return views.get(id) || null;
}
