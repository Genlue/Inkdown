/* ==========================================================================
   Inkdown · App
   仓库制：一个仓库 = 一个文件夹，配置在 .inkdown/，笔记是文件夹树里的 .md。

   结构：窗口顶栏 / 导航 / 文件树 / 编辑器 + 预览 / 可插拔右侧栏
   ========================================================================== */

import { h, fill, debounce } from './core/dom.js';
import { icon, iconEl } from './core/icons.js';
import { theme } from './core/theme.js';
import { mountGlassSurfaces } from './core/glass-lens.js';
import { revealOnScroll } from './core/scrollbar.js';
import { isDesktop, setSidebarBackdrop, setWindowTheme } from './core/platform.js';

/** Tauri v2 的窗口 API；比直接 invoke 插件命令稳，不用自己传窗口 label */
const tauriWindow = () => globalThis.__TAURI__?.window?.getCurrentWindow?.();
import {
  TrafficLights,
  Capsule,
  IconButton,
  Segmented,
  ToolbarTitle,
  NavColumn,
  NavGroup,
  NavSection,
  NavItem,
  NavInfo,
  List,
  ListRow,
  Empty,
  Button,
  Field,
  Row,
  Swatches,
  Inspector,
  toast,
  openMenu,
  openPopover,
  confirmDialog,
  markNavMoving,
  Badge,
  Drawer,
  Resizer,
  TopVeil,
} from './ui/kit.js';
import { TabManager } from './tabs/tab-manager.js';
import { ViewHost } from './views/view-host.js';
import { resolveMarkdownImages } from './markdown.js';
import { getFileType, getFileExt, isMarkdown, basename, dirname, listVaults, setActiveVaultId } from './vault/fs.js';
import { renderMarkdown } from './markdown.js';
import { Vault } from './vault/vault.js';
import {
  registerSidebarView,
  getSidebarViews,
  getSidebarView,
} from './sidebar/views.js';
import { outlineView } from './sidebar/outline-view.js';
import { infoView } from './sidebar/info-view.js';
import { relationsView } from './sidebar/relations-view.js';
import { buildIndex, findBacklinks, findOutlinks } from './vault/links.js';
import { relTime, charCount, lineCount, lineOffset } from './util.js';

/* ---------------- 启动 ---------------- */

theme.init();

if (isDesktop) {
  document.documentElement.dataset.desktop = '1';

  // 优先打开用户指定或历史仓库；若无历史记录，自动初始化默认仓库 (Documents/Inkdown Vault)
  try {
    const bootPath = await globalThis.__TAURI__.core.invoke('vault_boot_path');
    const saved = localStorage.getItem('inkdown.fsVault.v1');
    if (!saved && bootPath) localStorage.setItem('inkdown.fsVault.v1', bootPath);
  } catch {}
}

const vault = await Vault.openOrCreate();

const EDITOR_SIZES = { sm: '12.5px', md: '13.5px', lg: '15px' };

const PAGE_META = {
  files: { title: '文件', icon: 'folder-tree' },
  favorites: { title: '收藏', icon: 'star' },
  trash: { title: '回收站', icon: 'trash' },
};

const state = {
  page: 'files',
  current: '',
  currentView: 'note',
  query: '',
  searchResults: null,
  sort: ['updated', 'created', 'title'].includes(vault.workspace.sort)
    ? vault.workspace.sort
    : 'updated',
  mode: ['edit', 'split', 'preview'].includes(vault.workspace.mode)
    ? vault.workspace.mode
    : 'edit',
  sidebar: vault.workspace.sidebar !== false,
  navHidden: Boolean(vault.workspace.navHidden),
  view: ['outline', 'info', 'relations'].includes(vault.workspace.view)
    ? vault.workspace.view
    : 'outline',
  expanded: new Set(vault.workspace.expanded || []),
  selectedDir: '',
  splitRatio: Number(vault.workspace.split) || 0.5,
  recents: Array.isArray(vault.workspace.recents) ? vault.workspace.recents : [],
  recentsOpen: Boolean(vault.workspace.recentsOpen),
  listHidden: Boolean(vault.workspace.listHidden),
  treeCollapsed: Boolean(vault.workspace.treeCollapsed),
  navHistory: [],
  navIndex: -1,
};

let applyingTheme = false;
let loadToken = 0;
let lastLoaded = '';
let syncLock = 0;
let titleTask = Promise.resolve();

const app = document.getElementById('app');
const topbar = app.querySelector('.c-topbar');
const listPane = document.getElementById('listPane');
const splitRoot = listPane.parentElement;
const docPane = document.getElementById('docPane');

/* ---------------- 当前对象 ---------------- */

const currentNote = () => (state.page === 'trash' ? null : vault.noteAt(state.current));

const currentTrash = () =>
  state.page === 'trash' ? vault.trash.find((t) => t.path === state.current) || null : null;

function expandTo(path) {
  let dir = dirname(path);
  while (dir) {
    state.expanded.add(dir);
    dir = dirname(dir);
  }
}

function pickInitialNote() {
  const last = vault.workspace.lastNote;
  if (last && vault.noteAt(last)) return last;

  // 新仓库没有「上次打开」：选最近改过的一篇（示例仓库就是欢迎页）
  const notes = vault.flatNotes().sort((a, b) => b.mtime - a.mtime);
  return notes[0]?.path ?? '';
}

/* ---------------- 主题与外观 ---------------- */

function applyAppearance(appearance = {}) {
  applyingTheme = true;
  theme.setMode(appearance.mode || 'system');
  if (appearance.accent) theme.setAccent(appearance.accent);
  applyingTheme = false;
}

applyAppearance(vault.settings.appearance);

function applyFonts(fonts = {}) {
  const root = document.documentElement;
  if (fonts.ui) {
    root.style.setProperty('--ink-font-ui', `"${fonts.ui}", system-ui, sans-serif`);
    root.style.setProperty('--font-ui', `"${fonts.ui}", system-ui, sans-serif`);
  } else {
    root.style.removeProperty('--ink-font-ui');
  }

  if (fonts.markdown) {
    root.style.setProperty('--ink-font-markdown', `"${fonts.markdown}", var(--font-ui)`);
  } else {
    root.style.removeProperty('--ink-font-markdown');
  }

  if (fonts.mono) {
    root.style.setProperty('--ink-font-mono', `"${fonts.mono}", monospace`);
    root.style.setProperty('--font-mono', `"${fonts.mono}", monospace`);
  } else {
    root.style.removeProperty('--ink-font-mono');
  }
}

applyFonts(vault.settings.fonts || {});

function applyEditorSize() {
  const size = vault.settings.editor?.size || 'md';
  document.documentElement.style.setProperty(
    '--ink-editor-fs',
    EDITOR_SIZES[size] || EDITOR_SIZES.md
  );
}
applyEditorSize();

/* ---------------- 顶栏 ---------------- */

const navToggle = IconButton({ icon: 'panel-left', label: '收起导航栏' });
function toggleNav(force) {
  const isHidden = app.classList.contains('l-app--nav-hidden');
  const willHide = typeof force === 'boolean' ? force : !isHidden;
  if (isHidden !== willHide) {
    markNavMoving(app);
    app.classList.toggle('l-app--nav-hidden', willHide);
    state.navHidden = willHide;
    const label = willHide ? '展开导航栏' : '收起导航栏';
    navToggle.setAttribute('aria-label', label);
    navToggle.title = label;
    vault.patchWorkspace({ navHidden: willHide });
  }
}
navToggle.addEventListener('click', () => toggleNav());

/* 文件列表栏：和导航栏、右侧栏都独立的一条状态。
   收起 = 整列滑出去 + 正文的 margin-left 当帧归零（被还盖在上面的列表遮住），
   和导航栏同一套「先遮后让」的时序。 */
function setList(open, { persist = true } = {}) {
  const value = Boolean(open);
  state.listHidden = !value;
  splitRoot.dataset.list = value ? '1' : '0';

  const label = value ? '收起文件列表' : '展开文件列表';
  filesBtn.title = label;
  filesBtn.setAttribute('aria-label', label);
  filesBtn.setAttribute('aria-pressed', String(!value));

  if (persist) vault.patchWorkspace({ listHidden: !value });
}

function toggleList(force) {
  setList(typeof force === 'boolean' ? force : Boolean(state.listHidden));
}

const viewSeg = Segmented({
  value: state.mode,
  options: [
    { value: 'edit', icon: 'square-pen', title: '仅编辑' },
    { value: 'split', icon: 'columns-2', title: '分栏' },
    { value: 'preview', icon: 'eye', title: '仅预览' },
  ],
  onChange: (v) => setMode(v),
});

const themeBtn = IconButton({ icon: 'moon', label: '切换到深色' });
function syncThemeBtn() {
  const dark = theme.resolved === 'dark';
  fill(themeBtn, iconEl(dark ? 'sun' : 'moon', 16));
  const label = dark ? '切换到浅色' : '切换到深色';
  themeBtn.title = label;
  themeBtn.setAttribute('aria-label', label);
}
themeBtn.addEventListener('click', () => {
  theme.setMode(theme.resolved === 'dark' ? 'light' : 'dark');
  syncThemeBtn();
});

theme.subscribe(() => {
  syncThemeBtn();
  // 系统材质（DWM Acrylic）按窗口主题染色，必须跟着 CSS 主题一起切
  setWindowTheme(theme.resolved);
  if (applyingTheme) return;
  vault.patchSettings({ appearance: { mode: theme.mode, accent: theme.accent } });
});

const inspectorBtn = IconButton({
  icon: 'panel-right',
  label: '展开右侧栏',
  className: 'ink-panel-btn',
});
inspectorBtn.addEventListener('click', () => toggleSidebar());

const settingsBtn = IconButton({ icon: 'sliders', label: '设置' });
settingsBtn.addEventListener('click', () => {
  openSettingsTab();
});

async function openSettingsTab() {
  await commitDoc();
  state.currentView = 'settings';
  viewHost.switchView({
    id: 'settings',
    label: '偏好设置',
    icon: 'sliders',
    type: 'settings',
  });
  refreshNav();
  updateTopbar();
}

// 导航历史控制
const navBackBtn = IconButton({ icon: 'arrow-left', label: '后退 (Alt+Left)', size: 'sm', onClick: () => navigateBack() });
const navForwardBtn = IconButton({ icon: 'arrow-right', label: '前进 (Alt+Right)', size: 'sm', onClick: () => navigateForward() });
navBackBtn.style.opacity = '0.35';
navBackBtn.style.pointerEvents = 'none';
navForwardBtn.style.opacity = '0.35';
navForwardBtn.style.pointerEvents = 'none';
const navHistoryCapsule = Capsule({ round: true, children: [navBackBtn, navForwardBtn] });

function updateNavHistoryButtons() {
  if (navBackBtn) {
    const canBack = state.navIndex > 0;
    navBackBtn.style.opacity = canBack ? '1' : '0.35';
    navBackBtn.style.pointerEvents = canBack ? '' : 'none';
  }
  if (navForwardBtn) {
    const canForward = state.navIndex < state.navHistory.length - 1;
    navForwardBtn.style.opacity = canForward ? '1' : '0.35';
    navForwardBtn.style.pointerEvents = canForward ? '' : 'none';
  }
}

async function navigateBack() {
  if (state.navIndex > 0) {
    state.navIndex--;
    updateNavHistoryButtons();
    await openNote(state.navHistory[state.navIndex], true, true);
  }
}

async function navigateForward() {
  if (state.navIndex < state.navHistory.length - 1) {
    state.navIndex++;
    updateNavHistoryButtons();
    await openNote(state.navHistory[state.navIndex], true, true);
  }
}

// 快速切换笔记 (Quick Switcher)
const quickSwitcherBtn = IconButton({ icon: 'search', label: '快速打开 (Ctrl+P)', size: 'sm', onClick: () => openQuickSwitcher() });
const quickSwitcherCapsule = Capsule({ round: true, children: quickSwitcherBtn });

function openQuickSwitcher() {
  const existing = document.querySelector('.ink-qs-overlay');
  if (existing) {
    existing.remove();
    return;
  }

  const allNotes = vault.flatNotes();
  let selectedIndex = 0;
  let currentResults = [];

  const input = h('input', {
    class: 'ink-qs-input',
    placeholder: '输入笔记名称跳转或创建 (↑↓ 选择，Enter 打开，Esc 退出)...',
    spellcheck: 'false',
  });

  const list = h('div', { class: 'ink-qs-list' });

  function renderResults(q) {
    const query = q.trim().toLowerCase();
    selectedIndex = 0;
    list.innerHTML = '';

    if (!query) {
      currentResults = allNotes.slice(0, 15).map(n => ({ type: 'note', note: n }));
    } else {
      const matches = allNotes.filter(n =>
        (n.title || n.name).toLowerCase().includes(query) ||
        n.path.toLowerCase().includes(query)
      );
      currentResults = matches.slice(0, 20).map(n => ({ type: 'note', note: n }));

      const exact = matches.some(n => (n.title || n.name).toLowerCase() === query);
      if (!exact) {
        currentResults.push({ type: 'create', name: q.trim() });
      }
    }

    if (currentResults.length === 0) {
      currentResults.push({ type: 'create', name: q.trim() });
    }

    currentResults.forEach((item, idx) => {
      let row;
      if (item.type === 'note') {
        row = h(
          'div',
          {
            class: 'ink-qs-item ' + (idx === selectedIndex ? 'is-selected' : ''),
            onclick: () => choose(item),
          },
          h(
            'div',
            { class: 'ink-qs-item__left' },
            h('span', { class: 'ink-qs-item__icon', html: icon('file-text', 14) }),
            h('span', { class: 'ink-qs-item__title' }, item.note.title || item.note.name)
          ),
          h('span', { class: 'ink-qs-item__path' }, dirname(item.note.path) || '根目录')
        );
      } else {
        row = h(
          'div',
          {
            class: 'ink-qs-item ink-qs-item--create ' + (idx === selectedIndex ? 'is-selected' : ''),
            onclick: () => choose(item),
          },
          h(
            'div',
            { class: 'ink-qs-item__left' },
            h('span', { class: 'ink-qs-item__icon', html: icon('plus', 14) }),
            h('span', { class: 'ink-qs-item__title' }, '创建新笔记「' + item.name + '」')
          )
        );
      }
      list.append(row);
    });
  }

  function choose(item) {
    overlay.remove();
    if (!item) return;
    if (item.type === 'note') {
      openNote(item.note.path);
    } else if (item.type === 'create') {
      vault.createNote(state.selectedDir || 'Notes', item.name).then(newPath => {
        renderAll().then(() => openNote(newPath));
        toast('已创建「' + item.name + '」');
      });
    }
  }

  function updateActive() {
    const rows = list.querySelectorAll('.ink-qs-item');
    rows.forEach((r, i) => {
      r.classList.toggle('is-selected', i === selectedIndex);
      if (i === selectedIndex) {
        r.scrollIntoView({ block: 'nearest' });
      }
    });
  }

  input.addEventListener('input', () => {
    renderResults(input.value);
  });

  input.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (currentResults.length > 0) {
        selectedIndex = (selectedIndex + 1) % currentResults.length;
        updateActive();
      }
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (currentResults.length > 0) {
        selectedIndex = (selectedIndex - 1 + currentResults.length) % currentResults.length;
        updateActive();
      }
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      if (currentResults[selectedIndex]) {
        choose(currentResults[selectedIndex]);
      }
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      overlay.remove();
      return;
    }
  });

  const dialog = h(
    'div',
    { class: 'ink-qs-dialog', onclick: (e) => e.stopPropagation() },
    h(
      'div',
      { class: 'ink-qs-head' },
      h('span', { html: icon('search', 16) }),
      input
    ),
    list,
    h(
      'div',
      { class: 'ink-qs-foot' },
      h('span', null, '↑↓ 移动光标 · ↵ 打开或创建 · Esc 退出'),
      h('span', null, allNotes.length + ' 篇笔记')
    )
  );

  const overlay = h('div', { class: 'ink-qs-overlay', onclick: () => overlay.remove() }, dialog);
  document.body.append(overlay);
  renderResults('');
  setTimeout(() => input.focus(), 10);
}

