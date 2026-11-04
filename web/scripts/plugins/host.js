/* ==========================================================================
   Inkdown · 插件宿主
   负责「把文件夹里的插件跑起来、再干净地停下来」。

   目录约定（全部相对仓库根）：
     .inkdown/plugins/<id>/manifest.json   插件身份
     .inkdown/plugins/<id>/main.js         入口，export activate(ctx)（可返回 dispose）
     .inkdown/plugins/<id>/style.css       可选，启用时注入 <style data-plugin="<id>">
     .inkdown/plugins/<id>/data.json       插件私有数据，由 ctx.storage 代管
     .inkdown/plugins.json                 宿主记账：enabled / granted

   状态为什么不放插件目录：插件目录是「插件作者的东西」，账本必须能被用户
   单独重置（删掉 plugins.json 等于重置全部授权），而且插件目录整个被覆盖安装时
   不该把启用状态一起冲掉。

   ---- 内置插件（另一个代理按这个约定放进文件即可）----
   随应用分发的插件源码放 web/builtin-plugins/<id>/，清单是
   web/builtin-plugins/index.json：
       { "version": 1, "plugins": [ { "id": "ai", "files": ["manifest.json", "main.js", "style.css"] } ] }
   宿主启动时若 .inkdown/plugins/<id>/manifest.json 不存在，就按 files 用 fetch()
   逐个读出来写进仓库。文件列表必须写全 —— 宿主不会去猜插件目录里有什么。
   index.json 里登记随应用分发的插件；当前只有 AI 写作助手一项。
   ========================================================================== */

import { h } from '../core/dom.js';
import { Dialog, Button, toast } from '../ui/kit.js';
import { renderMarkdown } from '../markdown.js';
import { PLUGINS_DIR } from '../vault/vault.js';
import { isDesktop } from '../core/platform.js';
import { buildPlugin, cacheBust } from './loader.js';
import * as registry from './registry.js';
import { describePermission, PERMISSIONS } from './permissions.js';
import { on as onEvent } from './events.js';

/** 宿主版本：manifest.minApp 与 ctx.app.version 共用这一个值 */
export const APP_VERSION = '0.1.0';

const STATE_PATH = '.inkdown/plugins.json';
const BUILTIN_BASE = new URL('../../builtin-plugins/', import.meta.url);
const BUILTIN_INDEX = new URL('index.json', BUILTIN_BASE);

const ID_RE = /^[a-zA-Z0-9][a-zA-Z0-9._-]*$/;

/** 简化版 semver 比较，只认 x.y.z，够 manifest.minApp 用 */
function compareVersion(a, b) {
  const parse = (v) =>
    String(v || '')
      .split('.')
      .map((n) => Number.parseInt(n, 10) || 0);
  const [x, y, z] = parse(a);
  const [p, q, r] = parse(b);
  return x - p || y - q || z - r;
}

/** manifest 的全部校验规则集中在这里，scan 与 enable 复用 */
function validateManifest(manifest, dirId) {
  if (!manifest || typeof manifest !== 'object' || Array.isArray(manifest)) {
    return 'manifest.json 不是一个对象';
  }
  if (!manifest.id) return 'manifest 缺少 id';
  if (!ID_RE.test(String(manifest.id))) return 'id 只能包含字母、数字、点、下划线和连字符';
  if (manifest.id !== dirId) return `manifest 的 id「${manifest.id}」与文件夹名「${dirId}」不一致`;
  if (!manifest.name) return 'manifest 缺少 name';
  if (!manifest.version) return 'manifest 缺少 version';
  if (manifest.minApp && compareVersion(APP_VERSION, manifest.minApp) < 0) {
    return `需要 Inkdown ${manifest.minApp} 或更高版本（当前 ${APP_VERSION}）`;
  }
  if (manifest.main && typeof manifest.main !== 'string') return 'manifest.main 必须是字符串';
  if (manifest.permissions !== undefined && !Array.isArray(manifest.permissions)) {
    return 'manifest.permissions 必须是数组';
  }
  for (const p of manifest.permissions || []) {
    if (!PERMISSIONS[p]) return `未知权限「${p}」`;
  }
  return '';
}

