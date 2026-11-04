/* ==========================================================================
   Inkdown · Vault FS
   仓库的存储适配层。上层只认这几个异步方法，换后端不用动仓库逻辑。
   ========================================================================== */

/* ---------------- 路径工具 ---------------- */

export const norm = (p) =>
  String(p ?? '')
    .replace(/\\/g, '/')
    .replace(/\/{2,}/g, '/')
    .replace(/^\/+|\/+$/g, '');

export const dirname = (p) => {
  const s = norm(p);
  const i = s.lastIndexOf('/');
  return i < 0 ? '' : s.slice(0, i);
};

export const basename = (p) => {
  const s = norm(p);
  return s.slice(s.lastIndexOf('/') + 1);
};

export const joinPath = (...parts) => norm(parts.filter((p) => p !== '').join('/'));

export const stripExt = (name) => name.replace(/\.[^.]+$/, '');

export const isMarkdown = (p) => /\.(md|markdown|mdown)$/i.test(p);

export const isHidden = (p) => norm(p).split('/').some((seg) => seg.startsWith('.'));

export const getFileExt = (p) => {
  const name = basename(p);
  const idx = name.lastIndexOf('.');
  return idx > 0 ? name.slice(idx + 1).toLowerCase() : '';
};

export const getFileType = (p) => {
  const ext = getFileExt(p);
  if (['md', 'markdown', 'mdown'].includes(ext)) return 'markdown';
  if (['pdf'].includes(ext)) return 'pdf';
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'].includes(ext)) return 'image';
  if (['mp4', 'webm', 'mov', 'mkv', 'avi'].includes(ext)) return 'video';
  if (['mp3', 'wav', 'm4a', 'ogg', 'flac', 'aac'].includes(ext)) return 'audio';
  if (['docx', 'doc'].includes(ext)) return 'word';
  if (['xlsx', 'xls', 'csv'].includes(ext)) return 'excel';
  if (['pptx', 'ppt'].includes(ext)) return 'ppt';
  if ([
    'js', 'ts', 'jsx', 'tsx', 'mjs', 'cjs', 'rs', 'py', 'json', 'css', 'scss', 'html',
    'htm', 'toml', 'yaml', 'yml', 'sql', 'sh', 'bash', 'c', 'cpp', 'h', 'hpp', 'go',
    'java', 'kt', 'swift', 'php', 'rb', 'lua', 'xml', 'ini', 'env'
  ].includes(ext)) return 'code';
  return 'other';
};

/* ---------------- 虚拟文件系统 ---------------- */

const now = () => Date.now();

export class LocalFS {
  constructor(key) {
    this.key = key;
    this.entries = new Map();
    this.load();
  }

  load() {
    try {
      const raw = localStorage.getItem(this.key);
      if (!raw) return;
      const data = JSON.parse(raw);
      for (const [path, entry] of Object.entries(data)) {
        this.entries.set(norm(path), entry);
      }
    } catch (err) {
      console.warn('[vault] 读取失败', err);
    }
  }

  persist() {
    try {
      localStorage.setItem(this.key, JSON.stringify(Object.fromEntries(this.entries)));
    } catch (err) {
      console.warn('[vault] 写入失败', err);
    }
  }

  ensureParents(path) {
    const parts = norm(path).split('/');
    parts.pop();
    let cur = '';
    for (const part of parts) {
      cur = cur ? `${cur}/${part}` : part;
      if (!this.entries.has(cur)) {
        this.entries.set(cur, { type: 'dir', mtime: now() });
      }
    }
  }

  async exists(path) {
    return this.entries.has(norm(path));
  }

  async stat(path) {
    const e = this.entries.get(norm(path));
    if (!e) return null;
    return { type: e.type, mtime: e.mtime, size: e.text?.length ?? e.data?.length ?? 0 };
  }

  async read(path) {
    const e = this.entries.get(norm(path));
    return e && e.type === 'file' ? e.text ?? '' : null;
  }

  async readBinary(path) {
    const e = this.entries.get(norm(path));
    return e && e.type === 'file' ? (e.data ?? (e.text ? btoa(unescape(encodeURIComponent(e.text))) : '')) : null;
  }

  async write(path, text, mtime = now()) {
    const p = norm(path);
    this.ensureParents(p);
    this.entries.set(p, { type: 'file', text: String(text ?? ''), mtime });
    this.persist();
  }