// 方案 3：纯净流式顶栏 —— 中央面包屑路径 + 大面积窗口拖拽区域
/* 位置由 .c-topbar__lead 决定（展开导航栏时整组右移），
   胶囊只负责材质和排版 —— 少一个 lead 类，展开时就会和「收起」胶囊叠在一起。 */
const topbarBreadcrumb = Capsule({ className: 'c-topbar__breadcrumb' });
topbarBreadcrumb.setAttribute('data-no-drag', '');

/* 文件列表的开关。
   它跟着标题组一起移动，不进 .l-chrome —— 它管的是「内容列左边那一栏」，
   不是窗口 chrome（红绿灯 + 导航开关）。 */
const filesBtn = IconButton({
  icon: 'folder-tree',
  label: '收起文件列表',
  onClick: () => toggleList(),
});
const topbarLead = h(
  'div',
  { class: 'c-topbar__lead', 'data-no-drag': '' },
  Capsule({ round: true, children: filesBtn }),
  topbarBreadcrumb
);
const viewCapsule = Capsule({ children: [viewSeg] });

topbar.append(
  h(
    'div',
    { class: 'l-chrome', 'data-no-drag': '' },
    TrafficLights({
      focused: true,
      onClose: () =>
        isDesktop ? tauriWindow()?.close() : toast('关闭窗口（桌面端生效）', { icon: 'x-circle' }),
      onMinimize: () =>
        isDesktop ? tauriWindow()?.minimize() : toast('最小化（桌面端生效）', { icon: 'minus' }),
      onZoom: () =>
        isDesktop
          ? tauriWindow()?.toggleMaximize()
          : toast('最大化（桌面端生效）', { icon: 'maximize' }),
    }),
    Capsule({ round: true, className: 'l-chrome__toggle', children: navToggle })
  ),
  topbarLead,
  h(
    'div',
    { class: 'c-topbar__pills', 'data-no-drag': '' },
    navHistoryCapsule,
    quickSwitcherCapsule,
    viewCapsule,
    Capsule({ round: true, children: themeBtn }),
    Capsule({ round: true, children: inspectorBtn })
  )
);

/* ---------------- 导航 ---------------- */

const navGroup = NavGroup({ children: [] });

const navInfo = NavInfo({
  iconSrc: './assets/inkdown.png',
  title: 'Inkdown',
  subtitle: vault.meta.name,
  onClick: () => openVaultMenu(navInfo),
});

const side = NavColumn({
  children: [navInfo, navGroup],
  foot: h(
    'div',
    { class: 'ink-foot' },
    h('span', { class: 'ink-foot__dot' }),
    h('span', null, '本地文件 · 自动保存')
  ),
});

app.prepend(side);

function refreshNav() {
  const c = vault.counts();
  const isSettingsActive = state.currentView === 'settings';
  fill(
    navGroup,
    NavSection({ label: '知识库' }),
    NavItem({
      icon: 'folder-tree',
      label: '文件',
      count: c.all,
      selected: state.page === 'files' && !isSettingsActive,
      onClick: () => {
        state.currentView = 'note';
        go('files');
        if (state.current) openNote(state.current);
      },
    }),
    NavItem({
      icon: 'star',
      label: '收藏',
      count: c.favorites,
      selected: state.page === 'favorites' && !isSettingsActive,
      onClick: () => {
        state.currentView = 'note';
        go('favorites');
      },
    }),
    NavItem({
      icon: 'trash',
      label: '回收站',
      count: c.trash,
      selected: state.page === 'trash' && !isSettingsActive,
      onClick: () => {
        state.currentView = 'note';
        go('trash');
      },
    }),
    NavSection({ label: '系统' }),
    NavItem({
      icon: 'sliders',
      label: '偏好设置',
      selected: isSettingsActive,
      onClick: () => openSettingsTab(),
    })
  );
}

async function openInExplorer(relPath = '') {
  if (isDesktop && globalThis.__TAURI__?.core) {
    try {
      await globalThis.__TAURI__.core.invoke('open_in_explorer', { path: relPath });
      toast(relPath ? '已在资源管理器中定位' : '已在资源管理器中打开仓库', { icon: 'folder-open' });
    } catch (err) {
      console.warn('[explorer] 打开失败', err);
      toast('打开失败: ' + err, { icon: 'x-circle', tone: 'danger' });
    }
  } else {
    toast('当前为浏览器虚拟存储，可在设置中选择电脑上的真实本地文件夹', { icon: 'info' });
  }
}

async function copyVaultPath(relPath = '') {
  try {
    let p = vault.rootPath || '';
    if (relPath && isDesktop && globalThis.__TAURI__?.core) {
      p = await globalThis.__TAURI__.core.invoke('vault_abs_path', { rel: relPath });
    } else if (relPath && p) {
      p = p.replace(/[\\/]$/, '') + '/' + relPath;
    }
    if (!p) p = '浏览器虚拟存储 (localStorage)';
    await navigator.clipboard.writeText(p);
    toast('已复制路径到剪贴板: ' + p, { icon: 'check' });
  } catch (err) {
    toast('复制失败', { icon: 'x-circle', tone: 'danger' });
  }
}

function openVaultMenu(anchor) {
  const isReal = Boolean(vault.isFolder && vault.rootPath);
  const items = [
    { header: '当前知识库' },
    {
      label: vault.meta.name || '我的仓库',
      icon: 'hard-drive',
      checked: true,
      onClick: () => copyVaultPath(''),
    },
    {
      label: '在文件资源管理器中打开仓库',
      icon: 'folder-open',
      onClick: () => openInExplorer(''),
    },
    {
      label: '复制仓库绝对路径',
      icon: 'copy',
      onClick: () => copyVaultPath(''),
    },
    { sep: true },
    {
      label: '偏好设置…',
      icon: 'sliders',
      onClick: () => openSettingsTab(),
    },
    { sep: true },
    {
      label: '打开本地文件夹…',
      icon: 'folder-symlink',
      onClick: () => pickFolder(),
    },
  ];

  if (!isDesktop) {
    const vaults = listVaults();
    if (vaults.length > 1) {
      items.push({ sep: true }, { header: '切换虚拟仓库' });
      for (const v of vaults) {
        if (v.id !== vault.meta.id) {
          items.push({
            label: v.name,
            icon: 'database',
            onClick: () => switchVault(v.id),
          });
        }
      }
    }
    items.push({ label: '新建虚拟仓库', icon: 'plus', onClick: () => createVault() });
  }

  openMenu({ anchor, items });
}

async function pickFolder() {
  await commitDoc();
  try {
    const path = await globalThis.__TAURI__.core.invoke('vault_pick');
    if (!path) return;
    localStorage.setItem('inkdown.fsVault.v1', path);
    location.reload();
  } catch (err) {
    console.warn('[vault] 选择文件夹失败', err);
    toast('打开文件夹失败', { icon: 'x-circle', tone: 'danger' });
  }
}

async function switchVault(id) {
  if (id === vault.meta.id) return;
  await commitDoc();
  setActiveVaultId(id);
  location.reload();
}

async function createVault() {
  await commitDoc();
  const next = await Vault.create(`仓库 ${listVaults().length + 1}`);
  setActiveVaultId(next.meta.id);
  location.reload();
}

/* ---------------- 文件列表 ---------------- */

const search = Field({
  placeholder: '搜索笔记',
  icon: 'search',
  width: '100%',
  onInput: debounce((v) => runSearch(v), 180),
});

search.input.addEventListener('keydown', (e) => {
  if (e.key !== 'Escape') return;
  e.preventDefault();
  search.input.value = '';
  runSearch('');
  search.input.blur();
});

const newNoteBtn = IconButton({
  icon: 'plus',
  label: '新建笔记',
  variant: 'tonal',
  onClick: () => newNote(),
});

const newFolderBtn = IconButton({
  icon: 'folder-plus',
  label: '新建文件夹',
  onClick: () => newFolder(),
});

const sortBtn = IconButton({ icon: 'sort', label: '排序', size: 'sm' });
sortBtn.addEventListener('click', () => {
  openMenu({
    anchor: sortBtn,
    items: [
      { header: '排序方式' },
      { label: '名称', icon: 'list', checked: state.sort === 'title', onClick: () => setSort('title') },
      { label: '最近编辑', icon: 'clock', checked: state.sort === 'updated', onClick: () => setSort('updated') },
      { label: '最近创建', icon: 'calendar-range', checked: state.sort === 'created', onClick: () => setSort('created') },
    ],
  });
});

const listLabel = h('span', { class: 'ink-list__label' }, '文件');
const listCount = h('span', { class: 'ink-list__count' }, '0');

const treeChevron = h('span', { class: 'ink-list__tree-chevron' + (state.treeCollapsed ? ' is-collapsed' : '') }, iconEl('chevron-down', 11));
const listLabelWrap = h(
  'div',
  {
    class: 'ink-list__label-wrap',
    title: '点击折叠/展开文件树',
    onclick: () => toggleTreeSection(),
  },
  treeChevron,
  listLabel,
  listCount
);

function toggleTreeSection() {
  state.treeCollapsed = !state.treeCollapsed;
  listScroll.classList.toggle('is-collapsed', state.treeCollapsed);
  treeChevron.classList.toggle('is-collapsed', state.treeCollapsed);
  vault.patchWorkspace({ treeCollapsed: state.treeCollapsed });
}

const foldAllBtn = IconButton({
  icon: 'chevrons-up-down',
  label: '全部折叠 / 全部展开文件夹',
  size: 'sm',
  onClick: () => toggleFoldAll(),
});

function toggleFoldAll() {
  if (state.expanded.size > 0) {
    state.expanded.clear();
  } else {
    for (const entry of (vault.entries || [])) {
      if (entry.type === 'dir') state.expanded.add(entry.path);
    }
  }
  vault.patchWorkspace({ expanded: Array.from(state.expanded) });
  renderFileTree();
}

const revealVaultBtn = IconButton({
  icon: 'folder-open',
  label: '在文件资源管理器中打开仓库',
  size: 'sm',
  onClick: () => openInExplorer(''),
});

listPane.append(
  h(
    'div',
    { class: 'ink-list__head' },
    h('div', { class: 'ink-list__search' }, h('div', { class: 'l-fill' }, search), newNoteBtn, newFolderBtn),
    h('div', { class: 'ink-list__bar' }, listLabelWrap, h('span', { class: 'l-push' }), foldAllBtn, sortBtn, revealVaultBtn)
  )
);

const listScroll = h('div', { class: 'ink-list__scroll' + (state.treeCollapsed ? ' is-collapsed' : '') });
listPane.append(listScroll);

/* ---------------- 最近打开文件卡片 ---------------- */

const recentCount = h('span', { class: 'ink-recent__count' }, '0');

// 抽屉里的表头：不折叠，抽屉本身就是开合动作
const recentHeader = h(
  'div',
  { class: 'ink-recent__head' },
  h('span', { class: 'ink-recent__icon' }, iconEl('clock', 13)),
  h('span', { class: 'ink-recent__title' }, '最近打开'),
  recentCount
);

const recentList = h('div', { class: 'ink-recent__list' });

/**
 * 最近打开 = 从列表底部拉出的抽屉。
 * 收起时只留 30px（一条灰条），不占列表的空间；拉出来才盖住文件树。
 */
const recentDrawer = Drawer({
  side: 'bottom',
  peek: 54,
  size: '62%',
  label: '最近打开',
  className: 'ink-recent-drawer',
  open: Boolean(state.recentsOpen),
  children: [recentHeader, recentList],
  onOpen: () => {
    state.recentsOpen = true;
    vault.patchWorkspace({ recentsOpen: true });
  },
  onClose: () => {
    state.recentsOpen = false;
    vault.patchWorkspace({ recentsOpen: false });
  },
});

listPane.append(recentDrawer);

function addRecent(path) {
  if (!path || path.startsWith('.') || state.page === 'trash') return;
  state.recents = [path, ...state.recents.filter((p) => p !== path)].slice(0, 8);
  vault.patchWorkspace({ recents: state.recents });
  renderRecents();
}

