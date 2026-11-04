/* ==========================================================================
   Inkdown · Vault
   仓库 = 一个文件夹。结构：

     <vault>/
     ├── .inkdown/
     │   ├── vault.json        仓库身份
     │   ├── settings.json     外观 / 编辑器设置
     │   ├── workspace.json    展开的目录、上次打开的笔记、视图状态
     │   ├── index.json        笔记元数据（收藏、创建时间）
     │   ├── trash.json        回收站索引
     │   └── plugins/          插件目录（预留）
     ├── Notes/                普通目录 = 文件夹
     │   └── 欢迎.md           笔记 = .md 文件
     └── .trash/               回收站（隐藏）

   上层只跟 Vault 打交道，不直接碰 FS。换 Tauri 后端时 Vault 不用改。
   ========================================================================== */

import {
  LocalFS,
  TauriFS,
  isTauri,
  fsKeyFor,
  listVaults,
  saveVaults,
  activeVaultId,
  setActiveVaultId,
  dirname,
  basename,
  joinPath,
  stripExt,
  isMarkdown,
  isHidden,
  getFileExt,
  getFileType,
} from './fs.js';
import { seedNotes, SEED_FOLDERS } from './seed.js';
import { snippetAround } from './links.js';

const META_PATH = '.inkdown/vault.json';
const SETTINGS_PATH = '.inkdown/settings.json';
const WORKSPACE_PATH = '.inkdown/workspace.json';
const INDEX_PATH = '.inkdown/index.json';
const TRASH_PATH = '.inkdown/trash.json';
const PLUGINS_DIR = '.inkdown/plugins';
const TRASH_DIR = '.trash';

const DEFAULT_SETTINGS = {
  version: 1,
  appearance: { mode: 'system', accent: '#0a84ff' },
  editor: { size: 'md' },
};

const DEFAULT_WORKSPACE = {
  version: 1,
  lastNote: '',
  expanded: [],
  sort: 'updated',
  mode: 'edit',
  sidebar: true,
  view: 'outline',
  split: 0.5,
};

const isObject = (v) => v && typeof v === 'object' && !Array.isArray(v);

function mergeDeep(base, patch) {
  const out = { ...base };
  for (const [k, v] of Object.entries(patch || {})) {
    out[k] = isObject(v) && isObject(base?.[k]) ? mergeDeep(base[k], v) : v;
  }
  return out;
}