/** 授权弹窗。返回 Promise<boolean>，只有点「信任并启用」才为 true。 */
function askPermissions(record) {
  return new Promise((resolve) => {
    let done = false;
    const finish = (v) => {
      if (done) return;
      done = true;
      resolve(v);
    };

    const rows = record.permissions.map((name) => {
      const info = describePermission(name);
      return h(
        'div',
        { class: 'c-plugin-perm' },
        h('span', { class: 'c-plugin-perm__name' }, info.label),
        h('span', { class: 'c-plugin-perm__desc' }, info.desc)
      );
    });

    const ctl = Dialog({
      title: `允许「${record.name}」访问？`,
      desc: '插件需要以下能力才能运行。授权会记在仓库里，之后启用不再询问。',
      icon: 'puzzle',
      body: h('div', { class: 'l-stack l-stack--2' }, rows),
      actions: [
        Button({
          label: '取消',
          variant: 'plain',
          onClick: () => {
            finish(false);
            ctl.close();
          },
        }),
        Button({
          label: '信任并启用',
          icon: 'check',
          variant: 'primary',
          onClick: () => {
            finish(true);
            ctl.close();
          },
        }),
      ],
      onClose: () => finish(false),
    });
  });
}

export class PluginHost {
  /**
   * @param {object} o
   * @param {import('../vault/vault.js').Vault} o.vault
   * @param {object} [o.editor] 编辑器桥：getSelection / replaceSelection / insertAtCursor /
   *                            currentNote / onSelectionChange。缺了就让 editor.* 抛错。
   */
  constructor({ vault, editor = null } = {}) {
    this.vault = vault;
    this.editorBridge = editor;
    this.plugins = new Map(); // id -> record（磁盘上的插件，含错误）
    this.runtimes = new Map(); // id -> 运行中的资源
    this.state = { version: 1, enabled: [], granted: {} };
    this.listeners = new Set();
    this.busy = new Set();
  }

  /* ---------------- 状态 ---------------- */

  subscribe(fn) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  notify() {
    for (const fn of [...this.listeners]) {
      try {
        fn(this.list());
      } catch (err) {
        console.warn('[plugins] 监听器抛错', err);
      }
    }
  }

  async readState() {
    const fallback = { version: 1, enabled: [], granted: {} };
    let raw = null;
    try {
      raw = await this.vault.fs.read(STATE_PATH);
    } catch (err) {
      console.warn('[plugins] 读取 plugins.json 失败', err);
    }
    if (!raw) return fallback;
    try {
      const data = JSON.parse(raw);
      return {
        version: 1,
        enabled: Array.isArray(data.enabled) ? data.enabled.filter((x) => typeof x === 'string') : [],
        granted: data.granted && typeof data.granted === 'object' ? data.granted : {},
      };
    } catch (err) {
      console.warn('[plugins] plugins.json 解析失败，按空账本处理', err);
      return fallback;
    }
  }

  async saveState() {
    this.state.enabled = [...new Set(this.state.enabled)];
    await this.vault.fs.write(STATE_PATH, JSON.stringify(this.state, null, 2));
  }

  /* ---------------- 内置插件 ---------------- */

  /**
   * 把随应用分发的插件补写进仓库。
   *
   * 两种情况会写：仓库里还没有这个插件；或本地版本低于内置版本。
   * 同版本 / 本地更高版本一律不碰 —— 用户改过的插件不能被覆盖，
   * 而「上一版已经装过」的用户必须能收到内置插件的升级（否则旧面板会一直留着）。
   */
  async syncBuiltins() {
    let index = null;
    try {
      const res = await fetch(BUILTIN_INDEX);
      if (!res.ok) return 0;
      index = await res.json();
    } catch {
      // 浏览器直接开 file://、或内置清单被裁掉时都会走到这里 —— 不是错误
      return 0;
    }

    const list = Array.isArray(index?.plugins) ? index.plugins : [];
    let installed = 0;

    for (const item of list) {
      const id = typeof item === 'string' ? item : item?.id;
      if (!id || !ID_RE.test(id)) {
        console.warn('[plugins] 内置清单里有非法 id', item);
        continue;
      }
      const files = typeof item === 'string' ? ['manifest.json', 'main.js'] : item?.files;
      if (!Array.isArray(files) || !files.length) {
        console.warn(`[plugins] 内置插件 ${id} 没有给出文件列表`);
        continue;
      }

      const dir = `${PLUGINS_DIR}/${id}`;
      const manifestPath = `${dir}/manifest.json`;
      const hasLocal = await this.vault.fs.exists(manifestPath);

      // 先读内置版本；读不到就不升级（宁可保留旧版，也不要写进半套文件）
      let builtinVersion = '';
      try {
        const res = await fetch(new URL(`${id}/manifest.json`, BUILTIN_BASE));
        if (res.ok) builtinVersion = String((await res.json())?.version || '');
      } catch {
        builtinVersion = '';
      }

      if (hasLocal) {
        let localVersion = '';
        try {
          localVersion = String(JSON.parse(await this.vault.fs.read(manifestPath))?.version || '');
        } catch {
          localVersion = '';
        }
        if (!builtinVersion || compareVersion(builtinVersion, localVersion) <= 0) continue;
      }

      await this.vault.fs.mkdir(dir);
      for (const file of files) {
        try {
          const res = await fetch(new URL(`${id}/${file}`, BUILTIN_BASE));
          if (!res.ok) throw new Error(`HTTP ${res.status}`);
          const text = await res.text();
          await this.vault.fs.write(`${dir}/${file}`, text);
        } catch (err) {
          console.warn(`[plugins] 内置插件 ${id} 的 ${file} 写入失败`, err);
        }
      }
      installed += 1;
    }

    return installed;
  }