function renderRecents() {
  const validRecents = state.recents.filter((p) => vault.noteAt(p));
  recentCount.textContent = String(validRecents.length);

  if (!validRecents.length) {
    fill(
      recentList,
      h('div', { class: 'ink-recent__empty' }, '暂无最近打开的笔记')
    );
    return;
  }

  const items = validRecents.map((p) => {
    const n = vault.noteAt(p);
    const title = n?.title || basename(p);
    const timeVal = n?.mtime || n?.updatedAt;
    const updated = timeVal ? relTime(timeVal) : '';
    const isActive = state.current === p && state.currentView !== 'settings';
    return h(
      'div',
      {
        class: 'ink-recent__item' + (isActive ? ' is-active' : ''),
        onclick: () => openNote(p),
        oncontextmenu: (e) => {
          e.preventDefault();
          openMenu({
            x: e.clientX,
            y: e.clientY,
            items: [
              { header: title },
              { label: '打开', icon: 'file-text', onClick: () => openNote(p) },
              {
                label: '从最近打开中移除',
                icon: 'x',
                onClick: () => {
                  state.recents = state.recents.filter((item) => item !== p);
                  vault.patchWorkspace({ recents: state.recents });
                  renderRecents();
                },
              },
              { label: '在文件资源管理器中显示', icon: 'folder-open', onClick: () => openInExplorer(p) },
            ],
          });
        },
        title: p,
      },
      h('span', { class: 'ink-recent__item-icon' }, iconEl('file-text', 13)),
      h('span', { class: 'ink-recent__item-title' }, title),
      updated ? h('span', { class: 'ink-recent__item-time' }, updated) : null
    );
  });

  fill(recentList, ...items);
}

listScroll.addEventListener('dragover', (e) => {
  if (treeDraggingPath && !e.target.closest('.ink-tree__row--folder')) {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
  }
});

listScroll.addEventListener('drop', async (e) => {
  if (treeDraggingPath && !e.target.closest('.ink-tree__row--folder')) {
    e.preventDefault();
    const movedPath = treeDraggingPath;
    treeDraggingPath = null;
    try {
      const res = await vault.moveItem(movedPath, '');
      if (!res.ok) {
        toast(res.error || '移动失败', { icon: 'x-circle', tone: 'danger' });
        return;
      }
      if (state.current === movedPath) state.current = res.path;
      await renderAll();
      toast('已移动至仓库根目录', { icon: 'check' });
    } catch (err) {
      toast('移动失败: ' + err, { icon: 'x-circle', tone: 'danger' });
    }
  }
});


/* ---------------- 列表渲染 ---------------- */

let searchToken = 0;

async function runSearch(value) {
  state.query = value;
  const q = value.trim();

  if (!q) {
    state.searchResults = null;
    renderList();
    updateTopbar();
    return;
  }

  const token = ++searchToken;
  const hits = await vault.search(q);
  if (token !== searchToken) return;
  state.searchResults = hits;
  renderList();
  updateTopbar();
}

function pageCount() {
  if (state.page === 'favorites') return vault.counts().favorites;
  if (state.page === 'trash') return vault.counts().trash;
  return vault.counts().all;
}

function updateListHeader() {
  const searching = Boolean(state.query.trim());
  listLabel.textContent = searching ? '搜索结果' : PAGE_META[state.page].title;
  listCount.textContent = String(
    searching ? state.searchResults?.length ?? 0 : pageCount()
  );

  const canCreate = state.page === 'files' && !searching;
  newNoteBtn.style.display = canCreate ? '' : 'none';
  newFolderBtn.style.display = canCreate ? '' : 'none';
  sortBtn.style.display = searching || state.page === 'trash' ? 'none' : '';
}

function renderList() {
  updateListHeader();

  if (state.page === 'trash') {
    renderTrashList();
    return;
  }

  if (state.query.trim()) {
    renderSearchList();
    return;
  }

  if (state.page === 'favorites') {
    renderFavoritesList();
    return;
  }

  renderFileTree();
}

let treeDraggingPath = null;
let treeDraggingType = null;

function renderFileTree() {
  if (!vault.tree.length) {
    fill(
      listScroll,
      h(
        'div',
        { class: 'ink-list__empty' },
        Empty({
          icon: 'folder-tree',
          title: '仓库还是空的',
          desc: '新建一篇笔记，或者建一个文件夹。',
          action: Button({
            label: '新建笔记',
            icon: 'plus',
            variant: 'primary',
            onClick: () => newNote(),
          }),
        })
      )
    );
    return;
  }

  const indentMode = vault.settings.appearance?.treeIndent || 'lines';
  const treeRoot = h('div', {
    class: 'ink-tree',
    role: 'tree',
    'data-indent': indentMode,
  });

  const renderBranch = (nodes, depth) => {
    const frag = document.createDocumentFragment();
    for (const node of nodes) {
      if (node.type === 'dir') {
        const open = state.expanded.has(node.path);
        const fRow = folderRow(node, depth, open);
        if (!open) {
          frag.append(fRow);
        } else {
          const branch = h('div', {
            class: 'ink-tree__branch',
            'data-depth': String(depth),
            style: { '--tree-depth': depth },
          });
          if (node.children.length) {
            branch.append(renderBranch(node.children, depth + 1));
          } else {
            branch.append(
              h(
                'div',
                {
                  class: 'ink-tree__empty',
                  style: { '--tree-depth': depth + 1 },
                },
                '空文件夹'
              )
            );
          }
          frag.append(
            h(
              'div',
              { class: 'ink-tree__group', 'data-path': node.path, style: { '--tree-depth': depth } },
              fRow,
              branch
            )
          );
        }
      } else {
        frag.append(noteRow(node, depth));
      }
    }
    return frag;
  };

  treeRoot.append(renderBranch(vault.tree, 0));
  fill(listScroll, treeRoot);
}

function countNotes(node) {
  if (node.type === 'file') return 1;
  return (node.children || []).reduce((sum, child) => sum + countNotes(child), 0);
}

function folderRow(node, depth, open) {
  const row = h(
    'div',
    {
      class: 'ink-tree__row ink-tree__row--folder',
      'data-type': 'dir',
      'data-path': node.path,
      'data-open': open ? '1' : '0',
      role: 'treeitem',
      tabindex: '0',
      draggable: 'true',
      'aria-expanded': String(open),
      title: node.name,
      style: { '--tree-depth': depth },
      onclick: () => toggleFolder(node.path),
      onkeydown: (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          toggleFolder(node.path);
        }
      },
      oncontextmenu: (e) => {
        e.preventDefault();
        folderMenu(node, e.clientX, e.clientY);
      },
      ondragstart: (e) => {
        treeDraggingPath = node.path;
        treeDraggingType = 'dir';
        e.dataTransfer.setData('text/plain', node.path);
        e.dataTransfer.effectAllowed = 'move';
        row.classList.add('is-tree-dragging');
      },
      ondragend: () => {
        treeDraggingPath = null;
        treeDraggingType = null;
        row.classList.remove('is-tree-dragging');
        listScroll.querySelectorAll('.is-tree-dragover').forEach((el) => el.classList.remove('is-tree-dragover'));
      },
      ondragover: (e) => {
        if (!treeDraggingPath || treeDraggingPath === node.path) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = 'move';
        row.classList.add('is-tree-dragover');
      },
      ondragleave: () => {
        row.classList.remove('is-tree-dragover');
      },
      ondrop: async (e) => {
        e.preventDefault();
        e.stopPropagation();
        row.classList.remove('is-tree-dragover');
        if (!treeDraggingPath || treeDraggingPath === node.path) return;
        const movedPath = treeDraggingPath;
        treeDraggingPath = null;
        try {
          const res = await vault.moveItem(movedPath, node.path);
          if (!res.ok) {
            toast(res.error || '移动失败', { icon: 'x-circle', tone: 'danger' });
            return;
          }
          state.expanded.add(node.path);
          if (state.current === movedPath) state.current = res.path;
          await renderAll();
          toast(`已移动至 ${node.name}`, { icon: 'check' });
        } catch (err) {
          toast('移动失败: ' + err, { icon: 'x-circle', tone: 'danger' });
        }
      },
    },
    h('span', {
      class: `ink-tree__chevron${open ? ' is-open' : ''}`,
      html: icon('chevron-right', 12),
    }),
    h('span', { class: 'ink-tree__icon', html: icon(open ? 'folder-open' : 'folder', 14) }),
    h('span', { class: 'ink-tree__name' }, node.name),
    h('span', { class: 'ink-tree__count' }, String(countNotes(node)))
  );
  return row;
}

function noteRow(node, depth) {
  const isMd = isMarkdown(node.path);
  let fileIcon = 'file-text';
  if (!isMd) {
    const ft = node.fileType || getFileType(node.path);
    if (ft === 'image') fileIcon = 'image';
    else if (ft === 'video') fileIcon = 'video';
    else if (ft === 'audio') fileIcon = 'volume-2';
    else if (ft === 'pdf') fileIcon = 'file';
    else if (ft === 'word') fileIcon = 'file-text';
    else if (ft === 'excel') fileIcon = 'table';
    else if (ft === 'ppt') fileIcon = 'presentation';
    else if (ft === 'code') fileIcon = 'code';
    else fileIcon = 'file';
  }

  const extBadge = !isMd
    ? h('span', { class: 'ink-tree__badge' }, (node.ext || getFileExt(node.path) || 'FILE').toUpperCase())
    : null;

  const row = h(
    'div',
    {
      class: 'ink-tree__row ink-tree__row--note',
      'data-type': 'file',
      'data-path': node.path,
      'data-selected': node.path === state.current ? '1' : '0',
      role: 'treeitem',
      tabindex: '0',
      draggable: 'true',
      'aria-selected': node.path === state.current ? 'true' : 'false',
      title: node.name,
      style: { '--tree-depth': depth },
      onclick: () => openNote(node.path),
      onkeydown: (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          openNote(node.path);
        }
      },
      oncontextmenu: (e) => {
        e.preventDefault();
        noteMenu(node, e.clientX, e.clientY);
      },
      ondragstart: (e) => {
        treeDraggingPath = node.path;
        treeDraggingType = 'file';
        e.dataTransfer.setData('text/plain', node.path);
        e.dataTransfer.effectAllowed = 'move';
        row.classList.add('is-tree-dragging');
      },
      ondragend: () => {
        treeDraggingPath = null;
        treeDraggingType = null;
        row.classList.remove('is-tree-dragging');
        listScroll.querySelectorAll('.is-tree-dragover').forEach((el) => el.classList.remove('is-tree-dragover'));
      },
      ondragover: (e) => {
        if (!treeDraggingPath || treeDraggingPath === node.path) return;
        e.preventDefault();
        e.stopPropagation();
        e.dataTransfer.dropEffect = 'move';
        row.classList.add('is-tree-dragover');
      },
      ondragleave: () => {
        row.classList.remove('is-tree-dragover');
      },
      ondrop: async (e) => {
        e.preventDefault();
        e.stopPropagation();
        row.classList.remove('is-tree-dragover');
        if (!treeDraggingPath || treeDraggingPath === node.path) return;
        const movedPath = treeDraggingPath;
        treeDraggingPath = null;
        const targetDir = dirname(node.path);
        try {
          const res = await vault.moveItem(movedPath, targetDir);
          if (!res.ok) {
            toast(res.error || '移动失败', { icon: 'x-circle', tone: 'danger' });
            return;
          }
          if (targetDir) state.expanded.add(targetDir);
          if (state.current === movedPath) state.current = res.path;
          await renderAll();
          toast(`已移动至 ${targetDir || '仓库根目录'}`, { icon: 'check' });
        } catch (err) {
          toast('移动失败: ' + err, { icon: 'x-circle', tone: 'danger' });
        }
      },
    },
    h('span', { class: 'ink-tree__icon', html: icon(fileIcon, 13) }),
    h('span', { class: 'ink-tree__name' }, node.name),
    extBadge,
    node.favorite ? h('span', { class: 'ink-tree__star', html: icon('star', 10) }) : null
  );
  return row;
}

function renderFavoritesList() {
  const notes = vault.favorites();

  if (!notes.length) {
    fill(
      listScroll,
      h(
        'div',
        { class: 'ink-list__empty' },
        Empty({
          icon: 'star',
          title: '还没有收藏',
          desc: '在笔记右上角点星标，就会出现在这里。',
        })
      )
    );
    return;
  }

  fill(
    listScroll,
    List({
      children: notes.map((note) =>
        ListRow({
          title: note.title,
          subtitle: note.path,
          meta: relTime(note.mtime),
          selected: note.path === state.current,
          onClick: () => openNote(note.path),
        })
      ),
    })
  );
}

function renderTrashList() {
  const entries = [...vault.trash].sort((a, b) => b.deletedAt - a.deletedAt);

  if (!entries.length) {
    fill(
      listScroll,
      h(
        'div',
        { class: 'ink-list__empty' },
        Empty({
          icon: 'trash',
          title: '回收站是空的',
          desc: '删掉的笔记会先在这里放一会儿。',
        })
      )
    );
    return;
  }

  fill(
    listScroll,
    List({
      children: entries.map((entry) =>
        ListRow({
          title: entry.name,
          subtitle: entry.originalPath,
          meta: relTime(entry.deletedAt),
          selected: entry.path === state.current,
          onClick: () => openTrash(entry.path),
        })
      ),
    })
  );
}

function renderSearchList() {
  const hits = state.searchResults || [];

  if (!hits.length) {
    fill(
      listScroll,
      h(
        'div',
        { class: 'ink-list__empty' },
        Empty({
          icon: 'search',
          title: '没有匹配的笔记',
          desc: '换个关键词，或者清空搜索。',
        })
      )
    );
    return;
  }

  fill(
    listScroll,
    List({
      children: hits.map((note) =>
        ListRow({
          title: note.title,
          subtitle: note.snippet || note.path,
          meta: relTime(note.mtime),
          selected: note.path === state.current,
          onClick: () => openNote(note.path),
        })
      ),
    })
  );
}

function scrollTreeTo(path) {
  requestAnimationFrame(() => {
    const row = [...listScroll.querySelectorAll('.ink-tree__row')].find(
      (el) => el.dataset.path === path
    );
    row?.scrollIntoView({ block: 'nearest' });
  });
}

function toggleFolder(path) {
  if (state.expanded.has(path)) state.expanded.delete(path);
  else state.expanded.add(path);
  state.selectedDir = path;
  vault.patchWorkspace({ expanded: [...state.expanded] });
  renderList();
}

function setSort(v) {
  state.sort = v;
  vault.patchWorkspace({ sort: v });
  vault.refresh(v).then(() => renderList());
}

/* ---------------- 文档区 ---------------- */