export function sanitizeName(name) {
  return String(name ?? '')
    .replace(/[\\/:*?"<>|]/g, '')
    .replace(/^\.+/, '')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 80);
}

function sortNodes(nodes, sort, index) {
  const byName = (a, b) => a.name.localeCompare(b.name, 'zh-CN');

  nodes.sort((a, b) => {
    if (a.type !== b.type) return a.type === 'dir' ? -1 : 1;
    if (a.type === 'dir') return byName(a, b);

    if (sort === 'title') return byName(a, b);
    if (sort === 'created') {
      return (index.notes[b.path]?.createdAt ?? 0) - (index.notes[a.path]?.createdAt ?? 0);
    }
    return (b.mtime ?? 0) - (a.mtime ?? 0);
  });

  for (const node of nodes) if (node.children) sortNodes(node.children, sort, index);
}

export class Vault {
  constructor(fs, meta) {
    this.fs = fs;
    this.meta = meta;
    this.entries = [];
    this.tree = [];
    this.index = { version: 1, notes: {} };
    this.trash = [];
    this.settings = { ...DEFAULT_SETTINGS };
    this.workspace = { ...DEFAULT_WORKSPACE };
    this.cache = new Map();
    this.saveTimer = 0;
    this._sort = 'updated';
  }

  /* ---------------- 打开 / 新建 ---------------- */

  static async openOrCreate() {
    // 桌面端：优先打开上次选的文件夹
    if (isTauri) {
      const saved = localStorage.getItem('inkdown.fsVault.v1');
      if (saved) {
        try {
          return await Vault.openFolder(saved);
        } catch (err) {
          console.warn('[vault] 打开上次的文件夹失败', err);
        }
      }
    }

    // 示例仓库：浏览器里用它，桌面端没选文件夹时也先给它
    const registry = listVaults();
    let id = activeVaultId();

    if (!id || !registry.some((v) => v.id === id)) {
      if (registry.length) id = registry[0].id;
      else return Vault.create('我的仓库');
    }

    return Vault.open(id);
  }

  /** 打开一个真实文件夹作为仓库（仅桌面端） */
  static async openFolder(root) {
    if (!isTauri) throw new Error('当前环境不支持文件夹仓库');

    const resolved = await globalThis.__TAURI__.core.invoke('vault_open', { path: root });
    const fs = new TauriFS(resolved);

    let meta;
    const raw = await fs.read('.inkdown/vault.json');
    if (raw) {
      meta = JSON.parse(raw);
    } else {
      meta = {
        id: `fs_${Date.now().toString(36)}`,
        name: basename(resolved) || '仓库',
        version: 1,
        createdAt: Date.now(),
      };
    }

    const vault = new Vault(fs, meta);
    vault.rootPath = resolved;
    vault.isFolder = true;

    await vault.load();
    localStorage.setItem('inkdown.fsVault.v1', resolved);
    return vault;
  }

  static async open(id) {
    const registry = listVaults();
    let meta = registry.find((v) => v.id === id);

    const fs = new LocalFS(fsKeyFor(id));
    if (!meta) {
      const raw = await fs.read(META_PATH);
      if (!raw) throw new Error(`仓库 ${id} 不存在`);
      meta = JSON.parse(raw);
      registry.push(meta);
      saveVaults(registry);
    }

    setActiveVaultId(id);
    const vault = new Vault(fs, meta);
    await vault.load();
    return vault;
  }

  static async create(name) {
    const id = `v_${Date.now().toString(36)}`;
    const meta = {
      id,
      name: sanitizeName(name) || '未命名仓库',
      version: 1,
      createdAt: Date.now(),
    };

    const fs = new LocalFS(fsKeyFor(id));
    await fs.mkdir('.inkdown');
    await fs.mkdir(PLUGINS_DIR);
    await fs.mkdir(TRASH_DIR);
    await fs.write(META_PATH, JSON.stringify(meta, null, 2));

    const registry = listVaults();
    registry.push(meta);
    saveVaults(registry);
    setActiveVaultId(id);

    const vault = new Vault(fs, meta);
    await vault.load();
    // 只有新建仓库才铺示例内容；已有仓库被清空后不能又长回来
    await vault.seed();
    await vault.refresh();
    return vault;
  }

  async load() {
    await this.ensureStructure();
    this.settings = mergeDeep(DEFAULT_SETTINGS, await this.readJson(SETTINGS_PATH, {}));
    this.workspace = { ...DEFAULT_WORKSPACE, ...(await this.readJson(WORKSPACE_PATH, {})) };
    this.index = { version: 1, notes: {}, ...(await this.readJson(INDEX_PATH, {})) };
    this.trash = await this.readJson(TRASH_PATH, []);
    if (!Array.isArray(this.trash)) this.trash = [];

    // 树的排序跟着 workspace 走，别让刷新时退回默认值
    this._sort = ['updated', 'created', 'title'].includes(this.workspace.sort)
      ? this.workspace.sort
      : 'updated';

    await this.refresh(this._sort);
    return this;
  }

  async ensureStructure() {
    await this.fs.mkdir('.inkdown');
    await this.fs.mkdir(PLUGINS_DIR);
    await this.fs.mkdir(TRASH_DIR);

    if (!(await this.fs.exists(META_PATH))) {
      await this.fs.write(META_PATH, JSON.stringify(this.meta, null, 2));
    }
    if (!(await this.fs.exists(SETTINGS_PATH))) {
      await this.fs.write(SETTINGS_PATH, JSON.stringify(DEFAULT_SETTINGS, null, 2));
    }
    if (!(await this.fs.exists(WORKSPACE_PATH))) {
      await this.fs.write(WORKSPACE_PATH, JSON.stringify(DEFAULT_WORKSPACE, null, 2));
    }
    if (!(await this.fs.exists(INDEX_PATH))) {
      await this.fs.write(INDEX_PATH, JSON.stringify({ version: 1, notes: {} }, null, 2));
    }
    if (!(await this.fs.exists(TRASH_PATH))) {
      await this.fs.write(TRASH_PATH, '[]');
    }

  }

  async seed() {
    for (const dir of SEED_FOLDERS) await this.fs.mkdir(dir);

    const notes = seedNotes();
    const index = { version: 1, notes: {} };

    for (const note of notes) {
      await this.fs.write(note.path, note.content, note.mtime);
      index.notes[note.path] = {
        favorite: Boolean(note.favorite),
        createdAt: note.createdAt,
      };
    }

    this.index = index;
    await this.fs.write(INDEX_PATH, JSON.stringify(index, null, 2));
  }

  /* ---------------- 读写 ---------------- */

  async readJson(path, fallback) {
    try {
      const raw = await this.fs.read(path);
      if (!raw) return fallback;
      return JSON.parse(raw);
    } catch (err) {
      console.warn(`[vault] 解析失败 ${path}`, err);
      return fallback;
    }
  }

  async writeJson(path, value) {
    await this.fs.write(path, JSON.stringify(value, null, 2));
  }

  scheduleSave(kind) {
    if (kind === 'settings') {
      clearTimeout(this.settingsTimer);
      this.settingsTimer = setTimeout(() => {
        this.writeJson(SETTINGS_PATH, this.settings);
      }, 300);
    } else if (kind === 'workspace') {
      clearTimeout(this.workspaceTimer);
      this.workspaceTimer = setTimeout(() => {
        this.writeJson(WORKSPACE_PATH, this.workspace);
      }, 300);
    }
  }

  patchSettings(patch) {
    this.settings = mergeDeep(this.settings, patch);
    this.scheduleSave('settings');
    return this.settings;
  }

  patchWorkspace(patch) {
    this.workspace = mergeDeep(this.workspace, patch);
    this.scheduleSave('workspace');
    return this.workspace;
  }

  /* ---------------- 树 ---------------- */

  async refresh(sort = this._sort) {
    this._sort = sort;
    this.entries = await this.fs.walk();
    this.tree = this.buildTree(sort);
    return this.tree;
  }

  buildTree(sort = this._sort) {
    const root = { path: '', type: 'dir', name: '', children: [] };
    const dirs = new Map([['', root]]);

    const visible = this.entries
      .filter((e) => !isHidden(e.path))
      .sort((a, b) => a.path.localeCompare(b.path, 'zh-CN'));

    for (const entry of visible) {
      if (entry.type !== 'dir') continue;
      const node = { path: entry.path, type: 'dir', name: basename(entry.path), children: [] };
      dirs.set(entry.path, node);
      dirs.get(dirname(entry.path))?.children.push(node);
    }

    for (const entry of visible) {
      if (entry.type !== 'file') continue;
      const isMd = isMarkdown(entry.path);
      const ext = getFileExt(entry.path);
      const ft = getFileType(entry.path);
      dirs.get(dirname(entry.path))?.children.push({
        path: entry.path,
        type: 'file',
        name: isMd ? stripExt(basename(entry.path)) : basename(entry.path),
        mtime: entry.mtime,
        favorite: Boolean(this.index.notes?.[entry.path]?.favorite),
        ext,
        fileType: ft,
        isMarkdown: isMd,
      });
    }

    sortNodes(root.children, sort, this.index);
    return root.children;
  }

  /** 保存后只更新 mtime，不重排 —— 编辑过程中列表不该跳 */
  markSaved(path, mtime) {
    const entry = this.entries.find((e) => e.path === path);
    if (entry) entry.mtime = mtime;

    const walk = (nodes) => {
      for (const node of nodes) {
        if (node.path === path) node.mtime = mtime;
        if (node.children) walk(node.children);
      }
    };
    walk(this.tree);
  }

  flatNotes() {
    return this.entries
      .filter((e) => e.type === 'file' && isMarkdown(e.path) && !isHidden(e.path))
      .map((e) => this.noteAt(e.path))
      .filter(Boolean);
  }

  noteAt(path) {
    if (!path) return null;
    const entry = this.entries.find((e) => e.path === path);
    if (!entry) return null;
    const meta = this.index.notes?.[path] ?? {};
    const ext = getFileExt(path);
    const fileType = getFileType(path);
    const isMd = isMarkdown(path);
    return {
      path,
      title: isMd ? stripExt(basename(path)) : basename(path),
      name: basename(path),
      mtime: entry.mtime,
      updatedAt: entry.mtime,
      favorite: Boolean(meta.favorite),
      createdAt: meta.createdAt ?? entry.mtime,
      ext,
      fileType,
      isMarkdown: isMd,
    };
  }

  favorites() {
    return this.flatNotes()
      .filter((n) => n.favorite)
      .sort((a, b) => b.mtime - a.mtime);
  }

  counts() {
    const all = this.flatNotes().length;
    return {
      all,
      favorites: this.favorites().length,
      trash: this.trash.length,
    };
  }

  async readNote(path) {
    if (this.cache.has(path)) return this.cache.get(path);
    const text = (await this.fs.read(path)) ?? '';
    this.cache.set(path, text);
    return text;
  }

  async readBinary(path) {
    return this.fs.readBinary(path);
  }

  async saveAttachment(filename, base64Data) {
    const attachDir = 'attachments';
    await this.fs.mkdir(attachDir);
    const cleanName = sanitizeName(filename) || `attachment_${Date.now()}`;
    const rel = joinPath(attachDir, cleanName);
    await this.fs.writeBinary(rel, base64Data);
    this.entries = await this.fs.walk();
    return rel;
  }

  async saveNote(path, text) {
    const mtime = Date.now();
    await this.fs.write(path, text, mtime);
    this.cache.set(path, text);
    this.markSaved(path, mtime);
    return mtime;
  }

  /**
   * 全文搜索。
   * 逐篇 await 会让桌面端的 IPC 往返串成一条长队，所以按批并发读；
   * 命中后顺带截一段上下文，列表里就不用只显示路径了。
   */
  async search(query, limit = 60) {
    const q = String(query ?? '').trim().toLowerCase();
    if (!q) return [];

    const notes = this.flatNotes();
    const hits = [];
    const BATCH = 24;

    for (let i = 0; i < notes.length && hits.length < limit; i += BATCH) {
      const batch = notes.slice(i, i + BATCH);
      const contents = await Promise.all(batch.map((note) => this.readNote(note.path)));

      batch.forEach((note, k) => {
        if (hits.length >= limit) return;
        const text = contents[k] || '';
        const titleHit = note.title.toLowerCase().includes(q);
        const at = text.toLowerCase().indexOf(q);
        if (!titleHit && at < 0) return;
        hits.push({ ...note, snippet: at >= 0 ? snippetAround(text, q) : '' });
      });
    }

    return hits.sort((a, b) => b.mtime - a.mtime);
  }

  /* ---------------- 增删改 ---------------- */

  async uniqueName(dir, base, ext = '') {
    const taken = new Set(
      this.entries
        .filter((e) => dirname(e.path) === dir)
        .map((e) => basename(e.path).toLowerCase())
    );

    let name = base;
    let i = 2;
    while (taken.has(`${name}${ext}`.toLowerCase())) {
      name = `${base} ${i}`;
      i++;
    }
    return name;
  }

  async createNote(dir = '', base = '未命名笔记') {
    const name = await this.uniqueName(dir, sanitizeName(base) || '未命名笔记', '.md');
    const path = joinPath(dir, `${name}.md`);

    await this.fs.write(path, '');
    this.index.notes = this.index.notes || {};
    this.index.notes[path] = { favorite: false, createdAt: Date.now() };
    await this.writeJson(INDEX_PATH, this.index);

    await this.refresh();
    return path;
  }

  async renameNote(path, nextBase) {
    const name = sanitizeName(nextBase);
    if (!name) return { ok: false, error: '名称不能为空' };

    const dir = dirname(path);
    if (name === stripExt(basename(path))) return { ok: true, path };

    const unique = await this.uniqueName(dir, name, '.md');
    const next = joinPath(dir, `${unique}.md`);
    if (next === path) return { ok: true, path };

    await this.fs.rename(path, next);

    if (this.index.notes?.[path]) {
      this.index.notes[next] = this.index.notes[path];
      delete this.index.notes[path];
      await this.writeJson(INDEX_PATH, this.index);
    }
    if (this.cache.has(path)) {
      this.cache.set(next, this.cache.get(path));
      this.cache.delete(path);
    }

    await this.refresh();
    return { ok: true, path: next, renamed: unique !== name };
  }

  async moveItem(srcPath, targetDir = '') {
    if (!srcPath) return { ok: false, error: '源路径为空' };
    const base = basename(srcPath);
    let nextPath = targetDir ? joinPath(targetDir, base) : base;
    if (srcPath === nextPath) return { ok: true, path: srcPath };

    // 防止将目录移入它自身的子目录下
    if (targetDir === srcPath || targetDir.startsWith(`${srcPath}/`)) {
      return { ok: false, error: '无法将文件夹移入其自身或子文件夹中' };
    }

    // 冲突重命名检测
    const targetTaken = new Set(
      this.entries
        .filter((e) => dirname(e.path) === targetDir)
        .map((e) => basename(e.path).toLowerCase())
    );

    let unique = base;
    let i = 2;
    if (targetTaken.has(unique.toLowerCase())) {
      const dot = base.lastIndexOf('.');
      const ext = dot > 0 ? base.slice(dot) : '';
      const stem = stripExt(base);
      while (targetTaken.has(unique.toLowerCase())) {
        unique = ext ? `${stem} ${i}${ext}` : `${stem} ${i}`;
        i++;
      }
      nextPath = targetDir ? joinPath(targetDir, unique) : unique;
    }

    await this.fs.rename(srcPath, nextPath);

    // 迁移索引和缓存
    const isDir = this.entries.some((e) => e.path === srcPath && e.type === 'dir');
    if (isDir) {
      const migrated = {};
      for (const [key, value] of Object.entries(this.index.notes || {})) {
        if (key === srcPath || key.startsWith(`${srcPath}/`)) {
          migrated[`${nextPath}${key.slice(srcPath.length)}`] = value;
          delete this.index.notes[key];
        }
      }
      Object.assign(this.index.notes, migrated);

      for (const [key, text] of [...this.cache.entries()]) {
        if (key === srcPath || key.startsWith(`${srcPath}/`)) {
          this.cache.set(`${nextPath}${key.slice(srcPath.length)}`, text);
          this.cache.delete(key);
        }
      }

      if (Array.isArray(this.workspace.recents)) {
        this.workspace.recents = this.workspace.recents.map((p) =>
          p === srcPath || p.startsWith(`${srcPath}/`) ? `${nextPath}${p.slice(srcPath.length)}` : p
        );
        this.scheduleSave('workspace');
      }
    } else {
      if (this.index.notes?.[srcPath]) {
        this.index.notes[nextPath] = this.index.notes[srcPath];
        delete this.index.notes[srcPath];
      }
      if (this.cache.has(srcPath)) {
        this.cache.set(nextPath, this.cache.get(srcPath));
        this.cache.delete(srcPath);
      }
    }

    await this.writeJson(INDEX_PATH, this.index);
    await this.refresh();
    return { ok: true, path: nextPath };
  }

  async createFolder(dir = '', base = '新建文件夹') {
    const name = await this.uniqueName(dir, sanitizeName(base) || '新建文件夹');
    const path = joinPath(dir, name);
    await this.fs.mkdir(path);
    await this.refresh();
    return path;
  }

  async renameFolder(path, nextName) {
    const name = sanitizeName(nextName);
    if (!name) return { ok: false, error: '名称不能为空' };
    if (name === basename(path)) return { ok: true, path };

    const parent = dirname(path);
    const taken = new Set(
      this.entries
        .filter((e) => dirname(e.path) === parent)
        .map((e) => basename(e.path).toLowerCase())
    );
    let unique = name;
    let i = 2;
    while (taken.has(unique.toLowerCase())) {
      unique = `${name} ${i}`;
      i++;
    }

    const next = joinPath(parent, unique);
    await this.fs.rename(path, next);

    const migrated = {};
    for (const [key, value] of Object.entries(this.index.notes || {})) {
      if (key === path || key.startsWith(`${path}/`)) {
        migrated[`${next}${key.slice(path.length)}`] = value;
        delete this.index.notes[key];
      }
    }
    Object.assign(this.index.notes, migrated);
    for (const [key, text] of [...this.cache.entries()]) {
      if (key === path || key.startsWith(`${path}/`)) {
        this.cache.set(`${next}${key.slice(path.length)}`, text);
        this.cache.delete(key);
      }
    }
    if (Array.isArray(this.workspace.recents)) {
      this.workspace.recents = this.workspace.recents.map((p) =>
        p === path || p.startsWith(`${path}/`) ? `${next}${p.slice(path.length)}` : p
      );
      this.scheduleSave('workspace');
    }
    await this.writeJson(INDEX_PATH, this.index);

    await this.refresh();
    return { ok: true, path: next, renamed: unique !== name };
  }

  async deleteFolder(path) {
    await this.fs.remove(path);
    for (const key of Object.keys(this.index.notes || {})) {
      if (key === path || key.startsWith(`${path}/`)) delete this.index.notes[key];
    }
    for (const key of [...this.cache.keys()]) {
      if (key === path || key.startsWith(`${path}/`)) this.cache.delete(key);
    }
    if (Array.isArray(this.workspace.recents)) {
      this.workspace.recents = this.workspace.recents.filter(
        (p) => p !== path && !p.startsWith(`${path}/`)
      );
      this.scheduleSave('workspace');
    }
    await this.writeJson(INDEX_PATH, this.index);
    await this.refresh();
  }

  async toggleFavorite(path) {
    this.index.notes = this.index.notes || {};
    const meta = this.index.notes[path] || { createdAt: Date.now() };
    meta.favorite = !meta.favorite;
    this.index.notes[path] = meta;
    await this.writeJson(INDEX_PATH, this.index);

    const walk = (nodes) => {
      for (const node of nodes) {
        if (node.path === path) node.favorite = meta.favorite;
        if (node.children) walk(node.children);
      }
    };
    walk(this.tree);
    return meta.favorite;
  }

  async trashNote(path) {
    const name = basename(path);
    const trashPath = joinPath(TRASH_DIR, `${Date.now()}-${name}`);
    await this.fs.rename(path, trashPath);

    this.trash.push({
      path: trashPath,
      originalPath: path,
      name: stripExt(name),
      deletedAt: Date.now(),
    });
    await this.writeJson(TRASH_PATH, this.trash);

    if (this.cache.has(path)) {
      this.cache.set(trashPath, this.cache.get(path));
      this.cache.delete(path);
    }

    await this.refresh();
    return trashPath;
  }

  async restore(trashPath) {
    const entry = this.trash.find((t) => t.path === trashPath);
    if (!entry) return { ok: false, error: '回收站里找不到这篇笔记' };

    const dir = dirname(entry.originalPath);
    if (dir) await this.fs.mkdir(dir);

    let target = entry.originalPath;
    if (await this.fs.exists(target)) {
      const ext = getFileExt(target) ? `.${getFileExt(target)}` : '.md';
      const name = await this.uniqueName(dir, stripExt(basename(target)), ext);
      target = joinPath(dir, `${name}${ext}`);
    }

    await this.fs.rename(trashPath, target);
    this.trash = this.trash.filter((t) => t.path !== trashPath);
    await this.writeJson(TRASH_PATH, this.trash);

    if (this.cache.has(trashPath)) {
      this.cache.set(target, this.cache.get(trashPath));
      this.cache.delete(trashPath);
    }

    await this.refresh();
    return { ok: true, path: target };
  }

  async deleteForever(trashPath) {
    const entry = this.trash.find((t) => t.path === trashPath);
    await this.fs.remove(trashPath);

    if (entry && this.index.notes?.[entry.originalPath]) {
      delete this.index.notes[entry.originalPath];
      await this.writeJson(INDEX_PATH, this.index);
    }

    this.trash = this.trash.filter((t) => t.path !== trashPath);
    await this.writeJson(TRASH_PATH, this.trash);
    this.cache.delete(trashPath);
    await this.refresh();
  }
}

export { LocalFS };
