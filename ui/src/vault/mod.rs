//! 仓库域模型：一个文件夹就是一个知识库。
//!
//! 兼容既有的 `.inkdown/` 目录布局：
//! - `.inkdown/vault.json`
//! - `.inkdown/settings.json`
//! - `.inkdown/workspace.json`
//! - `.inkdown/index.json`
//! - `.inkdown/trash.json`
//! - `.trash/`

pub mod fs;
pub mod links;
pub mod seed;

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::tauri_bridge::{invoke_empty, invoke, is_tauri, ls_get, ls_set};
use fs::{
    active_vault_id, basename, dirname, fs_key_for, get_file_ext, get_file_type, is_hidden,
    is_markdown, join_path, list_vaults, now_ms, save_vaults, set_active_vault_id, strip_ext,
    FileType, FsEntry, LocalFs, VaultFs, VaultMeta, FS_VAULT_KEY,
};
use links::snippet_around;
use seed::{seed_notes, SEED_FOLDERS};

const META_PATH: &str = ".inkdown/vault.json";
const SETTINGS_PATH: &str = ".inkdown/settings.json";
const WORKSPACE_PATH: &str = ".inkdown/workspace.json";
const INDEX_PATH: &str = ".inkdown/index.json";
const TRASH_PATH: &str = ".inkdown/trash.json";
const TRASH_DIR: &str = ".trash";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppearanceSettings {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_accent")]
    pub accent: String,
    #[serde(rename = "treeIndent", default = "default_tree_indent")]
    pub tree_indent: String,
    #[serde(default = "default_glass")]
    pub glass: f32,
}