const titleInput = h('input', {
  class: 'ink-title selectable',
  type: 'text',
  placeholder: '无标题笔记',
  spellcheck: 'false',
  'aria-label': '笔记名称',
});

const starBtn = IconButton({ icon: 'star', label: '收藏' });
const moreBtn = IconButton({ icon: 'more', label: '更多操作' });

const docMeta = h('div', { class: 'ink-doc__meta' });

const editor = h('textarea', {
  class: 'ink-editor selectable',
  spellcheck: 'false',
  placeholder: '开始写点什么…',
  'aria-label': 'Markdown 编辑器',
});

const preview = h('article', { class: 'md-body' });

preview.addEventListener('click', (e) => {
  const link = e.target.closest?.('a[href]');
  if (!link || !isDesktop) return;
  const href = link.getAttribute('href') || '';
  if (!/^https?:/i.test(href)) return;
  e.preventDefault();
  globalThis.__TAURI__.core.invoke('open_url', { url: href }).catch(() => {});
});
const previewScroll = h('div', { class: 'ink-preview-scroll' }, preview);

const divider = h(
  'div',
  { class: 'ink-divider', title: '拖动调整分栏，双击复位' },
  h('span', { class: 'ink-divider__grip' })
);

function onDocInput() {
  resizeEditor();
  editor.dispatchEvent(new Event('input', { bubbles: true }));
}

const imageFileInput = document.createElement('input');
imageFileInput.type = 'file';
imageFileInput.accept = 'image/*';
imageFileInput.style.display = 'none';
document.body.append(imageFileInput);