  async writeBinary(path, base64Data, mtime = now()) {
    const p = norm(path);
    this.ensureParents(p);
    this.entries.set(p, { type: 'file', data: base64Data, mtime });
    this.persist();
  }

  async mkdir(path) {
    const p = norm(path);
    if (!p) return;
    this.ensureParents(p);
    if (!this.entries.has(p)) this.entries.set(p, { type: 'dir', mtime: now() });
    this.persist();
  }

  async list(path) {
    const dir = norm(path);
    const out = [];
    for (const [p, e] of this.entries) {
      if (dirname(p) !== dir) continue;
      out.push({ path: p, name: basename(p), type: e.type, mtime: e.mtime });
    }
    return out;
  }

  async walk() {
    return [...this.entries].map(([path, e]) => ({
      path,
      type: e.type,
      mtime: e.mtime,
    }));
  }

  async rename(from, to) {
    const a = norm(from);
    const b = norm(to);
    if (a === b) return;
    const moving = [...this.entries].filter(([p]) => p === a || p.startsWith(`${a}/`));
    if (!moving.length) return;

    this.ensureParents(b);
    for (const [p, e] of moving) {
      this.entries.delete(p);
      const next = p === a ? b : `${b}${p.slice(a.length)}`;
      this.entries.set(next, e);
    }
    this.persist();
  }

  async remove(path) {
    const p = norm(path);
    for (const key of [...this.entries.keys()]) {
      if (key === p || key.startsWith(`${p}/`)) this.entries.delete(key);
    }
    this.persist();
  }

  async absPath(rel) {
    return norm(rel);
  }

  async openPath(_rel) {
    // web fallback
  }
}

/* ---------------- 桌面端文件系统 ---------------- */

const tauri = globalThis.__TAURI__;

export const isTauri = Boolean(tauri?.core?.invoke);

export class TauriFS {
  constructor(root) {
    this.root = root;
  }

  invoke(cmd, args) {
    return tauri.core.invoke(cmd, args);
  }

  async exists(rel) {
    return this.invoke('vault_exists', { rel });
  }

  async read(rel) {
    return this.invoke('vault_read', { rel });
  }

  async readBinary(rel) {
    return this.invoke('vault_read_binary', { rel });
  }

  async write(rel, text) {
    return this.invoke('vault_write', { rel, text });
  }

  async writeBinary(rel, data) {
    return this.invoke('vault_write_binary', { rel, data });
  }

  async absPath(rel) {
    return this.invoke('vault_abs_path', { rel });
  }

  async openPath(rel) {
    return this.invoke('vault_open_path', { rel });
  }

  async systemFonts() {
    return this.invoke('system_fonts');
  }

  async mkdir(rel) {
    return this.invoke('vault_mkdir', { rel });
  }

  async remove(rel) {
    return this.invoke('vault_remove', { rel });
  }

  async rename(from, to) {
    return this.invoke('vault_rename', { from, to });
  }

  async walk() {
    return this.invoke('vault_walk');
  }

  /**
   * 列一层目录。
   *
   * 必须和 LocalFS.list() 返回同一个形状 —— walk() 的条目只有 path/type/mtime，
   * 少了 name。插件宿主是按 name 取目录名的，先前这里漏了它，
   * 桌面端就把目录名读成 undefined，转而去读 plugins/undefined/manifest.json，
   * 报「缺少 manifest.json」；浏览器走 LocalFS 所以一直正常。
   */
  async list(rel) {
    const all = await this.walk();
    const dir = norm(rel);
    return all
      .filter((entry) => dirname(entry.path) === dir)
      .map((entry) => ({ ...entry, name: basename(entry.path) }));
  }
}

/* ---------------- 仓库登记表 ---------------- */

const REGISTRY_KEY = 'inkdown.vaults.v1';
const ACTIVE_KEY = 'inkdown.activeVault.v1';

export function listVaults() {
  try {
    const raw = localStorage.getItem(REGISTRY_KEY);
    const list = raw ? JSON.parse(raw) : [];
    return Array.isArray(list) ? list : [];
  } catch {
    return [];
  }
}

export function saveVaults(list) {
  localStorage.setItem(REGISTRY_KEY, JSON.stringify(list));
}

export function activeVaultId() {
  return localStorage.getItem(ACTIVE_KEY) || '';
}

export function setActiveVaultId(id) {
  localStorage.setItem(ACTIVE_KEY, id);
}

export const fsKeyFor = (id) => `inkdown.vfs.${id}`;