fn default_mode() -> String {
    "system".to_string()
}
fn default_accent() -> String {
    "#0a84ff".to_string()
}
fn default_tree_indent() -> String {
    "lines".to_string()
}
fn default_glass() -> f32 {
    1.0
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            accent: default_accent(),
            tree_indent: default_tree_indent(),
            glass: default_glass(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FontSettings {
    #[serde(default)]
    pub ui: String,
    #[serde(default)]
    pub markdown: String,
    #[serde(default)]
    pub mono: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorSettings {
    #[serde(default = "default_editor_size")]
    pub size: String,
    #[serde(rename = "lineWidth", default = "default_line_width")]
    pub line_width: String,
    #[serde(rename = "autoContinueLists", default = "default_true")]
    pub auto_continue_lists: bool,
    #[serde(rename = "autoSave", default = "default_true")]
    pub auto_save: bool,
}

fn default_editor_size() -> String {
    "md".to_string()
}
fn default_line_width() -> String {
    "standard".to_string()
}
fn default_true() -> bool {
    true
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            size: default_editor_size(),
            line_width: default_line_width(),
            auto_continue_lists: true,
            auto_save: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultSettings {
    #[serde(default = "default_v1")]
    pub version: u32,
    #[serde(default)]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub fonts: FontSettings,
    #[serde(default)]
    pub editor: EditorSettings,
}

fn default_v1() -> u32 {
    1
}

impl Default for VaultSettings {
    fn default() -> Self {
        Self {
            version: 1,
            appearance: AppearanceSettings::default(),
            fonts: FontSettings::default(),
            editor: EditorSettings::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceState {
    #[serde(default = "default_v1")]
    pub version: u32,
    #[serde(rename = "lastNote", default)]
    pub last_note: String,
    #[serde(default)]
    pub expanded: Vec<String>,
    #[serde(default = "default_sort")]
    pub sort: String,
    #[serde(default = "default_view_mode")]
    pub mode: String,
    #[serde(default = "default_true")]
    pub sidebar: bool,
    #[serde(default = "default_inspector_tab")]
    pub view: String,
    #[serde(default = "default_split")]
    pub split: f64,
    #[serde(rename = "listRatio", default = "default_list_ratio")]
    pub list_ratio: f64,
    #[serde(rename = "navHidden", default)]
    pub nav_hidden: bool,
    #[serde(rename = "listHidden", default)]
    pub list_hidden: bool,
    #[serde(rename = "recentsOpen", default)]
    pub recents_open: bool,
    #[serde(default)]
    pub recents: Vec<String>,
    #[serde(rename = "openTabs", default)]
    pub open_tabs: Vec<String>,
    #[serde(rename = "fontSize", default = "default_font_size")]
    pub font_size: f32,
}

fn default_sort() -> String {
    "updated".to_string()
}
fn default_view_mode() -> String {
    "split".to_string()
}
fn default_inspector_tab() -> String {
    "outline".to_string()
}
fn default_split() -> f64 {
    0.5
}
fn default_list_ratio() -> f64 {
    0.24
}
fn default_font_size() -> f32 {
    14.0
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            version: 1,
            last_note: String::new(),
            expanded: Vec::new(),
            sort: default_sort(),
            mode: default_view_mode(),
            sidebar: true,
            view: default_inspector_tab(),
            split: default_split(),
            list_ratio: default_list_ratio(),
            nav_hidden: false,
            list_hidden: false,
            recents_open: false,
            recents: Vec::new(),
            open_tabs: Vec::new(),
            font_size: default_font_size(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NoteIndexEntry {
    #[serde(default)]
    pub favorite: bool,
    #[serde(rename = "createdAt", default)]
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultIndex {
    #[serde(default = "default_v1")]
    pub version: u32,
    #[serde(default)]
    pub notes: BTreeMap<String, NoteIndexEntry>,
}

impl Default for VaultIndex {
    fn default() -> Self {
        Self {
            version: 1,
            notes: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashEntry {
    pub path: String,
    #[serde(rename = "originalPath")]
    pub original_path: String,
    pub name: String,
    #[serde(rename = "deletedAt")]
    pub deleted_at: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VaultTreeNode {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub mtime: u64,
    pub favorite: bool,
    pub ext: String,
    pub file_type: FileType,
    pub is_markdown: bool,
    pub children: Vec<VaultTreeNode>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NoteInfo {
    pub path: String,
    pub title: String,
    pub name: String,
    pub mtime: u64,
    pub created_at: u64,
    pub favorite: bool,
    pub ext: String,
    pub file_type: FileType,
    pub is_markdown: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    pub note: NoteInfo,
    pub snippet: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VaultCounts {
    pub all: usize,
    pub favorites: usize,
    pub trash: usize,
}

pub fn sanitize_name(name: &str) -> String {
    let illegal = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
    let cleaned: String = name
        .chars()
        .filter(|c| !illegal.contains(c))
        .collect();
    let trimmed = cleaned.trim_start_matches('.');
    let collapsed = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(80).collect()
}

pub struct Vault {
    pub fs: VaultFs,
    pub meta: VaultMeta,
    pub entries: Vec<FsEntry>,
    pub tree: Vec<VaultTreeNode>,
    pub index: VaultIndex,
    pub trash: Vec<TrashEntry>,
    pub settings: VaultSettings,
    pub workspace: WorkspaceState,
    pub cache: HashMap<String, String>,
    pub sort: String,
}

impl Vault {
    pub async fn open_or_create() -> Result<Self, String> {
        if is_tauri() {
            if let Some(saved) = ls_get(FS_VAULT_KEY).filter(|s| !s.trim().is_empty()) {
                if let Ok(v) = Self::open_folder(&saved).await {
                    return Ok(v);
                }
            }
            if let Ok(Some(boot_path)) = invoke_empty::<Option<String>>("vault_boot_path").await {
                if let Ok(v) = Self::open_folder(&boot_path).await {
                    return Ok(v);
                }
            }
        }

        let registry = list_vaults();
        let active = active_vault_id();
        if !active.is_empty() && registry.iter().any(|v| v.id == active) {
            if let Ok(v) = Self::open_local(&active).await {
                return Ok(v);
            }
        }
        if let Some(first) = registry.first() {
            if let Ok(v) = Self::open_local(&first.id).await {
                return Ok(v);
            }
        }
        Self::create_local("我的仓库").await
    }

    pub async fn open_folder(path: &str) -> Result<Self, String> {
        if !is_tauri() {
            return Err("当前环境不支持文件夹仓库".into());
        }
        #[derive(Serialize)]
        struct Arg<'a> {
            path: &'a str,
        }
        let resolved: String = invoke("vault_open", &Arg { path }).await?;
        let fs = VaultFs::Tauri {
            root: resolved.clone(),
        };

        let meta = match fs.read(META_PATH).await? {
            Some(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| VaultMeta {
                id: format!("fs_{}", now_ms()),
                name: {
                    let b = basename(&resolved);
                    if b.is_empty() { "仓库".to_string() } else { b }
                },
                version: 1,
                created_at: now_ms(),
            }),
            None => VaultMeta {
                id: format!("fs_{}", now_ms()),
                name: {
                    let b = basename(&resolved);
                    if b.is_empty() { "仓库".to_string() } else { b }
                },
                version: 1,
                created_at: now_ms(),
            },
        };

        let mut vault = Self::empty(fs, meta);
        let existed_entries = vault.fs.walk().await?;
        let has_any_notes = existed_entries
            .iter()
            .any(|e| e.kind == "file" && !is_hidden(&e.path));
        vault.load().await?;
        if !has_any_notes {
            vault.seed().await?;
            vault.refresh().await?;
        }
        ls_set(FS_VAULT_KEY, &resolved);
        Ok(vault)
    }

    pub async fn open_local(id: &str) -> Result<Self, String> {
        let mut registry = list_vaults();
        let fs = VaultFs::Local(LocalFs::new(fs_key_for(id)));
        let meta = if let Some(m) = registry.iter().find(|v| v.id == id).cloned() {
            m
        } else {
            let raw = fs
                .read(META_PATH)
                .await?
                .ok_or_else(|| format!("仓库 {id} 不存在"))?;
            let m: VaultMeta = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            registry.push(m.clone());
            save_vaults(&registry);
            m
        };
        set_active_vault_id(id);
        let mut vault = Self::empty(fs, meta);
        vault.load().await?;
        Ok(vault)
    }

    pub async fn create_local(name: &str) -> Result<Self, String> {
        let id = format!("v_{}", now_ms());
        let clean = sanitize_name(name);
        let meta = VaultMeta {
            id: id.clone(),
            name: if clean.is_empty() {
                "未命名仓库".to_string()
            } else {
                clean
            },
            version: 1,
            created_at: now_ms(),
        };
        let fs = VaultFs::Local(LocalFs::new(fs_key_for(&id)));
        fs.mkdir(".inkdown").await?;
        fs.mkdir(TRASH_DIR).await?;
        fs.write(
            META_PATH,
            &serde_json::to_string_pretty(&meta).unwrap_or_default(),
            None,
        )
        .await?;

        let mut registry = list_vaults();
        registry.push(meta.clone());
        save_vaults(&registry);
        set_active_vault_id(&id);

        let mut vault = Self::empty(fs, meta);
        vault.load().await?;
        vault.seed().await?;
        vault.refresh().await?;
        Ok(vault)
    }

    fn empty(fs: VaultFs, meta: VaultMeta) -> Self {
        Self {
            fs,
            meta,
            entries: Vec::new(),
            tree: Vec::new(),
            index: VaultIndex::default(),
            trash: Vec::new(),
            settings: VaultSettings::default(),
            workspace: WorkspaceState::default(),
            cache: HashMap::new(),
            sort: "updated".to_string(),
        }
    }

    pub async fn load(&mut self) -> Result<(), String> {
        self.ensure_structure().await?;
        if let Some(raw) = self.fs.read(SETTINGS_PATH).await? {
            if let Ok(s) = serde_json::from_str(&raw) {
                self.settings = s;
            }
        }
        if let Some(raw) = self.fs.read(WORKSPACE_PATH).await? {
            if let Ok(w) = serde_json::from_str(&raw) {
                self.workspace = w;
            }
        }
        if let Some(raw) = self.fs.read(INDEX_PATH).await? {
            if let Ok(idx) = serde_json::from_str(&raw) {
                self.index = idx;
            }
        }
        if let Some(raw) = self.fs.read(TRASH_PATH).await? {
            if let Ok(t) = serde_json::from_str(&raw) {
                self.trash = t;
            }
        }

        self.sort = match self.workspace.sort.as_str() {
            "title" | "created" | "updated" => self.workspace.sort.clone(),
            _ => "updated".to_string(),
        };
        self.refresh().await?;
        Ok(())
    }

    async fn ensure_structure(&self) -> Result<(), String> {
        self.fs.mkdir(".inkdown").await?;
        self.fs.mkdir(TRASH_DIR).await?;
        if !self.fs.exists(META_PATH).await? {
            self.write_json(META_PATH, &self.meta).await?;
        }
        if !self.fs.exists(SETTINGS_PATH).await? {
            self.write_json(SETTINGS_PATH, &VaultSettings::default()).await?;
        }
        if !self.fs.exists(WORKSPACE_PATH).await? {
            self.write_json(WORKSPACE_PATH, &WorkspaceState::default()).await?;
        }
        if !self.fs.exists(INDEX_PATH).await? {
            self.write_json(INDEX_PATH, &VaultIndex::default()).await?;
        }
        if !self.fs.exists(TRASH_PATH).await? {
            self.fs.write(TRASH_PATH, "[]", None).await?;
        }
        Ok(())
    }

    async fn seed(&mut self) -> Result<(), String> {
        for dir in SEED_FOLDERS {
            self.fs.mkdir(dir).await?;
        }
        let mut index = VaultIndex::default();
        for note in seed_notes(now_ms()) {
            self.fs
                .write(note.path, &note.content, Some(note.mtime))
                .await?;
            self.cache.insert(note.path.to_string(), note.content);
            index.notes.insert(
                note.path.to_string(),
                NoteIndexEntry {
                    favorite: note.favorite,
                    created_at: note.created_at,
                },
            );
        }
        self.index = index;
        self.write_json(INDEX_PATH, &self.index).await
    }

    pub async fn write_json<T: Serialize>(&self, path: &str, value: &T) -> Result<(), String> {
        let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        self.fs.write(path, &json, None).await
    }

    pub async fn persist_settings(&self) {
        let _ = self.write_json(SETTINGS_PATH, &self.settings).await;
    }

    pub async fn persist_workspace(&self) {
        let _ = self.write_json(WORKSPACE_PATH, &self.workspace).await;
    }

    pub async fn refresh(&mut self) -> Result<(), String> {
        self.entries = self.fs.walk().await?;
        self.tree = self.build_tree();
        Ok(())
    }

    pub fn build_tree(&self) -> Vec<VaultTreeNode> {
        let mut visible: Vec<&FsEntry> = self
            .entries
            .iter()
            .filter(|e| !is_hidden(&e.path))
            .collect();
        visible.sort_by(|a, b| a.path.cmp(&b.path));

        // 先按父路径分组再递归组装，避免悬空指针
        let mut dirs_by_parent: HashMap<String, Vec<&FsEntry>> = HashMap::new();
        let mut files_by_parent: HashMap<String, Vec<&FsEntry>> = HashMap::new();

        for entry in visible {
            let parent = dirname(&entry.path);
            if entry.kind == "dir" {
                dirs_by_parent.entry(parent).or_default().push(entry);
            } else if entry.kind == "file" {
                files_by_parent.entry(parent).or_default().push(entry);
            }
        }

        self.build_children("", &dirs_by_parent, &files_by_parent)
    }

    fn build_children(
        &self,
        parent: &str,
        dirs_by_parent: &HashMap<String, Vec<&FsEntry>>,
        files_by_parent: &HashMap<String, Vec<&FsEntry>>,
    ) -> Vec<VaultTreeNode> {
        let mut out = Vec::new();
        if let Some(dirs) = dirs_by_parent.get(parent) {
            for d in dirs {
                let children = self.build_children(&d.path, dirs_by_parent, files_by_parent);
                out.push(VaultTreeNode {
                    path: d.path.clone(),
                    name: basename(&d.path),
                    is_dir: true,
                    mtime: d.mtime,
                    favorite: false,
                    ext: String::new(),
                    file_type: FileType::Other,
                    is_markdown: false,
                    children,
                });
            }
        }
        if let Some(files) = files_by_parent.get(parent) {
            for f in files {
                let is_md = is_markdown(&f.path);
                let name = if is_md {
                    strip_ext(&basename(&f.path))
                } else {
                    basename(&f.path)
                };
                out.push(VaultTreeNode {
                    path: f.path.clone(),
                    name,
                    is_dir: false,
                    mtime: f.mtime,
                    favorite: self
                        .index
                        .notes
                        .get(&f.path)
                        .map(|n| n.favorite)
                        .unwrap_or(false),
                    ext: get_file_ext(&f.path),
                    file_type: get_file_type(&f.path),
                    is_markdown: is_md,
                    children: Vec::new(),
                });
            }
        }
        sort_nodes(&mut out, &self.sort, &self.index);
        out
    }

    pub fn note_at(&self, path: &str) -> Option<NoteInfo> {
        if path.is_empty() {
            return None;
        }
        let entry = self
            .entries
            .iter()
            .find(|e| e.path == path && e.kind == "file")?;
        let meta = self.index.notes.get(path);
        let is_md = is_markdown(path);
        let base = basename(path);
        Some(NoteInfo {
            path: path.to_string(),
            title: if is_md { strip_ext(&base) } else { base.clone() },
            name: base,
            mtime: entry.mtime,
            created_at: meta.map(|m| m.created_at).unwrap_or(entry.mtime),
            favorite: meta.map(|m| m.favorite).unwrap_or(false),
            ext: get_file_ext(path),
            file_type: get_file_type(path),
            is_markdown: is_md,
        })
    }

    pub fn flat_notes(&self) -> Vec<NoteInfo> {
        self.entries
            .iter()
            .filter(|e| e.kind == "file" && is_markdown(&e.path) && !is_hidden(&e.path))
            .filter_map(|e| self.note_at(&e.path))
            .collect()
    }

    pub fn favorites(&self) -> Vec<NoteInfo> {
        let mut favs: Vec<NoteInfo> = self
            .flat_notes()
            .into_iter()
            .filter(|n| n.favorite)
            .collect();
        favs.sort_by(|a, b| b.mtime.cmp(&a.mtime));
        favs
    }

    pub fn counts(&self) -> VaultCounts {
        let notes = self.flat_notes();
        let fav = notes.iter().filter(|n| n.favorite).count();
        VaultCounts {
            all: notes.len(),
            favorites: fav,
            trash: self.trash.len(),
        }
    }

    pub async fn read_note(&mut self, path: &str, fresh: bool) -> Result<String, String> {
        if !fresh {
            if let Some(cached) = self.cache.get(path) {
                return Ok(cached.clone());
            }
        }
        let text = self.fs.read(path).await?.unwrap_or_default();
        self.cache.insert(path.to_string(), text.clone());
        Ok(text)
    }

    pub async fn save_note(&mut self, path: &str, text: &str) -> Result<u64, String> {
        let mtime = now_ms();
        self.fs.write(path, text, Some(mtime)).await?;
        self.cache.insert(path.to_string(), text.to_string());
        if let Some(entry) = self.entries.iter_mut().find(|e| e.path == path) {
            entry.mtime = mtime;
        }
        update_tree_mtime(&mut self.tree, path, mtime);
        Ok(mtime)
    }

    pub async fn save_attachment(&mut self, filename: &str, base64_data: &str) -> Result<String, String> {
        let attach_dir = "attachments";
        self.fs.mkdir(attach_dir).await?;
        let clean = sanitize_name(filename);
        let name = if clean.is_empty() {
            format!("attachment_{}.png", now_ms())
        } else {
            clean
        };
        let rel = join_path(attach_dir, &name);
        self.fs.write_binary(&rel, base64_data).await?;
        self.refresh().await?;
        Ok(rel)
    }

    pub async fn search(&mut self, query: &str, limit: usize) -> Vec<SearchHit> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let notes = self.flat_notes();
        let mut hits = Vec::new();

        for note in notes {
            if hits.len() >= limit {
                break;
            }
            let text = self.read_note(&note.path, false).await.unwrap_or_default();
            let title_hit = note.title.to_lowercase().contains(&q);
            let content_hit = text.to_lowercase().contains(&q);
            if !title_hit && !content_hit {
                continue;
            }
            let snippet = if content_hit {
                snippet_around(&text, query.trim(), 48)
            } else {
                String::new()
            };
            hits.push(SearchHit { note, snippet });
        }
        hits.sort_by(|a, b| b.note.mtime.cmp(&a.note.mtime));
        hits
    }

    pub fn unique_name(&self, dir: &str, base: &str, ext: &str) -> String {
        let taken: HashSet<String> = self
            .entries
            .iter()
            .filter(|e| dirname(&e.path) == dir)
            .map(|e| basename(&e.path).to_lowercase())
            .collect();

        let mut name = base.to_string();
        let mut i = 2usize;
        while taken.contains(&format!("{name}{ext}").to_lowercase()) {
            name = format!("{base} {i}");
            i += 1;
        }
        name
    }

    pub async fn create_note(&mut self, dir: &str, base: &str) -> Result<String, String> {
        let clean = sanitize_name(base);
        let stem = if clean.is_empty() { "未命名笔记" } else { &clean };
        let name = self.unique_name(dir, stem, ".md");
        let path = join_path(dir, &format!("{name}.md"));

        self.fs.write(&path, "", None).await?;
        self.cache.insert(path.clone(), String::new());
        self.index.notes.insert(
            path.clone(),
            NoteIndexEntry {
                favorite: false,
                created_at: now_ms(),
            },
        );
        self.write_json(INDEX_PATH, &self.index).await?;
        self.refresh().await?;
        Ok(path)
    }

    pub async fn create_folder(&mut self, dir: &str, base: &str) -> Result<String, String> {
        let clean = sanitize_name(base);
        let stem = if clean.is_empty() { "新建文件夹" } else { &clean };
        let name = self.unique_name(dir, stem, "");
        let path = join_path(dir, &name);
        self.fs.mkdir(&path).await?;
        self.refresh().await?;
        Ok(path)
    }

    pub async fn rename_note(&mut self, path: &str, next_base: &str) -> Result<String, String> {
        let clean = sanitize_name(next_base);
        if clean.is_empty() {
            return Err("名称不能为空".into());
        }
        let dir = dirname(path);
        if clean == strip_ext(&basename(path)) {
            return Ok(path.to_string());
        }
        let unique = self.unique_name(&dir, &clean, ".md");
        let next = join_path(&dir, &format!("{unique}.md"));
        if next == path {
            return Ok(path.to_string());
        }

        self.fs.rename(path, &next).await?;
        if let Some(meta) = self.index.notes.remove(path) {
            self.index.notes.insert(next.clone(), meta);
            self.write_json(INDEX_PATH, &self.index).await?;
        }
        if let Some(cached) = self.cache.remove(path) {
            self.cache.insert(next.clone(), cached);
        }
        self.migrate_workspace_path(path, &next);
        self.persist_workspace().await;
        self.refresh().await?;
        Ok(next)
    }

    pub async fn rename_folder(&mut self, path: &str, next_name: &str) -> Result<String, String> {
        let clean = sanitize_name(next_name);
        if clean.is_empty() {
            return Err("名称不能为空".into());
        }
        if clean == basename(path) {
            return Ok(path.to_string());
        }
        let parent = dirname(path);
        let unique = self.unique_name(&parent, &clean, "");
        let next = join_path(&parent, &unique);
        self.fs.rename(path, &next).await?;
        self.migrate_prefix(path, &next).await?;
        self.refresh().await?;
        Ok(next)
    }

    pub async fn move_item(&mut self, src_path: &str, target_dir: &str) -> Result<String, String> {
        if src_path.is_empty() {
            return Err("源路径为空".into());
        }
        if target_dir == src_path || target_dir.starts_with(&format!("{src_path}/")) {
            return Err("无法将文件夹移入其自身或子文件夹中".into());
        }
        let base = basename(src_path);
        let tentative = join_path(target_dir, &base);
        if src_path == tentative {
            return Ok(src_path.to_string());
        }

        let ext = get_file_ext(&base);
        let is_dir = self
            .entries
            .iter()
            .any(|e| e.path == src_path && e.kind == "dir");
        let dot_ext = if is_dir || ext.is_empty() {
            String::new()
        } else {
            format!(".{ext}")
        };
        let stem = if is_dir || ext.is_empty() {
            base.clone()
        } else {
            strip_ext(&base)
        };
        let unique = self.unique_name(target_dir, &stem, &dot_ext);
        let next_path = join_path(target_dir, &format!("{unique}{dot_ext}"));

        self.fs.rename(src_path, &next_path).await?;
        if is_dir {
            self.migrate_prefix(src_path, &next_path).await?;
        } else {
            if let Some(meta) = self.index.notes.remove(src_path) {
                self.index.notes.insert(next_path.clone(), meta);
                self.write_json(INDEX_PATH, &self.index).await?;
            }
            if let Some(cached) = self.cache.remove(src_path) {
                self.cache.insert(next_path.clone(), cached);
            }
            self.migrate_workspace_path(src_path, &next_path);
            self.persist_workspace().await;
        }
        self.refresh().await?;
        Ok(next_path)
    }

    pub async fn duplicate_note(&mut self, path: &str) -> Result<String, String> {
        let content = self.read_note(path, false).await?;
        let dir = dirname(path);
        let base = format!("{} 副本", strip_ext(&basename(path)));
        let new_path = self.create_note(&dir, &base).await?;
        self.save_note(&new_path, &content).await?;
        Ok(new_path)
    }

    pub async fn delete_folder(&mut self, path: &str) -> Result<(), String> {
        self.fs.remove(path).await?;
        let prefix = format!("{path}/");
        self.index
            .notes
            .retain(|k, _| k != path && !k.starts_with(&prefix));
        self.cache
            .retain(|k, _| k != path && !k.starts_with(&prefix));
        self.workspace
            .recents
            .retain(|p| p != path && !p.starts_with(&prefix));
        self.workspace
            .open_tabs
            .retain(|p| p != path && !p.starts_with(&prefix));
        self.write_json(INDEX_PATH, &self.index).await?;
        self.persist_workspace().await;
        self.refresh().await
    }

    pub async fn toggle_favorite(&mut self, path: &str) -> Result<bool, String> {
        let entry = self
            .index
            .notes
            .entry(path.to_string())
            .or_insert_with(|| NoteIndexEntry {
                favorite: false,
                created_at: now_ms(),
            });
        entry.favorite = !entry.favorite;
        let fav = entry.favorite;
        self.write_json(INDEX_PATH, &self.index).await?;
        update_tree_favorite(&mut self.tree, path, fav);
        Ok(fav)
    }

    pub async fn trash_note(&mut self, path: &str) -> Result<String, String> {
        let name = basename(path);
        let trash_path = join_path(TRASH_DIR, &format!("{}-{name}", now_ms()));
        self.fs.rename(path, &trash_path).await?;
        self.trash.push(TrashEntry {
            path: trash_path.clone(),
            original_path: path.to_string(),
            name: strip_ext(&name),
            deleted_at: now_ms(),
        });
        self.write_json(TRASH_PATH, &self.trash).await?;
        if let Some(cached) = self.cache.remove(path) {
            self.cache.insert(trash_path.clone(), cached);
        }
        self.workspace.recents.retain(|p| p != path);
        self.workspace.open_tabs.retain(|p| p != path);
        self.persist_workspace().await;
        self.refresh().await?;
        Ok(trash_path)
    }

    pub async fn restore_trash(&mut self, trash_path: &str) -> Result<String, String> {
        let entry = self
            .trash
            .iter()
            .find(|t| t.path == trash_path)
            .cloned()
            .ok_or_else(|| "回收站里找不到这篇笔记".to_string())?;

        let dir = dirname(&entry.original_path);
        if !dir.is_empty() {
            self.fs.mkdir(&dir).await?;
        }

        let mut target = entry.original_path.clone();
        if self.fs.exists(&target).await? {
            let ext = get_file_ext(&target);
            let dot_ext = if ext.is_empty() {
                ".md".to_string()
            } else {
                format!(".{ext}")
            };
            let unique = self.unique_name(&dir, &strip_ext(&basename(&target)), &dot_ext);
            target = join_path(&dir, &format!("{unique}{dot_ext}"));
        }

        self.fs.rename(trash_path, &target).await?;
        self.trash.retain(|t| t.path != trash_path);
        self.write_json(TRASH_PATH, &self.trash).await?;
        if let Some(cached) = self.cache.remove(trash_path) {
            self.cache.insert(target.clone(), cached);
        }
        self.refresh().await?;
        Ok(target)
    }

    pub async fn delete_forever(&mut self, trash_path: &str) -> Result<(), String> {
        let entry = self.trash.iter().find(|t| t.path == trash_path).cloned();
        self.fs.remove(trash_path).await?;
        if let Some(e) = entry {
            self.index.notes.remove(&e.original_path);
            self.write_json(INDEX_PATH, &self.index).await?;
        }
        self.trash.retain(|t| t.path != trash_path);
        self.write_json(TRASH_PATH, &self.trash).await?;
        self.cache.remove(trash_path);
        self.refresh().await
    }

    pub async fn empty_trash(&mut self) -> Result<(), String> {
        let items: Vec<TrashEntry> = self.trash.drain(..).collect();
        for item in items {
            let _ = self.fs.remove(&item.path).await;
            self.index.notes.remove(&item.original_path);
            self.cache.remove(&item.path);
        }
        self.write_json(INDEX_PATH, &self.index).await?;
        self.write_json(TRASH_PATH, &self.trash).await?;
        self.refresh().await
    }

    fn migrate_workspace_path(&mut self, from: &str, to: &str) {
        if self.workspace.last_note == from {
            self.workspace.last_note = to.to_string();
        }
        for item in &mut self.workspace.recents {
            if item == from {
                *item = to.to_string();
            }
        }
        for item in &mut self.workspace.open_tabs {
            if item == from {
                *item = to.to_string();
            }
        }
    }

    async fn migrate_prefix(&mut self, from: &str, to: &str) -> Result<(), String> {
        let prefix = format!("{from}/");
        let moved_notes: Vec<(String, NoteIndexEntry)> = self
            .index
            .notes
            .iter()
            .filter(|(k, _)| *k == from || k.starts_with(&prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (k, v) in moved_notes {
            self.index.notes.remove(&k);
            let next = format!("{to}{}", &k[from.len()..]);
            self.index.notes.insert(next, v);
        }

        let moved_cache: Vec<(String, String)> = self
            .cache
            .iter()
            .filter(|(k, _)| *k == from || k.starts_with(&prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (k, v) in moved_cache {
            self.cache.remove(&k);
            let next = format!("{to}{}", &k[from.len()..]);
            self.cache.insert(next, v);
        }

        let rewrite = |p: &mut String| {
            if *p == from || p.starts_with(&prefix) {
                *p = format!("{to}{}", &p[from.len()..]);
            }
        };
        rewrite(&mut self.workspace.last_note);
        for p in &mut self.workspace.recents {
            rewrite(p);
        }
        for p in &mut self.workspace.open_tabs {
            rewrite(p);
        }

        self.write_json(INDEX_PATH, &self.index).await?;
        self.persist_workspace().await;
        Ok(())
    }
}

fn sort_nodes(nodes: &mut Vec<VaultTreeNode>, sort: &str, index: &VaultIndex) {
    nodes.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return if a.is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        if a.is_dir || sort == "title" {
            return a.name.to_lowercase().cmp(&b.name.to_lowercase());
        }
        if sort == "created" {
            let ca = index.notes.get(&a.path).map(|n| n.created_at).unwrap_or(0);
            let cb = index.notes.get(&b.path).map(|n| n.created_at).unwrap_or(0);
            return cb.cmp(&ca);
        }
        b.mtime.cmp(&a.mtime)
    });
}

fn update_tree_mtime(nodes: &mut [VaultTreeNode], path: &str, mtime: u64) {
    for node in nodes {
        if node.path == path {
            node.mtime = mtime;
        }
        if !node.children.is_empty() {
            update_tree_mtime(&mut node.children, path, mtime);
        }
    }
}

fn update_tree_favorite(nodes: &mut [VaultTreeNode], path: &str, fav: bool) {
    for node in nodes {
        if node.path == path {
            node.favorite = fav;
        }
        if !node.children.is_empty() {
            update_tree_favorite(&mut node.children, path, fav);
        }
    }
}

/// 切换本地浏览器仓库时用的 helper（保留与旧版本一致的多库切换）。
pub fn list_local_vaults() -> Vec<VaultMeta> {
    list_vaults()
}