imageFileInput.addEventListener('change', async () => {
  const file = imageFileInput.files?.[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = async () => {
    try {
      const base64 = reader.result;
      const rel = await vault.saveAttachment(file.name, base64);
      insertAtCursor(`\n![${file.name}](${rel})\n`);
      onDocInput();
      toast(`已插入图片: ${file.name}`);
    } catch (err) {
      toast('插入图片失败');
    }
  };
  reader.readAsDataURL(file);
  imageFileInput.value = '';
});

const insertImgBtn = IconButton({
  icon: 'image',
  label: '插入图片 (引用式)',
  onClick: () => imageFileInput.click(),
});

// 真正的流式文档架构：标题和属性放在滚动容器内部首部，向下滚动长文章时标题平滑上移！
const docHead = h(
  'div',
  { class: 'ink-doc__head' },
  h(
    'div',
    { class: 'ink-doc__row' },
    titleInput,
    h('div', { class: 'ink-doc__tools' }, insertImgBtn, starBtn, moreBtn)
  ),
  docMeta
);

const editorScroll = h(
  'div',
  { class: 'ink-editor-scroll' },
  docHead,
  editor
);

function resizeEditor() {
  const prevTop = editorScroll.scrollTop;
  editor.style.height = 'auto';
  editor.style.height = Math.max(300, editor.scrollHeight) + 'px';
  if (editorScroll.scrollTop !== prevTop) {
    editorScroll.scrollTop = prevTop;
  }
}

editor.addEventListener('input', resizeEditor);

// 监听容器尺寸变化（分栏拖动、侧栏收起、窗口缩放）
const editorResizeObserver = new ResizeObserver(() => resizeEditor());
window.addEventListener('resize', resizeEditor);

// 滚轮穿透兜底：防止 Chromium textarea 拦截并吞没父级滚动
editor.addEventListener('wheel', (e) => {
  if (editorScroll.scrollHeight > editorScroll.clientHeight) {
    editorScroll.scrollTop += e.deltaY;
    if (Math.abs(e.deltaX) > 0) editorScroll.scrollLeft += e.deltaX;
  }
}, { passive: true });

// 点击正文下方过度留白区域，编辑器文末聚焦
editorScroll.addEventListener('click', (e) => {
  if (e.target === editorScroll) {
    editor.focus();
    editor.setSelectionRange(editor.value.length, editor.value.length);
  }
});

editorResizeObserver.observe(editorScroll);
const editPane = h('div', { class: 'ink-pane ink-pane--edit' }, editorScroll);
const previewPane = h('div', { class: 'ink-pane ink-pane--preview' }, previewScroll);

const panes = h(
  'div',
  { class: 'ink-panes', 'data-mode': state.mode },
  editPane,
  divider,
  previewPane
);

function syncDocHead() {
  if (state.mode === 'preview') {
    previewScroll.prepend(docHead);
  } else {
    editorScroll.prepend(docHead);
  }
}

const saveState = h('span', { class: 'ink-status__save' }, '已保存');
const statText = h('span', { class: 'ink-status__stat' }, '0 字 · 0 行');
const emptyHost = h('div', { class: 'ink-doc__empty' });

// 编辑器剪贴板粘贴图片
editor.addEventListener('paste', async (e) => {
  const items = e.clipboardData?.items;
  if (!items) return;
  for (const item of items) {
    if (item.type.startsWith('image/')) {
      e.preventDefault();
      const blob = item.getAsFile();
      if (!blob) continue;
      const reader = new FileReader();
      reader.onload = async () => {
        try {
          const base64 = reader.result;
          const filename = `image-${Date.now()}.png`;
          const rel = await vault.saveAttachment(filename, base64);
          insertAtCursor(`\n![${filename}](${rel})\n`);
          onDocInput();
          toast('已从剪贴板粘贴图片');
        } catch (err) {
          toast('保存图片失败');
        }
      };
      reader.readAsDataURL(blob);
      return;
    }
  }
});

// 编辑器拖拽图片放入
editor.addEventListener('dragover', (e) => {
  if (e.dataTransfer?.types?.includes('Files')) {
    e.preventDefault();
  }
});
editor.addEventListener('drop', async (e) => {
  const files = e.dataTransfer?.files;
  if (!files || !files.length) return;
  for (const file of files) {
    if (file.type.startsWith('image/')) {
      e.preventDefault();
      const reader = new FileReader();
      reader.onload = async () => {
        try {
          const base64 = reader.result;
          const rel = await vault.saveAttachment(file.name, base64);
          insertAtCursor(`\n![${file.name}](${rel})\n`);
          onDocInput();
          toast(`已拖入图片: ${file.name}`);
        } catch (err) {
          toast('保存图片失败');
        }
      };
      reader.readAsDataURL(file);
      return;
    }
  }
});

// 组织 Markdown 笔记专用视区节点（完全移除外层固定的 docHead，全流式穿过毛玻璃顶栏！）
const noteView = h(
  'div',
  {
    class: 'ink-note-view',
    style: { display: 'flex', flexDirection: 'column', height: '100%', minHeight: '0', position: 'relative' },
  },
  panes,
  h(
    'footer',
    { class: 'ink-status' },
    saveState,
    h('span', { class: 'l-push' }),
    statText,
    h('span', { class: 'ink-status__sep' }),
    h('span', { class: 'ink-status__mode' }, 'Markdown')
  )
);

docPane.append(noteView, emptyHost);

/* 内容区顶部的渐进模糊：只糊滚进固定区的内容，不糊胶囊。 */
const docVeil = TopVeil();
docVeil.dataset.on = '0';
noteView.append(docVeil);

function syncDocVeil() {
  const scroller = state.mode === 'preview' ? previewScroll : editorScroll;
  const on = state.currentView === 'note' && Boolean(scroller) && scroller.scrollTop > 2;
  docVeil.dataset.on = on ? '1' : '0';
}

editorScroll.addEventListener('scroll', syncDocVeil, { passive: true });
previewScroll.addEventListener('scroll', syncDocVeil, { passive: true });

const viewHost = new ViewHost({
  container: docPane,
  vault,
  onFontChange: (area, fontName) => {
    applyFonts(vault.settings.fonts || {});
  },
  onThemeChange: () => {
    syncThemeBtn();
  },
  onPickFolder: () => pickFolder(),
  onOpenInExplorer: (p) => openInExplorer(p),
  noteViewNode: noteView,
});

panes.style.setProperty('--split', `${Math.round(state.splitRatio * 100)}%`);

/* ---------------- 右侧栏（视图注册制） ---------------- */

registerSidebarView(outlineView);
registerSidebarView(infoView);
registerSidebarView(relationsView);

const viewHosts = new Map();
const viewNodes = getSidebarViews().map((view) => {
  const node = h('div', { class: 'ink-view' });
  viewHosts.set(view.id, node);
  return { id: view.id, label: view.label, node };
});

const controllers = new Map();

/* ---------------- 链接索引（反链 / 标签） ---------------- */

let linkIndex = null;
let linkIndexing = false;
let linkIndexToken = 0;

/**
 * 全量重建链接索引。
 * 读文件走 vault 的内容缓存，1000 篇量级实测约 20ms；
 * token 保证并发触发时只有最后一次的结果会落地。
 */
async function rebuildLinkIndex() {
  const token = ++linkIndexToken;
  if (!linkIndex) linkIndexing = true;
  try {
    const entries = await Promise.all(
      vault.flatNotes().map(async (note) => ({
        path: note.path,
        title: note.title || note.name || note.path,
        content: await vault.readNote(note.path),
      }))
    );
    if (token !== linkIndexToken) return;
    linkIndex = buildIndex(entries);
  } catch (err) {
    console.warn('[links] 建立索引失败', err);
  } finally {
    if (token === linkIndexToken) {
      linkIndexing = false;
      controllers.get('relations')?.refresh?.();
    }
  }
}

const rebuildIndexSoon = debounce(() => rebuildLinkIndex(), 900);

const sidebarCtx = {
  getNote: () => currentNote(),
  getContent: () => editor.value,
  getMode: () => state.mode,
  getEditor: () => editor,
  getEditorScroll: () => editorScroll,
  getPreview: () => preview,
  getPreviewScroll: () => previewScroll,
  getEmptyLabel: () => (currentTrash() ? '这篇笔记在回收站' : '没有打开的笔记'),
  isActive: (id) => state.sidebar && state.view === id,
  jumpToLine: (line) => jumpToLine(line),

  // —— 关联视图 ——
  isIndexing: () => linkIndexing && !linkIndex,
  getBacklinks: () => (linkIndex ? findBacklinks(linkIndex, state.current) : []),
  getOutlinks: () => (linkIndex ? findOutlinks(linkIndex, state.current) : []),
  getTags: () => {
    if (!linkIndex) return [];
    const own = linkIndex.byPath.get(state.current)?.tags || [];
    return own.map((tag) => ({ tag, count: linkIndex.tags.get(tag)?.size || 1 }));
  },
  getUnresolved: () => {
    if (!linkIndex) return [];
    return findOutlinks(linkIndex, state.current)
      .filter((link) => !link.path)
      .map((link) => ({
        target: link.target,
        count: linkIndex.unresolved.get(link.target)?.size || 1,
      }));
  },
  openPath: (path) => openNote(path),
  openWikilink: (target) => openWikilinkTarget(target),
  filterBy: (text) => {
    search.input.value = text;
    runSearch(text);
  },
};

const sidebar = Inspector({
  views: viewNodes,
  value: state.view,
  onChange: (id) => {
    state.view = id;
    vault.patchWorkspace({ view: id });
    refreshActiveView();
  },
  onCollapse: () => setSidebar(false),
  label: '大纲与信息',
  className: 'ink-inspector',
});

document.querySelector('.ink-split').append(sidebar);

function mountView(id) {
  const view = getSidebarView(id);
  const host = viewHosts.get(id);
  if (!view || !host || controllers.has(id)) return;
  controllers.set(id, view.mount(host, sidebarCtx) || {});
}

function refreshActiveView() {
  mountView(state.view);
  controllers.get(state.view)?.refresh?.();
}

let autoModeBackup = null;

function toggleSidebar(force) {
  setSidebar(typeof force === 'boolean' ? force : !state.sidebar);
}

function setSidebar(open, { persist = true, autoMode = true } = {}) {
  state.sidebar = open;

  const split = document.querySelector('.ink-split');
  split.dataset.inspector = open ? '1' : '0';

  if (open) {
    if (autoMode && !autoModeBackup && state.mode === 'split') {
      const width =
        split.getBoundingClientRect().width -
        listPane.getBoundingClientRect().width -
        sidebar.getBoundingClientRect().width;
      if (width < 880) {
        autoModeBackup = state.mode;
        setMode('edit');
      }
    }
    refreshActiveView();
  } else if (autoModeBackup) {
    if (state.mode === 'edit') setMode(autoModeBackup);
    autoModeBackup = null;
  }

  const label = open ? '收起右侧栏' : '展开右侧栏';
  inspectorBtn.title = label;
  inspectorBtn.setAttribute('aria-label', label);
  inspectorBtn.setAttribute('aria-pressed', String(open));
  inspectorBtn.classList.toggle('is-on', open);

  if (persist) vault.patchWorkspace({ sidebar: open });
}

/* ---------------- 渲染文档 ---------------- */

function setSaveState(kind) {
  saveState.dataset.state = kind;
  saveState.textContent = kind === 'editing' ? '正在保存…' : '已保存';
}

async function renderPreview(note) {
  preview.innerHTML = renderMarkdown(note.content);
  const first = preview.firstElementChild;
  const title = (titleInput.value || note.title || '').trim();
  if (first && first.tagName === 'H1' && title && first.textContent.trim() === title) {
    first.remove();
  }
  await resolveMarkdownImages(preview, vault, state.current);
}

// 预览区点击事件委托（复制代码、双链跳转）
preview.addEventListener('click', async (e) => {
  // 1. 代码块复制按钮
  const copyBtn = e.target.closest('.md-code__copy');
  if (copyBtn) {
    e.preventDefault();
    const codeEl = copyBtn.closest('.md-code')?.querySelector('code');
    if (codeEl) {
      try {
        await navigator.clipboard.writeText(codeEl.textContent || '');
        const orig = copyBtn.textContent;
        copyBtn.textContent = '已复制!';
        setTimeout(() => { copyBtn.textContent = orig; }, 1500);
      } catch (err) {
        toast('复制失败', { icon: 'x-circle', tone: 'danger' });
      }
    }
    return;
  }

  // 2. 双链跳转
  const wikilink = e.target.closest('.ink-wikilink');
  if (wikilink) {
    e.preventDefault();
    await openWikilinkTarget(wikilink.dataset.target);
  }
});

/**
 * 双链跳转的唯一入口：预览区、关联视图、快捷键都走这里。
 * 目标不存在时就地新建，避免"点了没反应"。
 */
async function openWikilinkTarget(target) {
  const raw = String(target ?? '').trim();
  const clean = raw.split('#')[0].trim();
  if (!clean) return;

  const needle = clean.toLowerCase();
  const match = vault.flatNotes().find(
    (n) => (n.name || '').toLowerCase() === needle || (n.title || '').toLowerCase() === needle
  );

  if (match) {
    await openNote(match.path);
    return;
  }

  const newPath = await vault.createNote(state.selectedDir || 'Notes', clean);
  await renderAll();
  await openNote(newPath);
  toast(`已创建并跳转到: ${clean}`);
}

// 预览区任务清单复选框双向互动回写
preview.addEventListener('change', (e) => {
  const checkbox = e.target.closest('.md-task input[type="checkbox"]');
  if (!checkbox) return;
  const lineNum = parseInt(checkbox.dataset.line, 10);
  if (isNaN(lineNum)) return;

  const lines = editor.value.split('\n');
  if (lineNum >= 0 && lineNum < lines.length) {
    const isChecked = checkbox.checked;
    if (isChecked) {
      lines[lineNum] = lines[lineNum].replace(/^(\s*[-*+]\s+\[) (\]\s*.*)$/, '$1x$2');
    } else {
      lines[lineNum] = lines[lineNum].replace(/^(\s*[-*+]\s+\[)x(\]\s*.*)$/i, '$1 $2');
    }
    editor.value = lines.join('\n');
    editor.dispatchEvent(new Event('input', { bubbles: true }));
  }
});

function updateMeta(note) {
  fill(
    docMeta,
    h('span', null, `更新于 ${relTime(note.mtime)}`),
    h('span', { class: 'ink-doc__dot' }),
    h('span', null, `${charCount(editor.value)} 字`),
    h('span', { class: 'ink-doc__dot' }),
    h('span', null, note.favorite ? '已收藏' : '本地文件')
  );
}

function updateStats(content) {
  statText.textContent = `${charCount(content)} 字 · ${lineCount(content)} 行`;
}

function updateStar(note) {
  starBtn.classList.toggle('is-on', note.favorite);
  starBtn.setAttribute('aria-pressed', String(note.favorite));
  const label = note.favorite ? '取消收藏' : '收藏';
  starBtn.title = label;
  starBtn.setAttribute('aria-label', label);
}

function updateTopbar() {
  const isNote = state.currentView === 'note';
  viewCapsule.style.display = isNote ? '' : 'none';

  const note = currentNote();
  const pageTitle = PAGE_META[state.page]?.title || '文件';
  const docTitle = note?.title || note?.name;
  document.title = docTitle ? `${docTitle} · Inkdown` : `${pageTitle} · Inkdown`;

  if (state.currentView === 'settings') {
    fill(
      topbarBreadcrumb,
      h('span', { class: 'c-topbar__crumb-icon' }, iconEl('sliders', 13)),
      h('span', { class: 'c-topbar__crumb-part c-topbar__crumb-title' }, '偏好设置')
    );
    return;
  }

  if (state.page === 'trash') {
    const trash = currentTrash();
    fill(
      topbarBreadcrumb,
      h('span', { class: 'c-topbar__crumb-icon' }, iconEl('trash', 13)),
      h('span', { class: 'c-topbar__crumb-part' }, '回收站'),
      h('span', { class: 'c-topbar__crumb-sep' }, '/'),
      h('span', { class: 'c-topbar__crumb-part c-topbar__crumb-title' }, trash?.title || '已删除笔记')
    );
    return;
  }

  if (!note) {
    fill(
      topbarBreadcrumb,
      h('span', { class: 'c-topbar__crumb-icon' }, iconEl('feather', 13)),
      h('span', { class: 'c-topbar__crumb-part c-topbar__crumb-title' }, vault.meta.name)
    );
    return;
  }

  const folder = dirname(note.path);
  const crumbs = [];
  const isMd = isMarkdown(note.path);
  let crumbIcon = 'file-text';
  if (!isMd) {
    const ft = note.fileType || getFileType(note.path);
    if (ft === 'code') crumbIcon = 'code';
    else if (ft === 'pdf') crumbIcon = 'file';
    else if (['image', 'video', 'audio'].includes(ft)) crumbIcon = 'image';
    else if (['excel', 'word', 'ppt'].includes(ft)) crumbIcon = 'table';
    else crumbIcon = 'file';
  }
  crumbs.push(
    h('span', { class: 'c-topbar__crumb-icon' }, iconEl(crumbIcon, 13)),
    h('span', { class: 'c-topbar__crumb-part c-topbar__crumb-vault' }, vault.meta.name),
    h('span', { class: 'c-topbar__crumb-sep' }, '/')
  );
  if (folder) {
    crumbs.push(
      h('span', { class: 'c-topbar__crumb-part c-topbar__crumb-dir' }, folder),
      h('span', { class: 'c-topbar__crumb-sep' }, '/')
    );
  }
  crumbs.push(
    h('span', { class: 'c-topbar__crumb-part c-topbar__crumb-title' }, note.title || basename(note.path))
  );
  fill(topbarBreadcrumb, ...crumbs);
}

function showEmpty({ icon: ic = 'square-dashed', title, desc, action = null }) {
  docPane.dataset.empty = '1';
  fill(emptyHost, Empty({ icon: ic, title, desc, action }));
}

async function renderDoc() {
  const token = ++loadToken;

  // 非 Markdown 视图不介入文档渲染
  if (state.currentView !== 'note') return;

  const trash = currentTrash();

  if (trash) {
    showEmpty({
      icon: 'trash',
      title: '这篇笔记在回收站',
      desc: `原位置：${trash.originalPath}`,
      action: h(
        'div',
        { class: 'l-row l-row--4' },
        Button({
          label: '恢复',
          icon: 'rotate-ccw',
          variant: 'primary',
          onClick: () => restoreCurrent(),
        }),
        Button({
          label: '彻底删除',
          icon: 'trash',
          variant: 'danger',
          onClick: () => deleteForeverCurrent(),
        })
      ),
    });
    refreshActiveView();
    return;
  }

  const note = currentNote();
  if (!note) {
    const hasNotes = vault.counts().all > 0;
    const emptyPage =
      state.page === 'favorites' && !vault.counts().favorites
        ? { icon: 'star', title: '还没有收藏', desc: '在笔记右上角点星标，就会出现在这里。' }
        : null;

    showEmpty({
      icon: emptyPage?.icon || (hasNotes ? 'square-pen' : 'square-dashed'),
      title: emptyPage?.title || (hasNotes ? '选择一篇笔记' : '还没有笔记'),
      desc:
        emptyPage?.desc ||
        (hasNotes ? '从左边挑一篇，或者新建一篇。' : '按下 ⌘ N，写下第一行。'),
      action:
        hasNotes || emptyPage
          ? null
          : Button({
              label: '新建笔记',
              icon: 'plus',
              variant: 'primary',
              onClick: () => newNote(),
            }),
    });
    refreshActiveView();
    return;
  }

  if (!isMarkdown(note.path)) return;

  try {
    const text = await vault.readNote(note.path);
    if (token !== loadToken) return;

    docPane.dataset.empty = '0';
    titleInput.value = note.title;
    editor.value = text;
    lastLoaded = text;
    renderPreview({ content: text, title: note.title });
    updateMeta(note);
    updateStar(note);
    updateStats(text);
    setSaveState('saved');
    syncDocHead();
    resizeEditor();
    editor.scrollTop = 0;
    editorScroll.scrollTop = 0;
    previewScroll.scrollTop = 0;
    refreshActiveView();
  } catch (err) {
    console.error('加载文档内容失败:', err);
  }
}

async function renderAll() {
  refreshNav();
  renderList();
  renderRecents();
  await renderDoc();
  updateTopbar();
  // 增删改都会走这里，索引跟着重建（去抖，避免连续操作时反复全量扫描）
  rebuildIndexSoon();
}

/* ---------------- 打开 / 新建 / 重命名 ---------------- */

async function openNote(path, shouldPushTab = true, isHistoryNav = false) {
  if (state.page === 'trash') return;
  await titleTask;
  await commitDoc();

  if (!isHistoryNav && path) {
    if (state.navHistory[state.navIndex] !== path) {
      state.navHistory = state.navHistory.slice(0, state.navIndex + 1);
      state.navHistory.push(path);
      state.navIndex = state.navHistory.length - 1;
      updateNavHistoryButtons();
    }
  }

  state.current = path;
  state.selectedDir = dirname(path);
  if (state.query.trim()) {
    state.query = '';
    state.searchResults = null;
    search.input.value = '';
  }

  expandTo(path);
  vault.patchWorkspace({ lastNote: path, expanded: [...state.expanded] });

  const isMd = isMarkdown(path);
  const ft = getFileType(path);
  const node = vault.noteAt(path) || {
    path,
    name: basename(path),
    title: basename(path),
    fileType: ft,
    isMarkdown: isMd,
  };
  node.isMarkdown = isMd;
  node.fileType = ft;

  let iconName = 'file-text';
  if (!isMd) {
    if (ft === 'image') iconName = 'image';
    else if (ft === 'video') iconName = 'video';
    else if (ft === 'audio') iconName = 'volume-2';
    else if (ft === 'pdf') iconName = 'file';
    else if (ft === 'word') iconName = 'file-text';
    else if (ft === 'excel') iconName = 'table';
    else if (ft === 'ppt') iconName = 'presentation';
    else if (ft === 'code') iconName = 'code';
    else iconName = 'file';
  }

  state.currentView = isMd ? 'note' : (node.fileType || 'other');
  addRecent(path);
  await viewHost.switchView({
    id: path,
    label: node.title || node.name,
    icon: iconName,
    type: isMd ? 'note' : node.fileType,
    path,
  }, {
    renderNote: (p) => renderDocForPath(p),
  });

  await renderAll();
  scrollTreeTo(path);
}

async function renderDocForPath(path) {
  state.current = path;
  await renderDoc();
}

async function openTrash(path) {
  await titleTask;
  await commitDoc();
  state.current = path;
  await renderAll();
}

async function go(page) {
  if (state.page === page) return;
  await titleTask;
  await commitDoc();

  state.page = page;
  state.query = '';
  state.searchResults = null;
  search.input.value = '';

  if (page === 'trash') {
    state.current = [...vault.trash].sort((a, b) => b.deletedAt - a.deletedAt)[0]?.path ?? '';
  } else if (page === 'favorites') {
    state.current = vault.favorites()[0]?.path ?? '';
  } else {
    state.current = vault.noteAt(state.current) ? state.current : pickInitialNote();
  }

  await renderAll();
}

function targetDir() {
  if (
    state.selectedDir &&
    vault.entries.some((e) => e.type === 'dir' && e.path === state.selectedDir)
  ) {
    return state.selectedDir;
  }
  const note = currentNote();
  return note ? dirname(note.path) : '';
}

async function newNote(dir = targetDir()) {
  await titleTask;
  await commitDoc();
  const path = await vault.createNote(dir);

  state.page = 'files';
  state.query = '';
  state.searchResults = null;
  search.input.value = '';
  state.current = path;
  state.selectedDir = dir;
  state.expanded.add(dir);
  expandTo(path);

  vault.patchWorkspace({ lastNote: path, expanded: [...state.expanded] });
  await renderAll();
  scrollTreeTo(path);

  titleInput.focus();
  titleInput.select();
}

async function newFolder(dir = targetDir()) {
  const path = await vault.createFolder(dir);
  state.expanded.add(dir);
  state.expanded.add(path);
  state.selectedDir = path;
  vault.patchWorkspace({ expanded: [...state.expanded] });
  await renderAll();

  scrollTreeTo(path);
  const row = [...listScroll.querySelectorAll('.ink-tree__row')].find(
    (el) => el.dataset.path === path
  );
  if (row) startRenameFolder(path, row);
}

function commitTitle() {
  const note = currentNote();
  if (!note) return titleTask;

  const next = titleInput.value.trim();
  if (!next || next === note.title) {
    titleInput.value = note.title;
    return titleTask;
  }

  titleTask = (async () => {
    const res = await vault.renameNote(note.path, next);
    if (!res.ok) {
      toast(res.error, { icon: 'x-circle', tone: 'danger' });
      titleInput.value = note.title;
      return;
    }

    if (state.current === note.path) state.current = res.path;
    state.recents = state.recents.map((p) => (p === note.path ? res.path : p));
    vault.patchWorkspace({ lastNote: state.current, recents: state.recents });

    if (res.renamed) {
      const title = vault.noteAt(res.path)?.title ?? next;
      toast(`名称已存在，已改为「${title}」`, { icon: 'info' });
    }

    await renderAll();
  })();

  return titleTask;
}

function startRenameFolder(path, rowEl) {
  const nameEl = rowEl.querySelector('.ink-tree__name');
  if (!nameEl) return;

  const old = nameEl.textContent;
  const input = h('input', {
    class: 'ink-tree__rename selectable',
    type: 'text',
    value: old,
    spellcheck: 'false',
  });
  nameEl.replaceWith(input);
  input.focus();
  input.select();

  let done = false;
  const finish = async (commit) => {
    if (done) return;
    done = true;

    const value = input.value.trim();
    if (!commit || !value || value === old) {
      renderList();
      return;
    }

    const res = await vault.renameFolder(path, value);
    if (!res.ok) toast(res.error, { icon: 'x-circle', tone: 'danger' });

    if (res.ok) {
      const nextExpanded = new Set();
      for (const p of state.expanded) {
        if (p === path) nextExpanded.add(res.path);
        else if (p.startsWith(`${path}/`)) nextExpanded.add(`${res.path}${p.slice(path.length)}`);
        else nextExpanded.add(p);
      }
      state.expanded = nextExpanded;

      if (state.current.startsWith(`${path}/`)) {
        state.current = `${res.path}${state.current.slice(path.length)}`;
      }
      if (state.selectedDir.startsWith(path)) {
        state.selectedDir = `${res.path}${state.selectedDir.slice(path.length)}`;
      }

      vault.patchWorkspace({ expanded: [...state.expanded], lastNote: state.current });
      if (res.renamed) {
        toast(`名称已存在，已改为「${res.path.split('/').pop()}」`, { icon: 'info' });
      }
    }

    await renderAll();
  };

  input.addEventListener('blur', () => finish(true));
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      finish(true);
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      finish(false);
    }
  });
}

