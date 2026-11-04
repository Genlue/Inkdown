/* ==========================================================================
   Inkdown · 插件事件总线
   只有宿主知道「笔记开了 / 存了 / 主题换了」，插件通过 ctx.events.on 订阅。
   订阅者的异常必须就地吞掉：一个插件写错事件处理，不能连累其它插件和宿主。
   ========================================================================== */

const listeners = new Map();

/**
 * 订阅事件，返回 disposer。
 * 已知事件：'note:open' | 'note:save' | 'theme:change'
 * 名字不做白名单 —— 宿主以后加事件时，老插件不用改。
 */
export function on(name, fn) {
  if (typeof fn !== 'function') throw new TypeError('events.on(name, fn) 需要一个函数');
  let set = listeners.get(name);
  if (!set) listeners.set(name, (set = new Set()));
  set.add(fn);
  return () => {
    set.delete(fn);
    if (!set.size) listeners.delete(name);
  };
}

export function emit(name, payload) {
  const set = listeners.get(name);
  if (!set) return;
  // 复制一份再遍历：订阅者在回调里退订是正常操作
  for (const fn of [...set]) {
    try {
      fn(payload);
    } catch (err) {
      console.warn(`[plugins] 事件 ${name} 的订阅者抛错`, err);
    }
  }
}

/** 当前订阅数，自检/调试用 */
export function listenerCount(name) {
  return listeners.get(name)?.size ?? 0;
}