  /* ---------------- 扫描 ---------------- */

  /**
   * 读插件目录 + manifest。会顺带清掉「目录已经没了但还在跑」的插件。
   * 返回给插件页用的公开列表。
   */
  async scan() {
    this.state = await this.readState();

    const children = await this.vault.fs.list(PLUGINS_DIR);
    // name 必须过滤：拿不到名字的条目不能变成一行 id 为 undefined 的幽灵插件
    const dirs = children.filter((e) => e.type === 'dir' && e.name).map((e) => e.name);

    const next = new Map();
    for (const name of dirs) {
      next.set(name, await this.readRecord(name));
    }

    // 目录被删掉的插件必须停掉，否则 blob 和注册项会一直挂着
    for (const id of [...this.runtimes.keys()]) {
      if (next.has(id)) continue;
      await this.teardown(id);
      this.state.enabled = this.state.enabled.filter((x) => x !== id);
    }

    this.plugins = next;

    const stale = [...this.state.enabled].filter((id) => !next.has(id));
    if (stale.length) {
      this.state.enabled = this.state.enabled.filter((id) => next.has(id));
      await this.saveState();
    }

    this.notify();
    return this.list();
  }

  async readRecord(id) {
    const dir = `${PLUGINS_DIR}/${id}`;
    const record = {
      id,
      dir,
      name: id,
      version: '',
      description: '',
      author: '',
      permissions: [],
      enabled: this.state.enabled.includes(id),
      granted: this.state.granted[id] || [],
      active: this.runtimes.has(id),
      error: '',
      manifest: null,
    };

    let raw = null;
    try {
      raw = await this.vault.fs.read(`${dir}/manifest.json`);
    } catch (err) {
      console.warn(`[plugins] 读取 ${dir}/manifest.json 失败`, err);
    }

    if (!raw) {
      record.error = '缺少 manifest.json';
      return record;
    }

    let manifest = null;
    try {
      manifest = JSON.parse(raw);
    } catch (err) {
      record.error = `manifest.json 解析失败：${err.message}`;
      return record;
    }

    record.manifest = manifest;
    record.name = String(manifest?.name || id);
    record.version = String(manifest?.version || '');
    record.description = String(manifest?.description || '');
    record.author = String(manifest?.author || '');
    record.permissions = Array.isArray(manifest?.permissions) ? [...manifest.permissions] : [];
    record.error = validateManifest(manifest, id);
    return record;
  }

  /** 公开列表：给插件页，不带 manifest 原文与内部对象 */
  list() {
    return [...this.plugins.values()].map((p) => ({
      id: p.id,
      dir: p.dir,
      name: p.name,
      version: p.version,
      description: p.description,
      author: p.author,
      permissions: [...p.permissions],
      enabled: p.enabled,
      granted: [...p.granted],
      active: this.runtimes.has(p.id),
      error: p.error,
    }));
  }

  get(id) {
    return this.plugins.get(id) || null;
  }

  /* ---------------- 启停 ---------------- */