/* ---------------- 保存 ---------------- */

async function commitDoc() {
  await titleTask;
  const note = currentNote();
  if (!note) return;

  const text = editor.value;
  if (text === lastLoaded) {
    setSaveState('saved');
    return;
  }

  setSaveState('editing');
  const mtime = await vault.saveNote(note.path, text);
  lastLoaded = text;
  setSaveState('saved');
  updateMeta({ ...note, mtime });
  // 正文变了 → 反链/标签可能变了，索引跟着更新
  rebuildIndexSoon();
}

const commitSoon = debounce(commitDoc, 500);
const refreshViewSoon = debounce(() => refreshActiveView(), 260);

editor.addEventListener('input', () => {
  if (!currentNote()) return;
  renderPreview({ content: editor.value, title: titleInput.value });
  updateStats(editor.value);
  updateMeta(currentNote());
  setSaveState('editing');
  commitSoon();
  refreshViewSoon();
});

titleInput.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') {
    e.preventDefault();
    commitTitle().then(() => editor.focus());
  }
  if (e.key === 'Escape') {
    e.preventDefault();
    titleInput.value = currentNote()?.title ?? '';
    titleInput.blur();
  }
});

titleInput.addEventListener('blur', () => commitTitle());

function insertAtCursor(text) {
  editor.focus();
  if (document.execCommand?.('insertText', false, text)) return;

  const start = editor.selectionStart;
  const end = editor.selectionEnd;
  editor.setRangeText(text, start, end, 'end');
  editor.dispatchEvent(new Event('input', { bubbles: true }));
}

function wrapSelection(marker) {
  editor.focus();
  const start = editor.selectionStart;
  const end = editor.selectionEnd;
  const selected = editor.value.slice(start, end) || '文字';
  const next = `${marker}${selected}${marker}`;

  const ok = document.execCommand?.('insertText', false, next);
  if (!ok) {
    editor.setRangeText(next, start, end, 'end');
    editor.dispatchEvent(new Event('input', { bubbles: true }));
  }

  editor.setSelectionRange(start + marker.length, start + marker.length + selected.length);
}

// 高级编辑器键盘行为：Tab/Shift+Tab 多行缩进反缩进、智能列表延续与优雅退出、Markdown 快捷键

/* ---------------- 单篇查找与替换 (Find & Replace) ---------------- */

let findBarEl = null;
let findMatches = [];
let findMatchIndex = -1;

function openFindBar(showReplace = false) {
  if (findBarEl) {
    const input = findBarEl.querySelector('.ink-findbar__input');
    if (input) {
      input.focus();
      input.select();
    }
    const replaceRow = findBarEl.querySelector('.ink-findbar__replace-row');
    if (replaceRow && showReplace) replaceRow.style.display = 'flex';
    return;
  }

  let selectedText = '';
  if (document.activeElement === editor) {
    selectedText = editor.value.slice(editor.selectionStart, editor.selectionEnd);
  }

  const findInput = h('input', {
    class: 'ink-findbar__input',
    placeholder: '查找...',
    value: selectedText,
    spellcheck: 'false',
  });

  const countBadge = h('span', { class: 'ink-findbar__count' }, '0/0');

  const replaceInput = h('input', {
    class: 'ink-findbar__input',
    placeholder: '替换为...',
    spellcheck: 'false',
  });

  function performSearch() {
    const q = findInput.value;
    findMatches = [];
    findMatchIndex = -1;

    if (!q) {
      countBadge.textContent = '0/0';
      return;
    }

    const val = editor.value;
    const lowerVal = val.toLowerCase();
    const lowerQ = q.toLowerCase();
    let idx = 0;
    while ((idx = lowerVal.indexOf(lowerQ, idx)) !== -1) {
      findMatches.push({ start: idx, end: idx + q.length });
      idx += q.length;
    }

    if (findMatches.length > 0) {
      findMatchIndex = 0;
      countBadge.textContent = '1/' + findMatches.length;
      highlightCurrentMatch();
    } else {
      countBadge.textContent = '无结果';
    }
  }

  function highlightCurrentMatch() {
    if (findMatchIndex < 0 || findMatchIndex >= findMatches.length) return;
    const m = findMatches[findMatchIndex];
    editor.focus();
    editor.setSelectionRange(m.start, m.end);

    const headH = docHead?.offsetHeight || 0;
    const targetY = headH + caretLineTop(editor, m.start) - (editorScroll.clientHeight / 3);
    editorScroll.scrollTo({ top: Math.max(0, targetY), behavior: 'smooth' });
    countBadge.textContent = (findMatchIndex + 1) + '/' + findMatches.length;
  }

  function nextMatch() {
    if (findMatches.length === 0) return;
    findMatchIndex = (findMatchIndex + 1) % findMatches.length;
    highlightCurrentMatch();
  }

  function prevMatch() {
    if (findMatches.length === 0) return;
    findMatchIndex = (findMatchIndex - 1 + findMatches.length) % findMatches.length;
    highlightCurrentMatch();
  }

  function replaceCurrent() {
    if (findMatchIndex < 0 || findMatchIndex >= findMatches.length) return;
    const m = findMatches[findMatchIndex];
    const rep = replaceInput.value;
    editor.setRangeText(rep, m.start, m.end, 'end');
    editor.dispatchEvent(new Event('input', { bubbles: true }));
    performSearch();
  }

  function replaceAll() {
    const q = findInput.value;
    if (!q) return;
    const rep = replaceInput.value;
    const count = findMatches.length;
    if (count === 0) return;

    const escaped = q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const parts = editor.value.split(new RegExp(escaped, 'gi'));
    editor.value = parts.join(rep);
    editor.dispatchEvent(new Event('input', { bubbles: true }));
    performSearch();
    toast('已替换 ' + count + ' 处内容');
  }

  findInput.addEventListener('input', performSearch);
  findInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (e.shiftKey) prevMatch();
      else nextMatch();
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      closeFindBar();
      return;
    }
  });

  replaceInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      replaceCurrent();
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      closeFindBar();
      return;
    }
  });

  const replaceRow = h(
    'div',
    {
      class: 'ink-findbar__row ink-findbar__replace-row',
      style: { display: showReplace ? 'flex' : 'none' },
    },
    h('div', { class: 'ink-findbar__input-wrap' }, replaceInput),
    h('button', { class: 'ink-findbar__action-btn', onclick: replaceCurrent }, '替换'),
    h('button', { class: 'ink-findbar__action-btn', onclick: replaceAll }, '全部替换')
  );

  const toggleReplaceBtn = h(
    'button',
    {
      class: 'ink-findbar__btn',
      title: '切换替换模式 (Ctrl+H)',
      onclick: () => {
        replaceRow.style.display = replaceRow.style.display === 'none' ? 'flex' : 'none';
        if (replaceRow.style.display === 'flex') replaceInput.focus();
      },
    },
    iconEl('sliders', 13)
  );

  const closeBtn = h(
    'button',
    {
      class: 'ink-findbar__btn',
      title: '关闭 (Esc)',
      onclick: closeFindBar,
    },
    iconEl('x', 14)
  );

  const prevBtn = h(
    'button',
    { class: 'ink-findbar__btn', title: '上一个 (Shift+Enter)', onclick: prevMatch },
    iconEl('chevron-up', 13)
  );

  const nextBtn = h(
    'button',
    { class: 'ink-findbar__btn', title: '下一个 (Enter)', onclick: nextMatch },
    iconEl('chevron-down', 13)
  );

  findBarEl = h(
    'div',
    { class: 'ink-findbar' },
    h(
      'div',
      { class: 'ink-findbar__row' },
      h('div', { class: 'ink-findbar__input-wrap' }, findInput, countBadge),
      prevBtn,
      nextBtn,
      toggleReplaceBtn,
      closeBtn
    ),
    replaceRow
  );

  editPane.append(findBarEl);
  if (selectedText) performSearch();
  setTimeout(() => {
    findInput.focus();
    if (selectedText) findInput.select();
  }, 10);
}

function closeFindBar() {
  if (findBarEl) {
    findBarEl.remove();
    findBarEl = null;
    findMatches = [];
    findMatchIndex = -1;
    editor.focus();
  }
}

/* ---------------- 键入 [[ 自动提示笔记 (Wikilink Auto-suggest) ---------------- */

let wikiPopupEl = null;
let wikiCandidates = [];
let wikiSelectedIndex = 0;
let wikiQueryStart = -1;

function checkWikilinkSuggest() {
  if (state.currentView !== 'note') {
    closeWikiPopup();
    return;
  }

  const cursor = editor.selectionStart;
  const val = editor.value;
  const textBefore = val.slice(0, cursor);

  const match = textBefore.match(/\[\[([^\]\n]*)$/);
  if (!match) {
    closeWikiPopup();
    return;
  }

  const query = match[1].toLowerCase();
  wikiQueryStart = cursor - match[1].length;

  const allNotes = vault.flatNotes();
  wikiCandidates = allNotes
    .filter(n => (n.title || n.name).toLowerCase().includes(query))
    .slice(0, 6);

  if (wikiCandidates.length === 0) {
    closeWikiPopup();
    return;
  }

  wikiSelectedIndex = 0;
  renderWikiPopup();
}

function renderWikiPopup() {
  if (!wikiPopupEl) {
    wikiPopupEl = h('div', { class: 'ink-wiki-popup' });
    editPane.append(wikiPopupEl);
  }

  wikiPopupEl.innerHTML = '';
  wikiCandidates.forEach((cand, idx) => {
    const item = h(
      'div',
      {
        class: 'ink-wiki-popup__item ' + (idx === wikiSelectedIndex ? 'is-selected' : ''),
        onmousedown: (e) => {
          e.preventDefault();
          applyWikiSuggest(cand);
        },
      },
      iconEl('file-text', 13),
      h('span', null, cand.title || cand.name)
    );
    wikiPopupEl.append(item);
  });

  const headH = docHead?.offsetHeight || 0;
  const caretY = headH + caretLineTop(editor, editor.selectionStart);
  const scrollY = editorScroll.scrollTop;
  const topPos = Math.max(10, caretY - scrollY + 28);

  wikiPopupEl.style.top = topPos + 'px';
  wikiPopupEl.style.left = '60px';
}

function applyWikiSuggest(cand) {
  const targetTitle = cand.title || cand.name;
  const val = editor.value;
  const cursor = editor.selectionStart;
  const before = val.slice(0, wikiQueryStart);
  const after = val.slice(cursor);

  editor.value = before + targetTitle + ']]' + after;
  const nextPos = before.length + targetTitle.length + 2;
  editor.setSelectionRange(nextPos, nextPos);
  editor.dispatchEvent(new Event('input', { bubbles: true }));
  closeWikiPopup();
  editor.focus();
}

function closeWikiPopup() {
  if (wikiPopupEl) {
    wikiPopupEl.remove();
    wikiPopupEl = null;
    wikiCandidates = [];
  }
}

/* ---------------- 字号与缩放 (Zoom) ---------------- */

let currentFs = parseFloat(vault.workspace.fontSize) || 14;

function setEditorFontSize(fs) {
  currentFs = Math.max(11, Math.min(26, fs));
  document.documentElement.style.setProperty('--ink-editor-fs', currentFs + 'px');
  vault.patchWorkspace({ fontSize: currentFs });
}

function adjustZoom(delta) {
  setEditorFontSize(currentFs + delta);
  toast('字号: ' + currentFs + 'px', { duration: 1000 });
}

function resetZoom() {
  setEditorFontSize(14);
  toast('字号已复位 (14px)', { duration: 1000 });
}

if (vault.workspace.fontSize) {
  setEditorFontSize(Number(vault.workspace.fontSize));
}

editor.addEventListener('keydown', (e) => {
  const mod = e.metaKey || e.ctrlKey;

  // 0. Wikilink 补全拦截
  if (wikiPopupEl && wikiCandidates.length > 0) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      wikiSelectedIndex = (wikiSelectedIndex + 1) % wikiCandidates.length;
      renderWikiPopup();
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      wikiSelectedIndex = (wikiSelectedIndex - 1 + wikiCandidates.length) % wikiCandidates.length;
      renderWikiPopup();
      return;
    }
    if (e.key === 'Enter' || e.key === 'Tab') {
      e.preventDefault();
      applyWikiSuggest(wikiCandidates[wikiSelectedIndex]);
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      closeWikiPopup();
      return;
    }
  }
  const start = editor.selectionStart;
  const end = editor.selectionEnd;
  const val = editor.value;

  // 1. Tab 键缩进与 Shift+Tab 反缩进
  if (e.key === 'Tab') {
    e.preventDefault();
    if (start === end && !e.shiftKey) {
      insertAtCursor('  ');
    } else {
      const lineStart = val.lastIndexOf('\n', start - 1) + 1;
      const lineEnd = val.indexOf('\n', end);
      const effectiveEnd = lineEnd === -1 ? val.length : lineEnd;
      const lines = val.slice(lineStart, effectiveEnd).split('\n');

      let modifiedLines;
      if (e.shiftKey) {
        modifiedLines = lines.map((l) => l.replace(/^( {1,2}|\t)/, ''));
      } else {
        modifiedLines = lines.map((l) => '  ' + l);
      }
      const nextText = modifiedLines.join('\n');
      editor.setRangeText(nextText, lineStart, effectiveEnd, 'select');
      editor.dispatchEvent(new Event('input', { bubbles: true }));
    }
    return;
  }

  // 2. Enter 键智能列表延续与退出
  if (e.key === 'Enter' && !e.shiftKey) {
    const lineStart = val.lastIndexOf('\n', start - 1) + 1;
    const curLine = val.slice(lineStart, start);

    const taskMatch = curLine.match(/^(\s*)([-*+]\s+\[(?: |x)\]\s+)(.*)$/i);
    const numMatch = curLine.match(/^(\s*)(\d+)\.\s+(.*)$/);
    const bulletMatch = curLine.match(/^(\s*)([-*+])\s+(.*)$/);

    if (taskMatch) {
      e.preventDefault();
      const [_, indent, prefix, contentText] = taskMatch;
      if (!contentText.trim()) {
        editor.setRangeText('', lineStart, start, 'end');
      } else {
        insertAtCursor(`\n${indent}- [ ] `);
      }
      editor.dispatchEvent(new Event('input', { bubbles: true }));
      return;
    }

    if (numMatch) {
      e.preventDefault();
      const [_, indent, numStr, contentText] = numMatch;
      if (!contentText.trim()) {
        editor.setRangeText('', lineStart, start, 'end');
      } else {
        const nextNum = parseInt(numStr, 10) + 1;
        insertAtCursor(`\n${indent}${nextNum}. `);
      }
      editor.dispatchEvent(new Event('input', { bubbles: true }));
      return;
    }

    if (bulletMatch) {
      e.preventDefault();
      const [_, indent, bullet, contentText] = bulletMatch;
      if (!contentText.trim()) {
        editor.setRangeText('', lineStart, start, 'end');
      } else {
        insertAtCursor(`\n${indent}${bullet} `);
      }
      editor.dispatchEvent(new Event('input', { bubbles: true }));
      return;
    }
  }

  // 3. 编辑态下的 Markdown 专用快捷键
  if (mod) {
    const k = e.key.toLowerCase();
    if (k === 'k') {
      e.preventDefault();
      e.stopPropagation();
      const sel = val.slice(start, end) || '链接文本';
      const template = `[${sel}](url)`;
      insertAtCursor(template);
      editor.setSelectionRange(start + sel.length + 3, start + sel.length + 6);
      return;
    }
    if (e.key === '`') {
      e.preventDefault();
      wrapSelection('`');
      return;
    }
    if (e.shiftKey && k === 'x') {
      e.preventDefault();
      wrapSelection('~~');
      return;
    }
  }
});

