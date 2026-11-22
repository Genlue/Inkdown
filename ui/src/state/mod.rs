//! 应用全局响应式状态与业务动作编排。

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use glass_design::prelude::*;
use leptos::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::tauri_bridge::{
    copy_to_clipboard, invoke_empty, invoke_empty_unit, is_tauri, listen_tauri_event, reload_page,
    sync_window_theme,
};
use crate::vault::fs::{
    basename, dirname, get_file_ext, is_hidden, join_path, list_vaults, now_ms,
    set_active_vault_id, strip_ext, FileType, VaultFs, VaultMeta,
};
use crate::vault::links::{build_index, resolve_wikilink_path, IndexInput, LinkIndex};
use crate::vault::{
    sanitize_name, NoteInfo, SearchHit, TrashEntry, Vault, VaultCounts, VaultTreeNode, INDEX_PATH,
    TRASH_DIR, TRASH_PATH, WORKSPACE_PATH,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveStatus {
    Saved,
    Saving,
    Dirty,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NavPage {
    Files,
    Favorites,
    Trash,
    Settings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenTab {
    pub path: String,
    pub label: String,
    pub file_type: FileType,
    pub is_markdown: bool,
    pub dirty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputDialogKind {
    NewNote { dir: String },
    NewFolder { dir: String },
    RenameNote { path: String },
    RenameFolder { path: String },
    CreateVault,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputDialogState {
    pub kind: InputDialogKind,
    pub title: String,
    pub description: String,
    pub placeholder: String,
    pub initial: String,
    pub confirm_label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfirmDialogKind {
    DeleteFolder { path: String },
    DeleteForever { trash_path: String },
    EmptyTrash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfirmDialogState {
    pub kind: ConfirmDialogKind,
    pub title: String,
    pub description: String,
    pub confirm_label: String,
}

#[derive(Clone, Debug)]
pub enum ContextMenuKind {
    TreeFile { path: String, is_md: bool, favorite: bool },
    TreeDir { path: String },
    TreeRoot,
    Tab { path: String },
    VaultSwitcher,
    Global,
}

#[derive(Clone, Debug)]
pub struct ContextMenuState {
    pub x: f64,
    pub y: f64,
    pub kind: ContextMenuKind,
}

#[derive(Clone, Copy)]
pub struct AppState {
    pub vault: StoredValue<Rc<RefCell<Option<Vault>>>, leptos::reactive::owner::LocalStorage>,
    pub vault_rev: RwSignal<u64>,
    pub ready: RwSignal<bool>,
    pub load_error: RwSignal<Option<String>>,

    pub link_index: RwSignal<LinkIndex>,
    pub indexing: RwSignal<bool>,

    pub nav_page: RwSignal<NavPage>,
    pub open_tabs: RwSignal<Vec<OpenTab>>,
    pub active_tab: RwSignal<Option<String>>,

    pub editor_path: RwSignal<Option<String>>,
    pub editor_title: RwSignal<String>,
    pub editor_content: RwSignal<String>,
    pub saved_content: RwSignal<String>,
    pub save_status: RwSignal<SaveStatus>,
    pub cursor_pos: RwSignal<(usize, usize)>,
    pub disk_conflict: RwSignal<Option<String>>,
    pub jump_to_line_req: RwSignal<Option<(usize, u64)>>,
    pub active_heading_line: RwSignal<Option<usize>>,

    pub view_mode: RwSignal<String>,
    pub nav_hidden: RwSignal<bool>,
    pub list_hidden: RwSignal<bool>,
    pub inspector_hidden: RwSignal<bool>,
    pub inspector_tab: RwSignal<String>,

    pub sort_mode: RwSignal<String>,
    pub tree_indent: RwSignal<String>,
    pub expanded_dirs: RwSignal<HashSet<String>>,
    pub recents_open: RwSignal<bool>,

    pub search_query: RwSignal<String>,
    pub search_hits: RwSignal<Vec<SearchHit>>,
    pub search_focus_tick: RwSignal<u64>,

    pub find_open: RwSignal<bool>,
    pub find_replace_open: RwSignal<bool>,
    pub find_focus_tick: RwSignal<u64>,

    pub editor_font_size: RwSignal<f32>,
    pub line_width: RwSignal<String>,
    pub auto_continue_lists: RwSignal<bool>,
    pub auto_save: RwSignal<bool>,
    pub ui_font: RwSignal<String>,
    pub md_font: RwSignal<String>,
    pub mono_font: RwSignal<String>,
    pub system_fonts: RwSignal<Vec<String>>,
    pub accent_hex: RwSignal<String>,
    pub glass_level: RwSignal<f32>,

    pub history: RwSignal<Vec<String>>,
    pub history_index: RwSignal<usize>,

    pub window_focused: RwSignal<bool>,
    pub window_maximized: RwSignal<bool>,

    pub quick_open: RwSignal<bool>,
    pub shortcuts_open: RwSignal<bool>,
    pub input_dialog: RwSignal<Option<InputDialogState>>,
    pub confirm_dialog: RwSignal<Option<ConfirmDialogState>>,
    pub context_menu: RwSignal<Option<ContextMenuState>>,

    save_timer: StoredValue<Option<i32>, leptos::reactive::owner::LocalStorage>,
    index_timer: StoredValue<Option<i32>, leptos::reactive::owner::LocalStorage>,
    jump_seq: RwSignal<u64>,
    pub is_desktop: bool,
}

impl AppState {
    pub fn provide(theme: Theme) -> Self {
        let state = Self {
            vault: StoredValue::new_local(Rc::new(RefCell::new(None))),
            vault_rev: RwSignal::new(0),
            ready: RwSignal::new(false),
            load_error: RwSignal::new(None),
            link_index: RwSignal::new(LinkIndex::default()),
            indexing: RwSignal::new(false),
            nav_page: RwSignal::new(NavPage::Files),
            open_tabs: RwSignal::new(Vec::new()),
            active_tab: RwSignal::new(None),
            editor_path: RwSignal::new(None),
            editor_title: RwSignal::new(String::new()),
            editor_content: RwSignal::new(String::new()),
            saved_content: RwSignal::new(String::new()),
            save_status: RwSignal::new(SaveStatus::Saved),
            cursor_pos: RwSignal::new((1, 1)),
            disk_conflict: RwSignal::new(None),
            jump_to_line_req: RwSignal::new(None),
            active_heading_line: RwSignal::new(None),
            view_mode: RwSignal::new("split".to_string()),
            nav_hidden: RwSignal::new(false),
            list_hidden: RwSignal::new(false),
            inspector_hidden: RwSignal::new(false),
            inspector_tab: RwSignal::new("大纲".to_string()),
            sort_mode: RwSignal::new("updated".to_string()),
            tree_indent: RwSignal::new("lines".to_string()),
            expanded_dirs: RwSignal::new(HashSet::new()),
            recents_open: RwSignal::new(false),
            search_query: RwSignal::new(String::new()),
            search_hits: RwSignal::new(Vec::new()),
            search_focus_tick: RwSignal::new(0),
            find_open: RwSignal::new(false),
            find_replace_open: RwSignal::new(false),
            find_focus_tick: RwSignal::new(0),
            editor_font_size: RwSignal::new(14.0),
            line_width: RwSignal::new("standard".to_string()),
            auto_continue_lists: RwSignal::new(true),
            auto_save: RwSignal::new(true),
            ui_font: RwSignal::new(String::new()),
            md_font: RwSignal::new(String::new()),
            mono_font: RwSignal::new(String::new()),
            system_fonts: RwSignal::new(Vec::new()),
            accent_hex: RwSignal::new("#0a84ff".to_string()),
            glass_level: RwSignal::new(1.0),
            history: RwSignal::new(Vec::new()),
            history_index: RwSignal::new(0),
            window_focused: RwSignal::new(true),
            window_maximized: RwSignal::new(false),
            quick_open: RwSignal::new(false),
            shortcuts_open: RwSignal::new(false),
            input_dialog: RwSignal::new(None),
            confirm_dialog: RwSignal::new(None),
            context_menu: RwSignal::new(None),
            save_timer: StoredValue::new_local(None),
            index_timer: StoredValue::new_local(None),
            jump_seq: RwSignal::new(0),
            is_desktop: is_tauri(),
        };
        provide_context(state);
        state.boot(theme);
        state
    }

    pub fn get() -> Self {
        use_context::<Self>().expect("AppState must be provided")
    }

    fn bump(&self) {
        self.vault_rev.update(|v| *v += 1);
    }

    fn boot(self, theme: Theme) {
        let state = self;
        spawn_local(async move {
            match Vault::open_or_create().await {
                Ok(v) => {
                    let s = v.settings.clone();
                    let w = v.workspace.clone();

                    // 初始化主题与外观令牌
                    let mode = match s.appearance.mode.as_str() {
                        "light" => ThemeMode::Light,
                        "dark" => ThemeMode::Dark,
                        _ => ThemeMode::System,
                    };
                    theme.set_mode(mode);
                    let accent_static = leak_accent(&s.appearance.accent);
                    theme.set_accent(accent_static);
                    theme.glass.set(s.appearance.glass.clamp(0.0, 1.0));
                    sync_window_theme(&s.appearance.mode).await;

                    state.accent_hex.set(s.appearance.accent.clone());
                    state.glass_level.set(s.appearance.glass.clamp(0.0, 1.0));
                    state.tree_indent.set(s.appearance.tree_indent.clone());
                    state.view_mode.set(if matches!(w.mode.as_str(), "edit" | "split" | "preview") {
                        w.mode.clone()
                    } else {
                        "split".to_string()
                    });
                    state.nav_hidden.set(w.nav_hidden);
                    state.list_hidden.set(w.list_hidden);
                    state.inspector_hidden.set(!w.sidebar);
                    state.inspector_tab.set(match w.view.as_str() {
                        "relations" | "关联" => "关联".to_string(),
                        "info" | "信息" => "信息".to_string(),
                        _ => "大纲".to_string(),
                    });
                    state.sort_mode.set(v.sort.clone());
                    state.expanded_dirs.set(w.expanded.iter().cloned().collect());
                    state.recents_open.set(w.recents_open);
                    state.editor_font_size.set(w.font_size.clamp(11.0, 26.0));
                    state.line_width.set(s.editor.line_width.clone());
                    state.auto_continue_lists.set(s.editor.auto_continue_lists);
                    state.auto_save.set(s.editor.auto_save);
                    state.ui_font.set(s.fonts.ui.clone());
                    state.md_font.set(s.fonts.markdown.clone());
                    state.mono_font.set(s.fonts.mono.clone());

                    apply_typography_tokens(&theme, &s, w.font_size);

                    // 恢复打开的标签页
                    let mut initial_tabs = Vec::new();
                    for tab_path in &w.open_tabs {
                        if let Some(info) = v.note_at(tab_path) {
                            initial_tabs.push(OpenTab {
                                path: info.path,
                                label: info.title,
                                file_type: info.file_type,
                                is_markdown: info.is_markdown,
                                dirty: false,
                            });
                        }
                    }
                    let target_open = if !w.last_note.is_empty() && v.note_at(&w.last_note).is_some() {
                        Some(w.last_note.clone())
                    } else if let Some(first_tab) = initial_tabs.first() {
                        Some(first_tab.path.clone())
                    } else {
                        v.flat_notes().first().map(|n| n.path.clone())
                    };

                    // 自动展开上次笔记的祖先目录
                    if let Some(ref p) = target_open {
                        let mut dirs = state.expanded_dirs.get_untracked();
                        let mut cur = dirname(p);
                        while !cur.is_empty() {
                            dirs.insert(cur.clone());
                            cur = dirname(&cur);
                        }
                        state.expanded_dirs.set(dirs);
                    }

                    // 异步加载字体列表
                    let fs_clone = v.fs.clone();
                    spawn_local(async move {
                        let fonts = fs_clone.system_fonts().await;
                        if !fonts.is_empty() {
                            state.system_fonts.set(fonts);
                        }
                    });

                    // 放入状态容器
                    let is_desktop = state.is_desktop;
                    state.vault.with_value(|cell| {
                        *cell.borrow_mut() = Some(v);
                    });
                    state.open_tabs.set(initial_tabs);
                    state.ready.set(true);
                    state.bump();

                    if let Some(p) = target_open {
                        state.open_file(&p, true).await;
                    }

                    state.rebuild_link_index().await;

                    if is_desktop {
                        state.install_desktop_listeners();
                    }
                }
                Err(err) => {
                    state.load_error.set(Some(err));
                    state.ready.set(true);
                }
            }
        });
    }

    fn install_desktop_listeners(self) {
        let state = self;
        listen_tauri_event("vault://fs-change", move |payload| {
            let paths: Vec<String> = payload
                .get("paths")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            spawn_local(async move {
                state.handle_external_fs_change(paths).await;
            });
        });

        spawn_local(async move {
            if let Ok(max) = invoke_empty::<bool>("window_is_maximized").await {
                state.window_maximized.set(max);
            }
        });
    }

    pub async fn handle_external_fs_change(self, paths: Vec<String>) {
        let current = self.editor_path.get_untracked();
        let mut need_refresh = false;

        let active_changed = current
            .as_ref()
            .map(|c| paths.is_empty() || paths.iter().any(|p| p == c))
            .unwrap_or(false);

        if !paths.is_empty() || active_changed {
            need_refresh = true;
        }

        if need_refresh {
            let mut reloaded = false;
            if let Some(ref cur) = current {
                if active_changed {
                    let disk_text = match self.vault_fs() {
                        Some(fs) => fs.read(cur).await.ok().map(|v| v.unwrap_or_default()),
                        None => None,
                    };
                    if let Some(text) = disk_text {
                        self.with_vault_mut(|v| v.remember_note(cur, &text));
                        if text != self.saved_content.get_untracked() {
                            let is_dirty = self.is_dirty();
                            if is_dirty {
                                self.disk_conflict.set(Some(text));
                            } else {
                                self.editor_content.set(text.clone());
                                self.saved_content.set(text);
                                self.save_status.set(SaveStatus::Saved);
                                reloaded = true;
                            }
                        }
                    }
                }
            }
            self.rescan_vault(false).await;
            if reloaded {
                toast("外部修改已同步到编辑器");
            }
        }
    }

    pub async fn rescan_vault(self, announce: bool) {
        if self.rescan().await {
            self.rebuild_link_index().await;
            if announce {
                toast("已刷新仓库文件列表");
            }
        }
    }

    pub async fn rebuild_link_index(self) {
        self.indexing.set(true);
        let Some((fs, notes)) = self.with_vault(|v| (v.fs.clone(), v.flat_notes())) else {
            self.indexing.set(false);
            return;
        };
        let mut inputs = Vec::with_capacity(notes.len());
        for note in notes {
            let cached = self
                .with_vault(|v| v.cached_text(&note.path).cloned())
                .flatten();
            let content = match cached {
                Some(text) => text,
                None => {
                    // 这里绝不能借仓库：读盘要走手上这份 fs 句柄
                    let text = match fs.read(&note.path).await {
                        Ok(Some(t)) => t,
                        _ => String::new(),
                    };
                    self.with_vault_mut(|v| v.remember_note(&note.path, &text));
                    text
                }
            };
            inputs.push(IndexInput {
                path: note.path,
                title: note.title,
                content,
            });
        }
        let idx = build_index(&inputs);
        self.link_index.set(idx);
        self.indexing.set(false);
    }

    fn schedule_index_rebuild(self) {
        if let Some(id) = self.index_timer.get_value() {
            if let Some(w) = web_sys::window() {
                w.clear_timeout_with_handle(id);
            }
        }
        let state = self;
        let cb = Closure::once_into_js(move || {
            spawn_local(async move {
                state.rebuild_link_index().await;
            });
        });
        if let Some(w) = web_sys::window() {
            if let Ok(id) = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.unchecked_ref(),
                450,
            ) {
                self.index_timer.set_value(Some(id));
            }
        }
    }

    /* ---------------- 读仓库派生数据 ---------------- */

    pub fn vault_name(&self) -> String {
        let _ = self.vault_rev.get();
        self.vault.with_value(|cell| {
            cell.borrow()
                .as_ref()
                .map(|v| v.meta.name.clone())
                .unwrap_or_else(|| "Inkdown".to_string())
        })
    }

    pub fn vault_root_path(&self) -> Option<String> {
        let _ = self.vault_rev.get();
        self.vault.with_value(|cell| {
            cell.borrow()
                .as_ref()
                .and_then(|v| v.fs.root_path().map(str::to_string))
        })
    }

    pub fn counts(&self) -> VaultCounts {
        let _ = self.vault_rev.get();
        self.vault.with_value(|cell| {
            cell.borrow()
                .as_ref()
                .map(|v| v.counts())
                .unwrap_or(VaultCounts {
                    all: 0,
                    favorites: 0,
                    trash: 0,
                })
        })
    }

    pub fn tree(&self) -> Vec<VaultTreeNode> {
        let _ = self.vault_rev.get();
        self.vault.with_value(|cell| {
            cell.borrow()
                .as_ref()
                .map(|v| v.tree.clone())
                .unwrap_or_default()
        })
    }

    pub fn flat_notes(&self) -> Vec<NoteInfo> {
        let _ = self.vault_rev.get();
        self.vault.with_value(|cell| {
            cell.borrow()
                .as_ref()
                .map(|v| v.flat_notes())
                .unwrap_or_default()
        })
    }

    pub fn favorites(&self) -> Vec<NoteInfo> {
        let _ = self.vault_rev.get();
        self.vault.with_value(|cell| {
            cell.borrow()
                .as_ref()
                .map(|v| v.favorites())
                .unwrap_or_default()
        })
    }

    pub fn trash(&self) -> Vec<TrashEntry> {
        let _ = self.vault_rev.get();
        self.vault.with_value(|cell| {
            cell.borrow()
                .as_ref()
                .map(|v| v.trash.clone())
                .unwrap_or_default()
        })
    }

    pub fn recents(&self) -> Vec<NoteInfo> {
        let _ = self.vault_rev.get();
        self.vault.with_value(|cell| {
            let guard = cell.borrow();
            let Some(v) = guard.as_ref() else {
                return Vec::new();
            };
            v.workspace
                .recents
                .iter()
                .filter_map(|p| v.note_at(p))
                .take(5)
                .collect()
        })
    }

    pub fn note_at(&self, path: &str) -> Option<NoteInfo> {
        let _ = self.vault_rev.get();
        self.vault
            .with_value(|cell| cell.borrow().as_ref().and_then(|v| v.note_at(path)))
    }

    pub fn is_dirty(&self) -> bool {
        self.editor_path.get_untracked().is_some()
            && self.editor_content.get_untracked() != self.saved_content.get_untracked()
    }

    /* ---------------- 仓库访问：借用绝不能跨 await ----------------
       整个应用只有一个线程，`.await` 期间渲染随时会读仓库。
       只要有一个 `borrow_mut()` 活过了 await 点，读仓库就是
       `RefCell already mutably borrowed` —— wasm 里 panic 等于 trap，
       界面从此不再响应。所以下面三个助手都只做同步的事：
       异步 I/O 一律先 `vault_fs()` 拿句柄，借用在 await 之前就已经结束。
    */

    /// 只读一次仓库。
    fn with_vault<R>(&self, f: impl FnOnce(&Vault) -> R) -> Option<R> {
        self.vault.with_value(|cell| cell.borrow().as_ref().map(f))
    }

    /// 写一次仓库（同步）。
    fn with_vault_mut<R>(&self, f: impl FnOnce(&mut Vault) -> R) -> Option<R> {
        self.vault.with_value(|cell| {
            let mut guard = cell.borrow_mut();
            guard.as_mut().map(f)
        })
    }

    /// 拿一份文件句柄；拿到之后随便 await，都不会再持有借用。
    fn vault_fs(&self) -> Option<VaultFs> {
        self.with_vault(|v| v.fs.clone())
    }

    /// 重新遍历仓库并刷新内存视图。
    async fn rescan(self) -> bool {
        let Some(fs) = self.vault_fs() else {
            return false;
        };
        let Ok(entries) = fs.walk().await else {
            return false;
        };
        self.with_vault_mut(|v| v.apply_entries(entries));
        self.bump();
        true
    }

    /* ---------------- 文档打开 / 切换 / 保存 ---------------- */

    pub async fn commit_doc(self) {
        if let Some(id) = self.save_timer.get_value() {
            if let Some(w) = web_sys::window() {
                w.clear_timeout_with_handle(id);
            }
            self.save_timer.set_value(None);
        }
        let Some(path) = self.editor_path.get_untracked() else {
            return;
        };
        let content = self.editor_content.get_untracked();
        if content == self.saved_content.get_untracked() {
            return;
        }
        self.save_status.set(SaveStatus::Saving);
        let Some(fs) = self.vault_fs() else {
            self.save_status.set(SaveStatus::Dirty);
            toast("保存失败：仓库未就绪");
            return;
        };
        let mtime = now_ms();
        let res = fs.write(&path, &content, Some(mtime)).await;
        match res {
            Ok(()) => {
                self.with_vault_mut(|v| v.mark_saved(&path, &content, mtime));
                self.saved_content.set(content);
                self.save_status.set(SaveStatus::Saved);
                self.mark_tab_dirty(&path, false);
                self.bump();
                self.schedule_index_rebuild();
            }
            Err(err) => {
                self.save_status.set(SaveStatus::Dirty);
                toast(format!("保存失败：{err}"));
            }
        }
    }

    pub fn on_editor_input(self, next: String) {
        self.editor_content.set(next.clone());
        let dirty = next != self.saved_content.get_untracked();
        if let Some(path) = self.editor_path.get_untracked() {
            self.mark_tab_dirty(&path, dirty);
        }
        if !dirty {
            self.save_status.set(SaveStatus::Saved);
            return;
        }
        self.save_status.set(SaveStatus::Dirty);
        if self.auto_save.get_untracked() {
            if let Some(id) = self.save_timer.get_value() {
                if let Some(w) = web_sys::window() {
                    w.clear_timeout_with_handle(id);
                }
            }
            let state = self;
            let cb = Closure::once_into_js(move || {
                spawn_local(async move {
                    state.commit_doc().await;
                });
            });
            if let Some(w) = web_sys::window() {
                if let Ok(id) = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                    cb.unchecked_ref(),
                    500,
                ) {
                    self.save_timer.set_value(Some(id));
                }
            }
        }
        self.schedule_index_rebuild();
    }

    pub fn mark_tab_dirty(&self, path: &str, dirty: bool) {
        self.open_tabs.update(|tabs| {
            for t in tabs {
                if t.path == path {
                    t.dirty = dirty;
                }
            }
        });
    }

    pub async fn open_file(self, path: &str, record_history: bool) {
        if path.is_empty() {
            return;
        }
        self.commit_doc().await;
        if matches!(self.nav_page.get_untracked(), NavPage::Settings) {
            self.nav_page.set(NavPage::Files);
        }

        let info = {
            let cell = self.vault.get_value();
            let guard = cell.borrow();
            guard.as_ref().and_then(|v| v.note_at(path))
        };
        let Some(info) = info else {
            return;
        };

        // 确保进入标签页
        self.open_tabs.update(|tabs| {
            if !tabs.iter().any(|t| t.path == info.path) {
                tabs.push(OpenTab {
                    path: info.path.clone(),
                    label: info.title.clone(),
                    file_type: info.file_type,
                    is_markdown: info.is_markdown,
                    dirty: false,
                });
            }
        });
        self.active_tab.set(Some(info.path.clone()));
        self.disk_conflict.set(None);

        // 自动展开父目录
        let mut dirs = self.expanded_dirs.get_untracked();
        let mut cur = dirname(&info.path);
        let mut changed_dirs = false;
        while !cur.is_empty() {
            if dirs.insert(cur.clone()) {
                changed_dirs = true;
            }
            cur = dirname(&cur);
        }
        if changed_dirs {
            self.expanded_dirs.set(dirs.clone());
        }

        if info.is_markdown {
            let cached = self
                .with_vault(|v| v.cached_text(&info.path).cloned())
                .flatten();
            let content = match cached {
                Some(text) => text,
                None => {
                    let text = match self.vault_fs() {
                        Some(fs) => match fs.read(&info.path).await {
                            Ok(Some(t)) => t,
                            _ => String::new(),
                        },
                        None => String::new(),
                    };
                    self.with_vault_mut(|v| v.remember_note(&info.path, &text));
                    text
                }
            };
            self.editor_path.set(Some(info.path.clone()));
            self.editor_title.set(info.title.clone());
            self.editor_content.set(content.clone());
            self.saved_content.set(content);
            self.save_status.set(SaveStatus::Saved);
            self.cursor_pos.set((1, 1));
            self.active_heading_line.set(None);
        } else {
            self.editor_path.set(None);
        }

        // 更新最近打开与历史
        if record_history {
            let mut hist = self.history.get_untracked();
            let idx = self.history_index.get_untracked();
            if hist.get(idx).map(|s| s.as_str()) != Some(&info.path) {
                if !hist.is_empty() && idx + 1 < hist.len() {
                    hist.truncate(idx + 1);
                }
                hist.push(info.path.clone());
                if hist.len() > 50 {
                    hist.remove(0);
                }
                let next_idx = hist.len().saturating_sub(1);
                self.history.set(hist);
                self.history_index.set(next_idx);
            }
        }

        let tabs_paths: Vec<String> = self
            .open_tabs
            .get_untracked()
            .into_iter()
            .map(|t| t.path)
            .collect();
        let expanded_vec: Vec<String> = self.expanded_dirs.get_untracked().into_iter().collect();

        {
            let ws_json = self.with_vault_mut(|v| {
                v.workspace.last_note = info.path.clone();
                v.workspace.open_tabs = tabs_paths;
                v.workspace.expanded = expanded_vec;
                v.workspace.recents.retain(|p| p != &info.path);
                v.workspace.recents.insert(0, info.path.clone());
                if v.workspace.recents.len() > 10 {
                    v.workspace.recents.truncate(10);
                }
                v.workspace_json()
            });
            if let (Some(fs), Some(json)) = (self.vault_fs(), ws_json) {
                spawn_local(async move {
                    let _ = fs.write(WORKSPACE_PATH, &json, None).await;
                });
            }
        }
        self.bump();
    }

    pub async fn close_tab(self, path: &str) {
        if self.editor_path.get_untracked().as_deref() == Some(path) {
            self.commit_doc().await;
        }
        let mut next_path = None;
        self.open_tabs.update(|tabs| {
            if let Some(pos) = tabs.iter().position(|t| t.path == path) {
                tabs.remove(pos);
                if self.active_tab.get_untracked().as_deref() == Some(path) {
                    next_path = tabs
                        .get(pos)
                        .or_else(|| pos.checked_sub(1).and_then(|i| tabs.get(i)))
                        .map(|t| t.path.clone());
                }
            }
        });
        if self.active_tab.get_untracked().as_deref() == Some(path) {
            match next_path {
                Some(np) => self.open_file(&np, true).await,
                None => {
                    self.active_tab.set(None);
                    self.editor_path.set(None);
                    self.save_workspace_tabs();
                }
            }
        } else {
            self.save_workspace_tabs();
        }
    }

    pub async fn close_other_tabs(self, keep: &str) {
        self.commit_doc().await;
        self.open_tabs.update(|tabs| {
            tabs.retain(|t| t.path == keep);
        });
        if self.active_tab.get_untracked().as_deref() != Some(keep) {
            self.open_file(keep, true).await;
        } else {
            self.save_workspace_tabs();
        }
    }

    pub fn reorder_tabs(&self, ordered_paths: Vec<String>) {
        self.open_tabs.update(|tabs| {
            let mut next = Vec::with_capacity(tabs.len());
            for p in &ordered_paths {
                if let Some(t) = tabs.iter().find(|t| &t.path == p) {
                    next.push(t.clone());
                }
            }
            for t in tabs.iter() {
                if !next.iter().any(|x| x.path == t.path) {
                    next.push(t.clone());
                }
            }
            *tabs = next;
        });
        self.save_workspace_tabs();
    }

    fn save_workspace_tabs(&self) {
        let paths: Vec<String> = self
            .open_tabs
            .get_untracked()
            .into_iter()
            .map(|t| t.path)
            .collect();
        let last = self.active_tab.get_untracked().unwrap_or_default();
        self.vault.with_value(|cell| {
            if let Some(v) = cell.borrow_mut().as_mut() {
                v.workspace.open_tabs = paths;
                v.workspace.last_note = last;
                let w = v.workspace.clone();
                let fs = v.fs.clone();
                spawn_local(async move {
                    if let Ok(json) = serde_json::to_string_pretty(&w) {
                        let _ = fs.write(".inkdown/workspace.json", &json, None).await;
                    }
                });
            }
        });
    }

    pub fn can_go_back(&self) -> bool {
        self.history_index.get() > 0
    }

    pub fn can_go_forward(&self) -> bool {
        let len = self.history.get().len();
        len > 0 && self.history_index.get() + 1 < len
    }

    pub async fn navigate_back(self) {
        let idx = self.history_index.get_untracked();
        if idx == 0 {
            return;
        }
        let next_idx = idx - 1;
        if let Some(path) = self.history.get_untracked().get(next_idx).cloned() {
            self.history_index.set(next_idx);
            self.open_file(&path, false).await;
        }
    }

    pub async fn navigate_forward(self) {
        let idx = self.history_index.get_untracked();
        let hist = self.history.get_untracked();
        if idx + 1 >= hist.len() {
            return;
        }
        let next_idx = idx + 1;
        if let Some(path) = hist.get(next_idx).cloned() {
            self.history_index.set(next_idx);
            self.open_file(&path, false).await;
        }
    }

    /* ---------------- 笔记 / 文件夹 CRUD ----------------
       每个动作都拆成三段：同步读内存 → 借用外的 I/O → 同步写回内存。
       顺序和以前保持一致，只是中间不再握着借用去 await。
    */

    pub async fn create_note(self, dir: String, title: String) {
        self.commit_doc().await;
        let Some(fs) = self.vault_fs() else {
            toast("仓库未就绪");
            return;
        };
        let Some(path) = self.with_vault(|v| v.plan_new_note(&dir, &title)) else {
            return;
        };
        if let Err(err) = fs.write(&path, "", None).await {
            toast(format!("新建失败：{err}"));
            return;
        }
        let index_json = self.with_vault_mut(|v| v.index_register(&path));
        if let Some(json) = index_json {
            let _ = fs.write(INDEX_PATH, &json, None).await;
        }
        self.rescan().await;
        self.open_file(&path, true).await;
        self.rebuild_link_index().await;
        toast("已新建笔记");
    }

    pub async fn create_folder(self, dir: String, name: String) {
        let Some(fs) = self.vault_fs() else {
            toast("仓库未就绪");
            return;
        };
        let Some(path) = self.with_vault(|v| v.plan_new_folder(&dir, &name)) else {
            return;
        };
        if let Err(err) = fs.mkdir(&path).await {
            toast(format!("新建文件夹失败：{err}"));
            return;
        }
        self.rescan().await;
        self.expanded_dirs.update(|set| {
            if !dir.is_empty() {
                set.insert(dir.clone());
            }
            set.insert(path);
        });
        self.persist_expanded();
        toast("已新建文件夹");
    }

    pub async fn rename_note_to(self, path: String, next_title: String) {
        if self.editor_path.get_untracked().as_deref() == Some(&path) {
            self.commit_doc().await;
        }
        let next_path = match self.with_vault(|v| v.plan_rename_note(&path, &next_title)) {
            Some(Ok(Some(next))) => next,
            Some(Ok(None)) => return,
            Some(Err(err)) => {
                toast(format!("重命名失败：{err}"));
                return;
            }
            None => return,
        };
        let Some(fs) = self.vault_fs() else {
            toast("仓库未就绪");
            return;
        };
        if let Err(err) = fs.rename(&path, &next_path).await {
            toast(format!("重命名失败：{err}"));
            return;
        }
        let index_json = self.with_vault_mut(|v| v.migrate_note_path(&path, &next_path));
        if let Some(json) = index_json {
            let _ = fs.write(INDEX_PATH, &json, None).await;
        }
        self.persist_workspace_via(&fs).await;

        let new_label = strip_ext(&basename(&next_path));
        self.open_tabs.update(|tabs| {
            for t in tabs {
                if t.path == path {
                    t.path = next_path.clone();
                    t.label = new_label.clone();
                }
            }
        });
        if self.active_tab.get_untracked().as_deref() == Some(&path) {
            self.active_tab.set(Some(next_path.clone()));
        }
        if self.editor_path.get_untracked().as_deref() == Some(&path) {
            self.editor_path.set(Some(next_path.clone()));
            self.editor_title.set(new_label);
        }
        self.history.update(|h| {
            for p in h {
                if p == &path {
                    *p = next_path.clone();
                }
            }
        });
        self.bump();
        self.rebuild_link_index().await;
    }

    pub async fn rename_folder_to(self, path: String, next_name: String) {
        self.commit_doc().await;
        let next_path = match self.with_vault(|v| v.plan_rename_folder(&path, &next_name)) {
            Some(Ok(Some(next))) => next,
            Some(Ok(None)) => return,
            Some(Err(err)) => {
                toast(format!("重命名失败：{err}"));
                return;
            }
            None => return,
        };
        let Some(fs) = self.vault_fs() else {
            toast("仓库未就绪");
            return;
        };
        if let Err(err) = fs.rename(&path, &next_path).await {
            toast(format!("重命名失败：{err}"));
            return;
        }
        let index_json = self.with_vault_mut(|v| v.migrate_dir_prefix(&path, &next_path));
        if let Some(json) = index_json {
            let _ = fs.write(INDEX_PATH, &json, None).await;
        }
        self.persist_workspace_via(&fs).await;
        self.rewrite_open_prefix(&path, &next_path);
        self.rescan().await;
        self.rebuild_link_index().await;
        toast("已重命名文件夹");
    }

    pub async fn move_item(self, src_path: String, target_dir: String) {
        self.commit_doc().await;
        let plan = match self.with_vault(|v| v.plan_move(&src_path, &target_dir)) {
            Some(Ok(Some(plan))) => plan,
            Some(Ok(None)) => return,
            Some(Err(err)) => {
                toast(err);
                return;
            }
            None => return,
        };
        let (next_path, is_dir) = plan;
        let Some(fs) = self.vault_fs() else {
            toast("仓库未就绪");
            return;
        };
        if let Err(err) = fs.rename(&src_path, &next_path).await {
            toast(err);
            return;
        }
        let index_json = self.with_vault_mut(|v| {
            if is_dir {
                v.migrate_dir_prefix(&src_path, &next_path)
            } else {
                v.migrate_note_path(&src_path, &next_path)
            }
        });
        if let Some(json) = index_json {
            let _ = fs.write(INDEX_PATH, &json, None).await;
        }
        self.persist_workspace_via(&fs).await;
        self.rewrite_open_prefix(&src_path, &next_path);
        if !target_dir.is_empty() {
            self.expanded_dirs.update(|s| {
                s.insert(target_dir);
            });
            self.persist_expanded();
        }
        self.rescan().await;
        self.rebuild_link_index().await;
        toast("已移动");
    }

    fn rewrite_open_prefix(&self, from: &str, to: &str) {
        let prefix = format!("{from}/");
        let map_path = |p: &mut String| {
            if *p == from || p.starts_with(&prefix) {
                *p = format!("{to}{}", &p[from.len()..]);
            }
        };
        self.open_tabs.update(|tabs| {
            for t in tabs {
                map_path(&mut t.path);
            }
        });
        if let Some(mut cur) = self.active_tab.get_untracked() {
            map_path(&mut cur);
            self.active_tab.set(Some(cur));
        }
        if let Some(mut cur) = self.editor_path.get_untracked() {
            map_path(&mut cur);
            self.editor_path.set(Some(cur));
        }
        self.history.update(|h| {
            for p in h {
                map_path(p);
            }
        });
    }

    pub async fn toggle_favorite(self, path: String) {
        let Some(fs) = self.vault_fs() else {
            toast("仓库未就绪");
            return;
        };
        let Some((fav, json)) = self.with_vault_mut(|v| {
            let fav = v.flip_favorite(&path);
            (fav, v.index_json())
        }) else {
            return;
        };
        if let Err(err) = fs.write(INDEX_PATH, &json, None).await {
            toast(err);
            return;
        }
        self.bump();
        toast(if fav { "已添加到收藏" } else { "已取消收藏" });
    }

    pub async fn trash_note(self, path: String) {
        if self.editor_path.get_untracked().as_deref() == Some(&path) {
            self.commit_doc().await;
        }
        let Some(fs) = self.vault_fs() else {
            toast("仓库未就绪");
            return;
        };
        let trash_path = join_path(TRASH_DIR, &format!("{}-{}", now_ms(), basename(&path)));
        if let Err(err) = fs.rename(&path, &trash_path).await {
            toast(format!("删除失败：{err}"));
            return;
        }
        let trash_json = self.with_vault_mut(|v| {
            v.push_trash(&trash_path, &path);
            v.trash_json()
        });
        if let Some(json) = trash_json {
            let _ = fs.write(TRASH_PATH, &json, None).await;
        }
        self.persist_workspace_via(&fs).await;
        self.close_tab(&path).await;
        self.rescan().await;
        self.rebuild_link_index().await;
        toast("已移到回收站");
    }

    pub async fn duplicate_note(self, path: String) {
        self.commit_doc().await;
        let Some(fs) = self.vault_fs() else {
            toast("仓库未就绪");
            return;
        };
        let dir = dirname(&path);
        let base = format!("{} 副本", strip_ext(&basename(&path)));
        let Some(new_path) = self.with_vault(|v| v.plan_new_note(&dir, &base)) else {
            return;
        };
        let cached = self.with_vault(|v| v.cached_text(&path).cloned()).flatten();
        let content = match cached {
            Some(text) => text,
            None => match fs.read(&path).await {
                Ok(Some(text)) => text,
                Ok(None) => String::new(),
                Err(err) => {
                    toast(format!("复制失败：{err}"));
                    return;
                }
            },
        };
        if let Err(err) = fs.write(&new_path, &content, None).await {
            toast(format!("复制失败：{err}"));
            return;
        }
        let index_json = self.with_vault_mut(|v| v.index_register(&new_path));
        if let Some(json) = index_json {
            let _ = fs.write(INDEX_PATH, &json, None).await;
        }
        self.rescan().await;
        self.open_file(&new_path, true).await;
        self.rebuild_link_index().await;
        toast("已创建副本");
    }

    pub async fn delete_folder(self, path: String) {
        self.commit_doc().await;
        let Some(fs) = self.vault_fs() else {
            toast("仓库未就绪");
            return;
        };
        if let Err(err) = fs.remove(&path).await {
            toast(format!("删除失败：{err}"));
            return;
        }
        let payload = self.with_vault_mut(|v| (v.forget_path(&path), v.workspace_json()));
        if let Some((index_json, ws_json)) = payload {
            let _ = fs.write(INDEX_PATH, &index_json, None).await;
            let _ = fs.write(WORKSPACE_PATH, &ws_json, None).await;
        }
        let prefix = format!("{path}/");
        self.open_tabs.update(|tabs| {
            tabs.retain(|t| t.path != path && !t.path.starts_with(&prefix));
        });
        if let Some(cur) = self.active_tab.get_untracked() {
            if cur == path || cur.starts_with(&prefix) {
                self.active_tab.set(None);
                self.editor_path.set(None);
            }
        }
        self.rescan().await;
        self.rebuild_link_index().await;
        toast("已删除文件夹");
    }

    pub async fn restore_trash(self, trash_path: String) {
        let Some(fs) = self.vault_fs() else {
            toast("仓库未就绪");
            return;
        };
        let Some(entry) = self
            .with_vault(|v| v.trash_entry(&trash_path))
            .flatten()
        else {
            toast("回收站里找不到这篇笔记");
            return;
        };

        let dir = dirname(&entry.original_path);
        if !dir.is_empty() {
            if let Err(err) = fs.mkdir(&dir).await {
                toast(err);
                return;
            }
        }

        let mut target = entry.original_path.clone();
        match fs.exists(&target).await {
            Ok(true) => {
                let ext = get_file_ext(&target);
                let dot_ext = if ext.is_empty() {
                    ".md".to_string()
                } else {
                    format!(".{ext}")
                };
                let stem = strip_ext(&basename(&target));
                if let Some(unique) = self.with_vault(|v| v.unique_name(&dir, &stem, &dot_ext)) {
                    target = join_path(&dir, &format!("{unique}{dot_ext}"));
                }
            }
            Ok(false) => {}
            Err(err) => {
                toast(err);
                return;
            }
        }

        if let Err(err) = fs.rename(&trash_path, &target).await {
            toast(err);
            return;
        }
        let trash_json = self.with_vault_mut(|v| {
            v.drop_trash(&trash_path);
            if let Some(cached) = v.cache.remove(&trash_path) {
                v.cache.insert(target.clone(), cached);
            }
            v.trash_json()
        });
        if let Some(json) = trash_json {
            let _ = fs.write(TRASH_PATH, &json, None).await;
        }
        self.rescan().await;
        self.open_file(&target, true).await;
        self.rebuild_link_index().await;
        toast("已恢复笔记");
    }

    pub async fn delete_forever(self, trash_path: String) {
        let Some(fs) = self.vault_fs() else {
            toast("仓库未就绪");
            return;
        };
        if let Err(err) = fs.remove(&trash_path).await {
            toast(err);
            return;
        }
        let entry = self
            .with_vault(|v| v.trash_entry(&trash_path))
            .flatten();
        let payload = self.with_vault_mut(|v| {
            if let Some(e) = &entry {
                v.index.notes.remove(&e.original_path);
            }
            v.drop_trash(&trash_path);
            v.cache.remove(&trash_path);
            (v.index_json(), v.trash_json())
        });
        if let Some((index_json, trash_json)) = payload {
            let _ = fs.write(INDEX_PATH, &index_json, None).await;
            let _ = fs.write(TRASH_PATH, &trash_json, None).await;
        }
        self.rescan().await;
        toast("已永久删除");
    }

    pub async fn empty_trash(self) {
        let Some(fs) = self.vault_fs() else {
            toast("仓库未就绪");
            return;
        };
        let items = self.with_vault_mut(|v| v.drain_trash()).unwrap_or_default();
        for item in &items {
            let _ = fs.remove(&item.path).await;
        }
        let payload = self.with_vault_mut(|v| (v.index_json(), v.trash_json()));
        if let Some((index_json, trash_json)) = payload {
            let _ = fs.write(INDEX_PATH, &index_json, None).await;
            let _ = fs.write(TRASH_PATH, &trash_json, None).await;
        }
        self.rescan().await;
        toast("已清空回收站");
    }

    pub async fn open_wikilink(self, target: String) {
        let clean_target = target.split('#').next().unwrap_or(&target).trim().to_string();
        if clean_target.is_empty() {
            return;
        }
        let hit = resolve_wikilink_path(&self.link_index.get_untracked(), &clean_target);
        if let Some(path) = hit {
            self.open_file(&path, true).await;
            return;
        }
        // 不存在则就地创建并跳转
        let dir = self
            .editor_path
            .get_untracked()
            .map(|p| dirname(&p))
            .unwrap_or_default();
        self.create_note(dir, clean_target).await;
    }

    /* ---------------- 界面与偏好控制 ---------------- */

    pub fn set_view_mode(&self, mode: String) {
        if !matches!(mode.as_str(), "edit" | "split" | "preview") {
            return;
        }
        self.view_mode.set(mode.clone());
        self.patch_workspace(|w| w.mode = mode);
    }

    pub fn toggle_nav(&self) {
        let next = !self.nav_hidden.get_untracked();
        self.nav_hidden.set(next);
        self.patch_workspace(|w| w.nav_hidden = next);
    }

    pub fn toggle_list(&self) {
        let next = !self.list_hidden.get_untracked();
        self.list_hidden.set(next);
        self.patch_workspace(|w| w.list_hidden = next);
    }

    pub fn toggle_inspector(&self) {
        let next = !self.inspector_hidden.get_untracked();
        self.inspector_hidden.set(next);
        self.patch_workspace(|w| w.sidebar = !next);
    }

    pub fn set_inspector_tab(&self, tab: String) {
        self.inspector_tab.set(tab.clone());
        let key = match tab.as_str() {
            "关联" => "relations",
            "信息" => "info",
            _ => "outline",
        };
        self.patch_workspace(|w| w.view = key.to_string());
    }

    pub fn toggle_dir(&self, path: &str) {
        self.expanded_dirs.update(|set| {
            if !set.remove(path) {
                set.insert(path.to_string());
            }
        });
        self.persist_expanded();
    }

    pub fn fold_or_expand_all(&self) {
        let all_dirs: HashSet<String> = self.vault.with_value(|cell| {
            cell.borrow()
                .as_ref()
                .map(|v| {
                    v.entries
                        .iter()
                        .filter(|e| e.kind == "dir" && !is_hidden(&e.path))
                        .map(|e| e.path.clone())
                        .collect()
                })
                .unwrap_or_default()
        });
        let currently_open = !self.expanded_dirs.get_untracked().is_empty();
        self.expanded_dirs.set(if currently_open {
            HashSet::new()
        } else {
            all_dirs
        });
        self.persist_expanded();
    }

    fn persist_expanded(&self) {
        let list: Vec<String> = self.expanded_dirs.get_untracked().into_iter().collect();
        self.patch_workspace(|w| w.expanded = list);
    }

    pub fn set_sort(&self, sort: String) {
        self.sort_mode.set(sort.clone());
        self.vault.with_value(|cell| {
            if let Some(v) = cell.borrow_mut().as_mut() {
                v.sort = sort.clone();
                v.tree = v.build_tree();
                v.workspace.sort = sort;
                let w = v.workspace.clone();
                let fs = v.fs.clone();
                spawn_local(async move {
                    if let Ok(json) = serde_json::to_string_pretty(&w) {
                        let _ = fs.write(".inkdown/workspace.json", &json, None).await;
                    }
                });
            }
        });
        self.bump();
    }

    pub fn run_search(self, query: String) {
        self.search_query.set(query.clone());
        if query.trim().is_empty() {
            self.search_hits.set(Vec::new());
            return;
        }
        spawn_local(async move {
            let Some((fs, notes)) = self.with_vault(|v| (v.fs.clone(), v.flat_notes())) else {
                return;
            };
            // 先把要检索的文本凑齐，再进纯内存的匹配：中间不持有任何借用
            let mut texts: HashMap<String, String> = HashMap::new();
            for note in &notes {
                let cached = self
                    .with_vault(|v| v.cached_text(&note.path).cloned())
                    .flatten();
                let text = match cached {
                    Some(text) => text,
                    None => match fs.read(&note.path).await {
                        Ok(Some(text)) => text,
                        _ => String::new(),
                    },
                };
                texts.insert(note.path.clone(), text);
            }
            let hits = self
                .with_vault(|v| v.search(&texts, &query, 60))
                .unwrap_or_default();
            if self.search_query.get_untracked() == query {
                self.search_hits.set(hits);
            }
        });
    }

    /// 把粘贴/拖入的图片写进 `attachments/`，返回相对路径。
    pub async fn save_attachment(self, filename: String, data_url: String) -> Result<String, String> {
        let fs = self.vault_fs().ok_or_else(|| "仓库未就绪".to_string())?;
        fs.mkdir("attachments").await?;
        let clean = sanitize_name(&filename);
        let name = if clean.is_empty() {
            format!("attachment_{}.png", now_ms())
        } else {
            clean
        };
        let rel = join_path("attachments", &name);
        fs.write_binary(&rel, &data_url).await?;
        self.rescan().await;
        Ok(rel)
    }

    pub fn jump_to_line(&self, line: usize) {
        let seq = self.jump_seq.get_untracked() + 1;
        self.jump_seq.set(seq);
        self.jump_to_line_req.set(Some((line, seq)));
    }

    pub fn set_editor_font_size(&self, size: f32) {
        let clamped = size.clamp(11.0, 26.0);
        self.editor_font_size.set(clamped);
        if let Some(theme) = use_context::<Theme>() {
            theme.set_token("--ink-editor-fs", &format!("{clamped}px"));
        }
        self.patch_workspace(|w| w.font_size = clamped);
    }

    pub fn set_theme_mode(&self, theme: Theme, mode_str: &str) {
        let mode = match mode_str {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::System,
        };
        theme.set_mode(mode);
        let m = mode_str.to_string();
        spawn_local(async move {
            sync_window_theme(&m).await;
        });
        self.patch_settings(|s| s.appearance.mode = mode_str.to_string());
    }

    pub fn set_accent(&self, theme: Theme, hex: &str) {
        let leaked = leak_accent(hex);
        theme.set_accent(leaked);
        self.accent_hex.set(hex.to_string());
        self.patch_settings(|s| s.appearance.accent = hex.to_string());
    }

    pub fn set_glass_level(&self, theme: Theme, level: f32) {
        let v = level.clamp(0.0, 1.0);
        theme.glass.set(v);
        self.glass_level.set(v);
        self.patch_settings(|s| s.appearance.glass = v);
    }

    pub fn set_tree_indent(&self, style: String) {
        self.tree_indent.set(style.clone());
        self.patch_settings(|s| s.appearance.tree_indent = style);
    }

    pub fn set_line_width(&self, theme: Theme, width: String) {
        self.line_width.set(width.clone());
        theme.set_token("--ink-line-w", line_width_css(&width));
        self.patch_settings(|s| s.editor.line_width = width);
    }

    pub fn set_font(&self, theme: Theme, kind: &str, family: String) {
        match kind {
            "ui" => {
                self.ui_font.set(family.clone());
                apply_font_token(&theme, "--font-sans", &family);
                self.patch_settings(|s| s.fonts.ui = family);
            }
            "markdown" => {
                self.md_font.set(family.clone());
                apply_font_token(&theme, "--ink-font-md", &family);
                self.patch_settings(|s| s.fonts.markdown = family);
            }
            "mono" => {
                self.mono_font.set(family.clone());
                apply_font_token(&theme, "--font-mono", &family);
                self.patch_settings(|s| s.fonts.mono = family);
            }
            _ => {}
        }
    }

    pub fn patch_settings<F: FnOnce(&mut crate::vault::VaultSettings)>(&self, f: F) {
        self.vault.with_value(|cell| {
            if let Some(v) = cell.borrow_mut().as_mut() {
                f(&mut v.settings);
                let s = v.settings.clone();
                let fs = v.fs.clone();
                spawn_local(async move {
                    if let Ok(json) = serde_json::to_string_pretty(&s) {
                        let _ = fs.write(".inkdown/settings.json", &json, None).await;
                    }
                });
            }
        });
    }

    pub fn patch_workspace<F: FnOnce(&mut crate::vault::WorkspaceState)>(&self, f: F) {
        self.vault.with_value(|cell| {
            if let Some(v) = cell.borrow_mut().as_mut() {
                f(&mut v.workspace);
                let w = v.workspace.clone();
                let fs = v.fs.clone();
                spawn_local(async move {
                    if let Ok(json) = serde_json::to_string_pretty(&w) {
                        let _ = fs.write(".inkdown/workspace.json", &json, None).await;
                    }
                });
            }
        });
    }

    /// 把工作区状态落盘（调用方已经在借用之外拿到了 fs 句柄）。
    async fn persist_workspace_via(&self, fs: &VaultFs) {
        if let Some(json) = self.with_vault(|v| v.workspace_json()) {
            let _ = fs.write(WORKSPACE_PATH, &json, None).await;
        }
    }

    /* ---------------- 桌面窗口与外部操作 ---------------- */

    pub fn pick_folder(self) {
        if !self.is_desktop {
            toast("浏览器预览使用本地沙盒仓库；桌面端可打开电脑上的任意文件夹");
            return;
        }
        spawn_local(async move {
            self.commit_doc().await;
            match invoke_empty::<Option<String>>("vault_pick").await {
                Ok(Some(path)) if !path.is_empty() => {
                    crate::tauri_bridge::ls_set(crate::vault::fs::FS_VAULT_KEY, &path);
                    reload_page();
                }
                Ok(_) => {}
                Err(err) => {
                    toast(format!("打开文件夹失败：{err}"));
                }
            }
        });
    }

    pub fn open_in_explorer(self, rel_path: String) {
        let cell = self.vault.get_value();
        let fs_opt = cell.borrow().as_ref().map(|v| v.fs.clone());
        let Some(fs) = fs_opt else { return };
        spawn_local(async move {
            match fs.open_in_explorer(&rel_path).await {
                Ok(()) => {
                    toast(if rel_path.is_empty() {
                        "已在资源管理器中打开仓库"
                    } else {
                        "已在资源管理器中定位"
                    });
                }
                Err(err) => {
                    toast(err);
                }
            }
        });
    }

    pub fn copy_vault_path(self, rel_path: String) {
        let cell = self.vault.get_value();
        let fs_opt = cell.borrow().as_ref().map(|v| v.fs.clone());
        let Some(fs) = fs_opt else { return };
        spawn_local(async move {
            let p = fs
                .abs_path(&rel_path)
                .await
                .unwrap_or_else(|_| rel_path.clone());
            match copy_to_clipboard(&p).await {
                Ok(()) => {
                    toast(format!("已复制路径：{p}"));
                }
                Err(err) => {
                    toast(format!("复制失败：{err}"));
                }
            }
        });
    }

    pub fn switch_local_vault(self, id: String) {
        spawn_local(async move {
            self.commit_doc().await;
            set_active_vault_id(&id);
            reload_page();
        });
    }

    pub fn create_local_vault(self, name: String) {
        spawn_local(async move {
            self.commit_doc().await;
            match Vault::create_local(&name).await {
                Ok(v) => {
                    set_active_vault_id(&v.meta.id);
                    reload_page();
                }
                Err(err) => {
                    toast(format!("创建仓库失败：{err}"));
                }
            }
        });
    }

    pub fn window_minimize(&self) {
        if !self.is_desktop {
            toast("浏览器预览模式下无法最小化桌面窗口");
            return;
        }
        spawn_local(async {
            let _ = invoke_empty_unit("window_minimize").await;
        });
    }

    pub fn window_toggle_maximize(&self) {
        if !self.is_desktop {
            toast("浏览器预览模式下无法切换窗口最大化");
            return;
        }
        let sig = self.window_maximized;
        spawn_local(async move {
            if let Ok(max) = invoke_empty::<bool>("window_toggle_maximize").await {
                sig.set(max);
            }
        });
    }

    pub fn window_close(self) {
        if !self.is_desktop {
            toast("这是浏览器预览，没有桌面窗口可关闭");
            return;
        }
        spawn_local(async move {
            self.commit_doc().await;
            let _ = invoke_empty_unit("window_close").await;
        });
    }

    pub fn list_browser_vaults(&self) -> Vec<VaultMeta> {
        list_vaults()
    }
}

fn line_width_css(width: &str) -> &'static str {
    match width {
        "wide" => "920px",
        "full" => "100%",
        _ => "720px",
    }
}

fn apply_font_token(theme: &Theme, token: &str, family: &str) {
    if family.trim().is_empty() {
        theme.remove_token(token);
    } else {
        let quoted = if family.contains(' ') && !family.starts_with('"') {
            format!("\"{family}\", system-ui, sans-serif")
        } else {
            format!("{family}, system-ui, sans-serif")
        };
        theme.set_token(token, &quoted);
    }
}

fn apply_typography_tokens(
    theme: &Theme,
    s: &crate::vault::VaultSettings,
    font_size: f32,
) {
    theme.set_token("--ink-editor-fs", &format!("{}px", font_size.clamp(11.0, 26.0)));
    theme.set_token("--ink-line-w", line_width_css(&s.editor.line_width));
    apply_font_token(theme, "--font-sans", &s.fonts.ui);
    apply_font_token(theme, "--ink-font-md", &s.fonts.markdown);
    apply_font_token(theme, "--font-mono", &s.fonts.mono);
}

fn leak_accent(hex: &str) -> &'static str {
    match hex.to_ascii_lowercase().as_str() {
        "#0a84ff" => "#0a84ff",
        "#5e5ce6" => "#5e5ce6",
        "#bf5af2" => "#bf5af2",
        "#ff375f" => "#ff375f",
        "#ff9f0a" => "#ff9f0a",
        "#30d158" => "#30d158",
        "#64d2ff" => "#64d2ff",
        "#8e8e93" => "#8e8e93",
        _ => "#0a84ff",
    }
}