  /**
   * 启用。任何一步失败都只影响这个插件：注册项回滚、blob 撤销、错误记在 record 上。
   * @returns {Promise<boolean>} 是否真的跑起来了
   */
  async enable(id, { silent = false } = {}) {
    if (this.busy.has(id)) return false;
    this.busy.add(id);
    try {
      const record = this.plugins.get(id);
      if (!record) throw new Error(`插件 ${id} 不存在，先重新扫描`);
      if (!record.manifest || record.error) {
        throw new Error(record.error || 'manifest.json 无效，不能启用');
      }
      if (this.runtimes.has(id)) return true;

      const allowed = await this.ensureGranted(record, silent);
      if (!allowed) {
        // 启动恢复时缺授权不是错误，但要让用户知道「开关是开的，人没跑起来」
        if (silent) {
          record.error = '缺少权限授权，自动启用已跳过';
          this.notify();
        }
        return false;
      }

      try {
        await this.activate(record);
      } catch (err) {
        await this.teardown(id);
        record.error = String(err?.message || err);
        record.active = false;
        record.enabled = false;
        this.state.enabled = this.state.enabled.filter((x) => x !== id);
        await this.saveState();
        console.warn(`[plugins] 插件 ${id} 启动失败`, err);
        this.notify();
        return false;
      }

      record.error = '';
      record.enabled = true;
      record.active = true;
      record.granted = [...(this.state.granted[id] || [])];
      this.state.enabled = [...new Set([...this.state.enabled, id])];
      await this.saveState();
      this.notify();
      return true;
    } catch (err) {
      const record = this.plugins.get(id);
      if (record) record.error = String(err?.message || err);
      console.warn(`[plugins] 启用 ${id} 失败`, err);
      this.notify();
      return false;
    } finally {
      this.busy.delete(id);
    }
  }

  async disable(id) {
    await this.teardown(id);
    this.state.enabled = this.state.enabled.filter((x) => x !== id);
    const record = this.plugins.get(id);
    if (record) {
      record.enabled = false;
      record.active = false;
    }
    await this.saveState();
    this.notify();
  }

  async reload(id) {
    await this.disable(id);
    return this.enable(id);
  }

  /** 启动时恢复上次启用的插件。缺授权就跳过 —— 启动过程不弹窗。 */
  async restore() {
    for (const id of [...this.state.enabled]) {
      const record = this.plugins.get(id);
      if (!record || record.error || this.runtimes.has(id)) continue;
      await this.enable(id, { silent: true });
    }
  }

  /** 初始化：读账本 → 补内置插件 → 扫描 → 恢复启用状态 */
  async init() {
    this.state = await this.readState();
    await this.syncBuiltins();
    await this.scan();
    await this.restore();
    this.notify();
    return this.list();
  }

  /* ---------------- 运行时 ---------------- */

  async ensureGranted(record, silent) {
    const granted = this.state.granted[record.id] || [];
    if (record.permissions.every((p) => granted.includes(p))) return true;

    // 启动恢复不弹窗：用户没坐在屏幕前，弹窗只会被当成故障
    if (silent) return false;

    const ok = await askPermissions(record);
    if (!ok) return false;

    this.state.granted[record.id] = [...record.permissions];
    await this.saveState();
    record.granted = [...record.permissions];
    return true;
  }