// 分栏主动源锁同步（彻底根除 30ms 互锁死循环）
let activeScrollSource = null;
let activeScrollTimer = 0;

function syncSplitScroll(source, target) {
  if (state.mode !== 'split') return;
  if (activeScrollSource && activeScrollSource !== source) return;

  activeScrollSource = source;
  clearTimeout(activeScrollTimer);
  activeScrollTimer = setTimeout(() => {
    activeScrollSource = null;
  }, 120);

  const sRange = source.scrollHeight - source.clientHeight;
  const tRange = target.scrollHeight - target.clientHeight;
  if (sRange > 1 && tRange > 1) {
    target.scrollTop = (source.scrollTop / sRange) * tRange;
  }
}

editorScroll.addEventListener('scroll', () => syncSplitScroll(editorScroll, previewScroll), { passive: true });
previewScroll.addEventListener('scroll', () => syncSplitScroll(previewScroll, editorScroll), { passive: true });

/* ---------------- 收藏 / 回收站 ---------------- */

async function toggleFavorite(note) {
  await titleTask;
  // 改名可能刚发生：路径失效时退回当前笔记
  const live = vault.noteAt(note.path) ?? currentNote();
  if (!live) return;

  const favorite = await vault.toggleFavorite(live.path);
  if (state.current === live.path) updateStar({ ...live, favorite });
  refreshNav();
  renderList();
  refreshActiveView();
  toast(favorite ? '已加入收藏' : '已取消收藏', { icon: 'star' });
}

async function trashNote(path) {
  await titleTask;
  await commitDoc();

  const target = vault.noteAt(path) ? path : currentNote()?.path;
  if (!target) return;

  await vault.trashNote(target);
  state.recents = state.recents.filter((p) => p !== target);
  vault.patchWorkspace({ recents: state.recents });
  if (state.current === target) state.current = '';

  refreshNav();
  await renderAll();
  toast('已移到回收站', { icon: 'trash' });
}

async function restoreCurrent() {
  const entry = currentTrash();
  if (!entry) return;

  const res = await vault.restore(entry.path);
  if (!res.ok) {
    toast(res.error, { icon: 'x-circle', tone: 'danger' });
    return;
  }

  state.page = 'files';
  state.current = res.path;
  expandTo(res.path);
  vault.patchWorkspace({ lastNote: res.path, expanded: [...state.expanded] });
  await renderAll();
  scrollTreeTo(res.path);
  toast('已恢复', { icon: 'rotate-ccw' });
}

async function deleteForeverCurrent() {
  const entry = currentTrash();
  if (!entry) return;

  const ok = await confirmDialog({
    title: `彻底删除「${entry.name}」？`,
    desc: '删除后无法恢复。',
    confirmLabel: '删除',
    tone: 'danger',
  });
  if (!ok) return;

  await vault.deleteForever(entry.path);
  state.current = '';
  refreshNav();
  await renderAll();
  toast('已删除', { icon: 'trash' });
}

async function deleteFolder(node) {
  const ok = await confirmDialog({
    title: `删除文件夹「${node.name}」？`,
    desc: '文件夹里的笔记会一起删除，且无法恢复。',
    confirmLabel: '删除',
    tone: 'danger',
  });
  if (!ok) return;

  await vault.deleteFolder(node.path);
  if (state.current.startsWith(`${node.path}/`)) state.current = '';
  refreshNav();
  await renderAll();
  toast('文件夹已删除', { icon: 'trash' });
}

/* ---------------- 右键菜单 ---------------- */

function noteMenu(note, x, y) {
  openMenu({
    x,
    y,
    items: [
      {
        label: note.favorite ? '取消收藏' : '加入收藏',
        icon: 'star',
        onClick: () => toggleFavorite(note),
      },
      {
        label: '重命名',
        icon: 'square-pen',
        onClick: () => {
          openNote(note.path).then(() => {
            titleInput.focus();
            titleInput.select();
          });
        },
      },
      { sep: true },
      {
        label: '在资源管理器中定位',
        icon: 'folder-open',
        onClick: () => openInExplorer(note.path),
      },
      {
        label: '复制文件绝对路径',
        icon: 'copy',
        onClick: () => copyVaultPath(note.path),
      },
      {
        label: '复制相对路径',
        icon: 'link',
        onClick: async () => {
          await navigator.clipboard.writeText(note.path);
          toast('相对路径已复制');
        },
      },
      { sep: true },
      {
        label: '移到回收站',
        icon: 'trash',
        danger: true,
        onClick: () => trashNote(note.path),
      },
    ],
  });
}

function folderMenu(node, x, y) {
  openMenu({
    x,
    y,
    items: [
      { label: '新建笔记', icon: 'plus', onClick: () => newNote(node.path) },
      { label: '新建文件夹', icon: 'folder-plus', onClick: () => newFolder(node.path) },
      { sep: true },
      {
        label: '在资源管理器中打开此目录',
        icon: 'folder-open',
        onClick: () => openInExplorer(node.path),
      },
      {
        label: '复制文件夹路径',
        icon: 'copy',
        onClick: () => copyVaultPath(node.path),
      },
      { sep: true },
      {
        label: '重命名',
        icon: 'square-pen',
        onClick: () => {
          const row = [...listScroll.querySelectorAll('.ink-tree__row')].find(
            (el) => el.dataset.path === node.path
          );
          if (row) startRenameFolder(node.path, row);
        },
      },
      {
        label: '删除文件夹',
        icon: 'trash',
        danger: true,
        onClick: () => deleteFolder(node),
      },
    ],
  });
}

starBtn.addEventListener('click', () => {
  const note = currentNote();
  if (note) toggleFavorite(note);
});

moreBtn.addEventListener('click', () => {
  const note = currentNote();
  if (!note) return;

  openMenu({
    anchor: moreBtn,
    items: [
      {
        label: note.favorite ? '取消收藏' : '加入收藏',
        icon: 'star',
        onClick: () => toggleFavorite(note),
      },
      {
        label: '复制 Markdown',
        icon: 'layers',
        onClick: async () => {
          try {
            await navigator.clipboard.writeText(editor.value);
            toast('已复制到剪贴板', { icon: 'check' });
          } catch {
            toast('复制失败', { icon: 'x-circle', tone: 'danger' });
          }
        },
      },
      {
        label: '导出 .md 文件',
        icon: 'download',
        onClick: () => exportNote(note),
      },
      { sep: true },
      {
        label: '移到回收站',
        icon: 'trash',
        danger: true,
        onClick: () => trashNote(note.path),
      },
    ],
  });
});

function exportNote(note) {
  const blob = new Blob([editor.value], { type: 'text/markdown;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = h('a', { href: url, download: `${note.title}.md` });
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  toast('已导出 Markdown', { icon: 'download' });
}

/* ---------------- 视图模式 / 跳转 ---------------- */

function setMode(v) {
  state.mode = v;
  panes.dataset.mode = v;
  docPane.dataset.mode = v;
  viewSeg.set(v);
  vault.patchWorkspace({ mode: v });
  syncDocHead();
  resizeEditor();
  syncDocVeil();
}

function caretLineTop(ta, offset) {
  const cs = getComputedStyle(ta);
  const mirror = document.createElement('div');
  const props = [
    'fontFamily',
    'fontSize',
    'fontWeight',
    'fontStyle',
    'lineHeight',
    'letterSpacing',
    'tabSize',
    'paddingTop',
    'paddingRight',
    'paddingBottom',
    'paddingLeft',
    'borderTopWidth',
    'borderRightWidth',
    'borderBottomWidth',
    'borderLeftWidth',
    'boxSizing',
  ];
  for (const p of props) mirror.style[p] = cs[p];

  mirror.style.position = 'absolute';
  mirror.style.left = '-99999px';
  mirror.style.top = '0';
  mirror.style.width = `${ta.clientWidth}px`;
  mirror.style.height = 'auto';
  mirror.style.visibility = 'hidden';
  mirror.style.whiteSpace = 'pre-wrap';
  mirror.style.wordBreak = 'break-word';
  mirror.textContent = ta.value.slice(0, offset);
  document.body.append(mirror);

  const lh = parseFloat(cs.lineHeight) || 24;
  const endsWithNewline = mirror.textContent.endsWith('\n');
  const top =
    mirror.scrollHeight -
    (parseFloat(cs.paddingBottom) || 0) -
    (endsWithNewline ? 0 : lh);
  mirror.remove();
  return Math.max(0, top);
}

function jumpToLine(line) {
  const offset = lineOffset(editor.value, line);
  const headH = docHead?.offsetHeight || 0;

  if (state.mode === 'edit') {
    editor.focus();
    editor.setSelectionRange(offset, offset);
    const targetY = headH + caretLineTop(editor, offset) - (editorScroll.clientHeight / 3);
    editorScroll.scrollTo({ top: Math.max(0, targetY), behavior: 'smooth' });
  }

  if (state.mode === 'split') {
    activeScrollSource = editorScroll;
    clearTimeout(activeScrollTimer);
    activeScrollTimer = setTimeout(() => { activeScrollSource = null; }, 700);
    const targetY = headH + caretLineTop(editor, offset) - (editorScroll.clientHeight / 3);
    editorScroll.scrollTo({ top: Math.max(0, targetY), behavior: 'smooth' });
  }

  if (state.mode !== 'edit') {
    const el = preview.querySelector(`[data-line="${line}"]`);
    if (el) {
      const targetTop =
        el.getBoundingClientRect().top -
        previewScroll.getBoundingClientRect().top +
        previewScroll.scrollTop -
        (previewScroll.clientHeight / 3);
      previewScroll.scrollTo({ top: Math.max(0, targetTop), behavior: 'smooth' });
    }
  }
}

/* ---------------- 分栏拖拽 ---------------- */

let dragging = false;

divider.addEventListener('pointerdown', (e) => {
  if (e.button !== 0) return;
  e.preventDefault();
  dragging = true;
  divider.setPointerCapture?.(e.pointerId);
  document.body.classList.add('is-resizing');
});

divider.addEventListener('pointermove', (e) => {
  if (!dragging) return;
  const rect = panes.getBoundingClientRect();
  const ratio = Math.min(0.75, Math.max(0.25, (e.clientX - rect.left) / rect.width));
  state.splitRatio = ratio;
  panes.style.setProperty('--split', `${(ratio * 100).toFixed(2)}%`);
});

function endDrag(e) {
  if (!dragging) return;
  dragging = false;
  try {
    divider.releasePointerCapture?.(e.pointerId);
  } catch {}
  document.body.classList.remove('is-resizing');
  vault.patchWorkspace({ split: state.splitRatio });
}

divider.addEventListener('pointerup', endDrag);
divider.addEventListener('pointercancel', endDrag);
divider.addEventListener('dblclick', () => {
  state.splitRatio = 0.5;
  panes.style.setProperty('--split', '50%');
  vault.patchWorkspace({ split: 0.5 });
});

/* ---------------- 快捷键 ---------------- */

document.addEventListener('keydown', (e) => {
  const mod = e.metaKey || e.ctrlKey;
  const key = e.key.toLowerCase();

  if (mod && (key === 'p' || key === 'o')) {
    e.preventDefault();
    openQuickSwitcher();
    return;
  }

  if (mod && key === 'f') {
    e.preventDefault();
    openFindBar(false);
    return;
  }

  if (mod && key === 'h') {
    e.preventDefault();
    openFindBar(true);
    return;
  }

  if (mod && (e.key === '=' || e.key === '+')) {
    e.preventDefault();
    adjustZoom(1);
    return;
  }

  if (mod && (e.key === '-' || e.key === '_')) {
    e.preventDefault();
    adjustZoom(-1);
    return;
  }

  if (mod && e.key === '0') {
    e.preventDefault();
    resetZoom();
    return;
  }

  if (e.altKey && e.key === 'ArrowLeft') {
    e.preventDefault();
    navigateBack();
    return;
  }

  if (e.altKey && e.key === 'ArrowRight') {
    e.preventDefault();
    navigateForward();
    return;
  }

  if (mod && key === 'n') {
    e.preventDefault();
    if (e.shiftKey) newFolder();
    else newNote();
    return;
  }

  if (mod && key === 's') {
    e.preventDefault();
    commitDoc().then(() => toast('已保存', { icon: 'check' }));
    return;
  }

  if (mod && key === 'k') {
    e.preventDefault();
    search.input.focus();
    search.input.select();
    return;
  }

  if (mod && (key === '\\' || (!e.shiftKey && key === 'b' && document.activeElement !== editor))) {
    e.preventDefault();
    toggleNav();
    return;
  }

  if (mod && e.shiftKey && key === 'b') {
    e.preventDefault();
    toggleList();
    return;
  }

  if (mod && e.shiftKey && key === 'e') {
    e.preventDefault();
    toggleSidebar();
    return;
  }

  if (document.activeElement === editor) {
    if (mod && key === 'b') {
      e.preventDefault();
      wrapSelection('**');
      return;
    }
    if (mod && key === 'i') {
      e.preventDefault();
      wrapSelection('*');
    }
  }
});

/* ---------------- 插件接口 ---------------- */

globalThis.inkdown = {
  version: '0.1.0',

  /** 插件挂一个右侧栏视图：{ id, label, mount(host, ctx) } */
  registerSidebarView(view) {
    const off = registerSidebarView(view);
    const node = h('div', { class: 'ink-view' });
    viewHosts.set(view.id, node);
    sidebar.addView({ id: view.id, label: view.label, node });
    controllers.set(view.id, view.mount(node, sidebarCtx) || {});
    return () => {
      off();
      controllers.get(view.id)?.destroy?.();
      controllers.delete(view.id);
      viewHosts.delete(view.id);
      sidebar.removeView(view.id);
    };
  },

  get vault() {
    return vault;
  },

  toast,
};

/* ---------------- 分栏拖拽 ----------------
   三根缝：导航栏 ↔ 文件列表、文件列表 ↔ 正文、正文 ↔ 右侧栏。
   拖动只改 CSS 变量（宽度都从变量来），松手才写回 workspace。 */

const rootStyle = document.documentElement.style;
const clampPx = (v, min, max) => Math.min(Math.max(v, min), max);

/* 导航栏 */
let navBase = 0;
const navResizer = Resizer({
  axis: 'x',
  label: '调整导航栏宽度',
  className: 'ink-resizer ink-resizer--nav',
  onDelta(dx, phase) {
    if (phase === 'start' || phase === 'key') navBase = side.getBoundingClientRect().width;
    const next = clampPx(navBase + dx, 176, 360);
    app.style.setProperty('--nav-w', `${Math.round(next)}px`);
    if (phase === 'end' || phase === 'key') vault.patchWorkspace({ navWidth: Math.round(next) });
  },
  onReset() {
    app.style.removeProperty('--nav-w');
    vault.patchWorkspace({ navWidth: 0 });
  },
});

/* 文件列表 */
let listBase = 0;
const listResizer = Resizer({
  axis: 'x',
  label: '调整文件列表宽度',
  className: 'ink-resizer ink-resizer--list',
  onDelta(dx, phase) {
    if (phase === 'start' || phase === 'key') listBase = listPane.getBoundingClientRect().width;
    const next = clampPx(listBase + dx, 200, 520);
    rootStyle.setProperty('--ink-list-w', `${Math.round(next)}px`);
    if (phase === 'end' || phase === 'key') vault.patchWorkspace({ listWidth: Math.round(next) });
  },
  onReset() {
    rootStyle.removeProperty('--ink-list-w');
    vault.patchWorkspace({ listWidth: 0 });
  },
});

/* 右侧栏 */
let inspectorBase = 0;
const inspectorResizer = Resizer({
  axis: 'x',
  label: '调整右侧栏宽度',
  className: 'ink-resizer ink-resizer--inspector',
  onDelta(dx, phase) {
    if (phase === 'start' || phase === 'key') inspectorBase = sidebar.getBoundingClientRect().width;
    // 往左拖变宽，所以取负
    const next = clampPx(inspectorBase - dx, 220, 460);
    rootStyle.setProperty('--ink-inspector-w', `${Math.round(next)}px`);
    if (phase === 'end' || phase === 'key') vault.patchWorkspace({ inspectorWidth: Math.round(next) });
  },
  onReset() {
    rootStyle.removeProperty('--ink-inspector-w');
    vault.patchWorkspace({ inspectorWidth: 0 });
  },
});

splitRoot.append(navResizer, listResizer, inspectorResizer);

// 上次会话的栏宽
const savedLayout = vault.workspace;
if (savedLayout.navWidth > 0) app.style.setProperty('--nav-w', `${savedLayout.navWidth}px`);
if (savedLayout.listWidth > 0) rootStyle.setProperty('--ink-list-w', `${savedLayout.listWidth}px`);
if (savedLayout.inspectorWidth > 0) {
  rootStyle.setProperty('--ink-inspector-w', `${savedLayout.inspectorWidth}px`);
}

/* ---------------- 首次渲染 ---------------- */

state.current = pickInitialNote();

if (!state.expanded.size) {
  for (const node of vault.tree) if (node.type === 'dir') state.expanded.add(node.path);
}
if (state.current) expandTo(state.current);
state.selectedDir = state.current ? dirname(state.current) : '';

vault.patchWorkspace({ lastNote: state.current, expanded: [...state.expanded] });

const splitEl = document.querySelector('.ink-split');
splitEl.classList.add('is-booting');

if (state.current) {
  await openNote(state.current);
} else {
  await renderAll();
}
setSidebar(state.sidebar, { persist: false, autoMode: false });
setList(!state.listHidden, { persist: false });
requestAnimationFrame(() => requestAnimationFrame(() => splitEl.classList.remove('is-booting')));

// 首屏画完再建索引：不挡住第一次绘制，建好后关联视图自己会刷新
setTimeout(() => rebuildLinkIndex(), 0);

syncThemeBtn();

if (vault.workspace.navHidden) {
  app.classList.add('l-app--nav-hidden');
  const label = '展开导航栏';
  navToggle.setAttribute('aria-label', label);
  navToggle.title = label;
}

const initialBackdrop = vault.settings.appearance?.backdrop || 'system';
// 先定窗口主题，再下发材质：DWM 按调用当下的窗口主题取色，
// 顺序反了第一帧会是浅色材质，深色界面上一闪一块白。
setWindowTheme(theme.resolved).finally(() => setSidebarBackdrop(initialBackdrop).catch(() => {}));

mountGlassSurfaces();

if (isDesktop) {
  // 顶栏空白处拖窗：按钮和胶囊标了 data-no-drag，不会被误拖
  topbar.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    if (e.target.closest('[data-no-drag]')) return;
    if (e.target.closest('button, a, input, [role="button"]')) return;
    tauriWindow()?.startDragging();
  });

  topbar.addEventListener('dblclick', (e) => {
    if (e.target.closest('[data-no-drag]')) return;
    tauriWindow()?.toggleMaximize();
  });

  const handles = [
    ['n', 'North'],
    ['s', 'South'],
    ['e', 'East'],
    ['w', 'West'],
    ['ne', 'NorthEast'],
    ['nw', 'NorthWest'],
    ['se', 'SouthEast'],
    ['sw', 'SouthWest'],
  ];

  for (const [cls, direction] of handles) {
    const handle = h('div', { class: `ink-resize ink-resize--${cls}` });
    handle.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      tauriWindow()?.startResizeDragging(direction);
    });
    document.body.append(handle);
  }
}

revealOnScroll(listScroll);
revealOnScroll(editorScroll);
revealOnScroll(previewScroll);
revealOnScroll(sidebar.body);
revealOnScroll(side.querySelector('.l-nav__body'));


/* ---------------- 全局上下文菜单（禁用原生浏览器右键，采用 Golden Gate 控件库 openMenu） ---------------- */

function handleGlobalContextMenu(e) {
  const x = e.clientX;
  const y = e.clientY;

  // 1. 文件树上的笔记或文件夹
  const treeRow = e.target.closest('.ink-tree__row');
  if (treeRow && treeRow.dataset.path) {
    const p = treeRow.dataset.path;
    const isFolder = treeRow.classList.contains('ink-tree__row--folder');
    if (isFolder) {
      folderMenu({ path: p, name: p.split('/').pop() }, x, y);
    } else {
      const note = vault.noteAt(p) || { path: p, name: p.split('/').pop() };
      noteMenu(note, x, y);
    }
    return;
  }

  // 2. 侧边栏文件列表空白区域
  if (e.target.closest('.ink-list') || e.target.closest('.ink-sidebar')) {
    openMenu({
      x,
      y,
      items: [
        { header: '文件与仓库' },
        { label: '新建笔记', icon: 'plus', onClick: () => newNote() },
        { label: '新建文件夹', icon: 'folder-plus', onClick: () => newFolder() },
        { sep: true },
        { label: '在文件资源管理器中打开仓库', icon: 'folder-open', onClick: () => openInExplorer('') },
        { label: '复制仓库绝对路径', icon: 'copy', onClick: () => copyVaultPath('') },
        { sep: true },
        { label: '打开本地文件夹…', icon: 'folder-symlink', onClick: () => pickFolder() },
        { label: '重新加载仓库', icon: 'refresh-cw', onClick: () => location.reload() },
      ],
    });
    return;
  }

  // 3. 最近打开卡片项 (Recent items)
  const recentItem = e.target.closest('.ink-recent__item');
  if (recentItem && recentItem.title) {
    const itemPath = recentItem.title;
    const n = vault.noteAt(itemPath);
    openMenu({
      x,
      y,
      items: [
        { header: n?.title || basename(itemPath) },
        { label: '打开笔记', icon: 'file-text', onClick: () => openNote(itemPath) },
        {
          label: '从最近打开中移除',
          icon: 'x',
          onClick: () => {
            state.recents = state.recents.filter((p) => p !== itemPath);
            vault.patchWorkspace({ recents: state.recents });
            renderRecents();
          },
        },
        { sep: true },
        { label: '在文件资源管理器中显示', icon: 'folder-open', onClick: () => openInExplorer(itemPath) },
        { label: '复制文件路径', icon: 'copy', onClick: () => copyVaultPath(itemPath) },
      ],
    });
    return;
  }

  // 4. Markdown 编辑器文本区
  const textarea = e.target.closest('textarea');
  if (textarea) {
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    const hasSelection = start !== end;
    const selectedText = hasSelection ? textarea.value.slice(start, end) : '';

    openMenu({
      x,
      y,
      items: [
        {
          label: '撤销',
          icon: 'undo-2',
          onClick: () => {
            textarea.focus();
            document.execCommand('undo');
          },
        },
        {
          label: '重做',
          icon: 'redo-2',
          onClick: () => {
            textarea.focus();
            document.execCommand('redo');
          },
        },
        { sep: true },
        {
          label: '剪切',
          icon: 'scissors',
          disabled: !hasSelection,
          onClick: async () => {
            if (hasSelection) {
              await navigator.clipboard.writeText(selectedText);
              insertAtCursor('');
              onDocInput();
            }
          },
        },
        {
          label: '复制',
          icon: 'copy',
          disabled: !hasSelection,
          onClick: async () => {
            if (hasSelection) {
              await navigator.clipboard.writeText(selectedText);
              toast('已复制到剪贴板');
            }
          },
        },
        {
          label: '粘贴',
          icon: 'clipboard',
          onClick: async () => {
            try {
              const text = await navigator.clipboard.readText();
              if (text) {
                insertAtCursor(text);
                onDocInput();
              }
            } catch {
              toast('请使用快捷键 Ctrl+V 粘贴', { icon: 'info' });
            }
          },
        },
        {
          label: '全选',
          icon: 'check-square',
          onClick: () => {
            textarea.focus();
            textarea.select();
          },
        },
        { sep: true },
        {
          label: '加粗',
          icon: 'bold',
          onClick: () => wrapSelection('**'),
        },
        {
          label: '斜体',
          icon: 'italic',
          onClick: () => wrapSelection('*'),
        },
        {
          label: '插入图片…',
          icon: 'image',
          onClick: () => imageFileInput.click(),
        },
        { sep: true },
        {
          label: '在资源管理器中定位当前文件',
          icon: 'folder-open',
          onClick: () => openInExplorer(state.current),
        },
      ],
    });
    return;
  }

  // 5. 内容阅读预览区及其他多格式视图
  if (e.target.closest('.ink-preview') || e.target.closest('.ink-view-host') || e.target.closest('#docPane')) {
    const sel = window.getSelection().toString();
    openMenu({
      x,
      y,
      items: [
        ...(sel
          ? [
              {
                label: '复制选中文本',
                icon: 'copy',
                onClick: async () => {
                  await navigator.clipboard.writeText(sel);
                  toast('已复制选中文字');
                },
              },
              { sep: true },
            ]
          : []),
        {
          label: '切换为编辑视图',
          icon: 'square-pen',
          onClick: () => setMode('edit'),
        },
        {
          label: '复制笔记内容',
          icon: 'copy',
          onClick: async () => {
            await navigator.clipboard.writeText(editor.value);
            toast('笔记全文已复制');
          },
        },
        {
          label: '在资源管理器中定位当前文件',
          icon: 'folder-open',
          onClick: () => openInExplorer(state.current),
        },
      ],
    });
    return;
  }

  // 6. 其他通用窗口区域（顶栏、空隙、底栏）
  openMenu({
    x,
    y,
    items: [
      { label: '新建笔记', icon: 'plus', onClick: () => newNote() },
      { label: '新建文件夹', icon: 'folder-plus', onClick: () => newFolder() },
      { sep: true },
      { label: '在文件资源管理器中打开仓库', icon: 'folder-open', onClick: () => openInExplorer('') },
      { label: '复制仓库绝对路径', icon: 'copy', onClick: () => copyVaultPath('') },
      { sep: true },
      { label: '偏好设置…', icon: 'sliders', onClick: () => openSettingsTab() },
      {
        label: theme.mode === 'dark' ? '切换浅色模式' : '切换深色模式',
        icon: theme.mode === 'dark' ? 'sun' : 'moon',
        onClick: () => {
          const next = theme.mode === 'dark' ? 'light' : 'dark';
          theme.setMode(next);
          vault.patchSettings({ appearance: { mode: next, accent: theme.accent } });
          syncThemeBtn();
        },
      },
    ],
  });
}

// 全局彻底拦截 contextmenu，禁用浏览器默认右键，统一切换为 Golden Gate 液态玻璃组件菜单
window.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  handleGlobalContextMenu(e);
});


/* ==========================================================================
   窗口关闭防丢保护 & 最大化状态同步
   ========================================================================== */
window.addEventListener('beforeunload', () => {
  commitDoc();
  vault.writeJson('.inkdown/workspace.json', vault.workspace);
});

if (isDesktop) {
  const win = tauriWindow();
  if (win) {
    const syncMax = async () => {
      try {
        const isMax = await win.isMaximized();
        if (isMax) {
          document.documentElement.dataset.maximized = '1';
        } else {
          delete document.documentElement.dataset.maximized;
        }
      } catch (e) {}
    };
    syncMax();
    win.onResized?.(syncMax);
  }
}