  /** 真正加载一个插件：样式 → 存储 → 模块 → activate */
  async activate(record) {
    const runtime = {
      id: record.id,
      urls: [],
      styleEl: null,
      dispose: null,
      storage: null,
      eventOffs: [],
    };
    // 先占位：activate 期间注册的东西都归这个插件，失败时能一起回滚
    this.runtimes.set(record.id, runtime);

    try {
      const manifest = record.manifest;

      if (manifest.style) {
        const css = await this.vault.fs.read(`${record.dir}/${manifest.style}`);
        if (css) {
          const styleEl = document.createElement('style');
          styleEl.dataset.plugin = record.id;
          styleEl.textContent = css;
          document.head.append(styleEl);
          runtime.styleEl = styleEl;
        }
      }

      const dataRaw = await this.vault.fs.read(`${record.dir}/data.json`);
      let data = {};
      if (dataRaw) {
        try {
          const parsed = JSON.parse(dataRaw);
          if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) data = parsed;
        } catch (err) {
          console.warn(`[plugins] ${record.id}/data.json 解析失败，按空数据处理`, err);
        }
      }
      runtime.storage = this.createStorage(record, runtime, data);

      const built = await buildPlugin({
        vault: this.vault,
        dir: record.dir,
        main: manifest.main || 'main.js',
      });
      runtime.urls = built.urls;

      const ctx = this.createCtx(record, runtime);
      // 入口加缓存版本：同名 blob 不会复用，热重载才拿得到新模块
      const mod = await import(/* @vite-ignore */ cacheBust(built.entryUrl));

      if (typeof mod.activate !== 'function') {
        throw new Error('main.js 需要 export async function activate(ctx)');
      }

      const dispose = await mod.activate(ctx);
      if (typeof dispose === 'function') runtime.dispose = dispose;
    } catch (err) {
      await this.teardown(record.id);
      throw err;
    }
  }

  /**
   * 拆卸。顺序：插件自己的 dispose → 注册项（逆序）→ 样式 → blob → 存储落盘。
   * 每一步单独 try：某一步写坏了不能把后面的清理卡住（否则 blob 会泄一辈子）。
   */
  async teardown(id) {
    const runtime = this.runtimes.get(id);
    if (!runtime) {
      // 没有运行时也要扫一遍注册表：activate 可能在建 runtime 之前就抛过
      registry.disposePlugin(id);
      return;
    }
    this.runtimes.delete(id);

    try {
      runtime.dispose?.();
    } catch (err) {
      console.warn(`[plugins] 插件 ${id} 的 dispose 抛错`, err);
    }

    registry.disposePlugin(id);

    for (const off of runtime.eventOffs.splice(0)) {
      try {
        off?.();
      } catch {}
    }

    try {
      runtime.styleEl?.remove();
    } catch {}

    for (const url of runtime.urls) {
      try {
        URL.revokeObjectURL(url);
      } catch {}
    }
    runtime.urls = [];

    try {
      await runtime.storage?.flush();
    } catch (err) {
      console.warn(`[plugins] 插件 ${id} 的数据落盘失败`, err);
    }
  }

  /** 窗口关闭前把所有插件的 storage 立即落盘（防抖可能还压着最后一次 set） */
  flushAll() {
    for (const runtime of this.runtimes.values()) {
      try {
        runtime.storage?.flush();
      } catch {}
    }
  }

  /* ---------------- ctx ---------------- */

  createStorage(record, runtime, data) {
    const path = `${record.dir}/data.json`;
    const snapshot = data;
    let timer = 0;

    const flush = async () => {
      if (timer) {
        clearTimeout(timer);
        timer = 0;
      }
      try {
        await this.vault.fs.write(path, JSON.stringify(snapshot, null, 2));
      } catch (err) {
        console.warn(`[plugins] 写入 ${path} 失败`, err);
      }
    };

    const storage = {
      get(key, fallback) {
        return Object.prototype.hasOwnProperty.call(snapshot, key) ? snapshot[key] : fallback;
      },
      set(key, value) {
        snapshot[key] = value;
        if (timer) clearTimeout(timer);
        // 防抖：插件可能每敲一个字就 set 一次，不能每次都落盘
        timer = setTimeout(flush, 400);
      },
      flush,
    };
    return storage;
  }

  /**
   * 插件上下文。这份形状是契约，插件（含 AI 插件）按它写，宿主不要随手加字段：
   *
   *   ctx = {
   *     id, version,
   *     app:     { version },
   *     vault:   { read, write, exists, list, walk },        // 需 vault:read / vault:write
   *     editor:  { getSelection, replaceSelection, insertAtCursor, setContent, currentNote, onSelectionChange },
   *     ui:      { navItem, page, sidebarView, command, settingsSection, toast, dialog,
   *                markdown, openSidebar },
   *     storage: { get(key, fallback), set(key, value) },    // data.json，防抖落盘
   *     net:     { fetch(url, init) },                       // 需 net
   *     clipboard: { readText, writeText },                  // 需 clipboard
   *     events:  { on(name, fn) },                           // note:open / note:save / theme:change
   *   }
   *
   * ui.* 的五个注册函数都返回 disposer；page() 返回的 disposer 额外带 .open()。
   * ui.dialog(options) 直接返回 kit 的 Dialog 控制器（{ close }），按钮由插件自己给。
   * ui.markdown(text) 转发宿主的 renderMarkdown（插件不能 import 宿主模块）。
   * ui.openSidebar(id) 展开右侧栏并切到指定视图；视图不存在时返回 false。
   * 未声明的能力调用时直接抛错 —— 静默失败会让插件作者以为「功能没生效」而不是「没权限」。
   */
  createCtx(record, runtime) {
    const id = record.id;
    const perms = new Set(record.permissions);
    const need = (perm, method) => {
      if (!perms.has(perm)) {
        throw new Error(`插件「${id}」没有 ${perm} 权限，不能调用 ${method}`);
      }
    };
    const bridge = () => {
      if (!this.editorBridge) throw new Error('编辑器还没有就绪');
      return this.editorBridge;
    };
    const fs = this.vault.fs;

    return {
      id,
      version: record.manifest.version,
      app: { version: APP_VERSION },

      vault: {
        read: async (path) => {
          need('vault:read', 'vault.read');
          return (await fs.read(path)) ?? '';
        },
        write: async (path, text) => {
          need('vault:write', 'vault.write');
          await fs.write(path, String(text ?? ''));
          // 这个文件可能正躺在 vault 的读取缓存里，插件改完必须让缓存失效，
          // 否则宿主下一次打开拿到的还是旧正文
          this.vault.cache?.delete(path);
        },
        exists: async (path) => {
          need('vault:read', 'vault.exists');
          return Boolean(await fs.exists(path));
        },
        list: async (path = '') => {
          need('vault:read', 'vault.list');
          return fs.list(path);
        },
        walk: async () => {
          need('vault:read', 'vault.walk');
          return fs.walk();
        },
      },

      editor: {
        getSelection: () => bridge().getSelection(),
        replaceSelection: (text) => bridge().replaceSelection(text),
        insertAtCursor: (text) => bridge().insertAtCursor(text),
        setContent: (text) => bridge().setContent(text),
        currentNote: () => bridge().currentNote(),
        onSelectionChange: (fn) => {
          const off = bridge().onSelectionChange(fn);
          runtime.eventOffs.push(off);
          return off;
        },
      },

      ui: {
        navItem: (spec) => registry.addNavItem(id, spec),
        page: (spec) => registry.addPage(id, spec),
        sidebarView: (spec) => registry.addSidebarView(id, spec),
        command: (spec) => registry.addCommand(id, spec),
        settingsSection: (spec) => registry.addSettingsSection(id, spec),
        // toast / dialog 直接转 kit：插件拿到的就是宿主那一套，不用自己画
        toast: (message, options) => toast(message, options),
        dialog: (options) => Dialog(options),
        // 渲染能力与「打开右栏」都只有宿主有：插件用 blob 加载，import 不到宿主模块
        markdown: (text) => renderMarkdown(text),
        openSidebar: (viewId) => registry.openSidebar(viewId),
      },

      storage: runtime.storage,

      net: {
        fetch: (url, init) => {
          need('net', 'net.fetch');
          return proxyFetch(url, init);
        },
      },

      // 契约里只列了 clipboard 权限，没列方法；这里补上对应实现，
      // 否则那条权限在 ctx 里无处落地，只能变成摆设。
      clipboard: {
        readText: async () => {
          need('clipboard', 'clipboard.readText');
          if (!navigator.clipboard?.readText) throw new Error('当前环境不支持读取剪贴板');
          return navigator.clipboard.readText();
        },
        writeText: async (text) => {
          need('clipboard', 'clipboard.writeText');
          if (!navigator.clipboard?.writeText) throw new Error('当前环境不支持写入剪贴板');
          await navigator.clipboard.writeText(String(text ?? ''));
        },
      },

      events: {
        on: (name, fn) => {
          const off = onEvent(name, fn);
          runtime.eventOffs.push(off);
          return off;
        },
      },
    };
  }
}

/**
 * 网络请求。
 *
 * 桌面端的 webview 里 fetch 仍受 CORS 约束，真正的出口应该是 Rust 侧代理。
 * 本仓库目前还没有 net_fetch 命令，所以这里先尝试调用、失败就回退到原生 fetch ——
 * 插件不会因为宿主少一个命令就直接崩。接 Rust 代理时只要实现：
 *   net_fetch(url, method, headers, body) -> { status, statusText, headers, body }
 * 这段代码不用改。
 */
async function proxyFetch(url, init = {}) {
  if (isDesktop && globalThis.__TAURI__?.core) {
    try {
      const res = await globalThis.__TAURI__.core.invoke('net_fetch', {
        url: String(url),
        method: init.method || 'GET',
        headers: init.headers || {},
        body: typeof init.body === 'string' ? init.body : null,
      });
      return new Response(res?.body ?? null, {
        status: res?.status ?? 200,
        statusText: res?.statusText ?? '',
        headers: res?.headers ?? {},
      });
    } catch {
      // 命令不存在 / 代理未启用：静默回退，不打断插件
    }
  }
  return fetch(url, init);
}
