//! 仓库存储适配层：统一桌面端 TauriFS 与浏览器端 LocalFS。

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::tauri_bridge::{invoke, invoke_empty, invoke_unit, ls_get, ls_set};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub mtime: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileType {
    Markdown,
    Pdf,
    Image,
    Video,
    Audio,
    Word,
    Excel,
    Ppt,
    Code,
    Other,
}

impl FileType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Pdf => "pdf",
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Word => "word",
            Self::Excel => "excel",
            Self::Ppt => "ppt",
            Self::Code => "code",
            Self::Other => "other",
        }
    }
}

/* ---------------- 路径工具 ---------------- */

pub fn norm(p: &str) -> String {
    let replaced = p.replace('\\', "/");
    let parts: Vec<&str> = replaced
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    parts.join("/")
}

pub fn dirname(p: &str) -> String {
    let s = norm(p);
    match s.rfind('/') {
        Some(idx) => s[..idx].to_string(),
        None => String::new(),
    }
}

pub fn basename(p: &str) -> String {
    let s = norm(p);
    match s.rfind('/') {
        Some(idx) => s[idx + 1..].to_string(),
        None => s,
    }
}

pub fn join_path(a: &str, b: &str) -> String {
    let a = norm(a);
    let b = norm(b);
    if a.is_empty() {
        b
    } else if b.is_empty() {
        a
    } else {
        format!("{a}/{b}")
    }
}

pub fn strip_ext(name: &str) -> String {
    match name.rfind('.') {
        Some(idx) if idx > 0 => name[..idx].to_string(),
        _ => name.to_string(),
    }
}

pub fn get_file_ext(p: &str) -> String {
    let name = basename(p);
    match name.rfind('.') {
        Some(idx) if idx > 0 => name[idx + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

pub fn is_markdown(p: &str) -> bool {
    matches!(
        get_file_ext(p).as_str(),
        "md" | "markdown" | "mdown"
    )
}

pub fn is_hidden(p: &str) -> bool {
    norm(p).split('/').any(|seg| seg.starts_with('.'))
}

pub fn get_file_type(p: &str) -> FileType {
    match get_file_ext(p).as_str() {
        "md" | "markdown" | "mdown" => FileType::Markdown,
        "pdf" => FileType::Pdf,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" => FileType::Image,
        "mp4" | "webm" | "mov" | "mkv" | "avi" => FileType::Video,
        "mp3" | "wav" | "m4a" | "ogg" | "flac" | "aac" => FileType::Audio,
        "docx" | "doc" => FileType::Word,
        "xlsx" | "xls" | "csv" => FileType::Excel,
        "pptx" | "ppt" => FileType::Ppt,
        "js" | "ts" | "jsx" | "tsx" | "mjs" | "cjs" | "rs" | "py" | "json" | "css" | "scss"
        | "html" | "htm" | "toml" | "yaml" | "yml" | "sql" | "sh" | "bash" | "c" | "cpp"
        | "h" | "hpp" | "go" | "java" | "kt" | "swift" | "php" | "rb" | "lua" | "xml"
        | "ini" | "env" => FileType::Code,
        _ => FileType::Other,
    }
}

pub fn now_ms() -> u64 {
    js_sys::Date::now() as u64
}

/* ---------------- 浏览器本地虚拟存储 ---------------- */

#[derive(Clone, Debug, Serialize, Deserialize)]
struct LocalEntry {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(default)]
    mtime: u64,
}

#[derive(Clone)]
pub struct LocalFs {
    key: String,
    entries: Rc<RefCell<BTreeMap<String, LocalEntry>>>,
}

impl LocalFs {
    pub fn new(key: String) -> Self {
        let mut map = BTreeMap::new();
        if let Some(raw) = ls_get(&key) {
            if let Ok(parsed) = serde_json::from_str::<BTreeMap<String, LocalEntry>>(&raw) {
                for (k, v) in parsed {
                    map.insert(norm(&k), v);
                }
            }
        }
        Self {
            key,
            entries: Rc::new(RefCell::new(map)),
        }
    }

    fn persist(&self) {
        if let Ok(json) = serde_json::to_string(&*self.entries.borrow()) {
            ls_set(&self.key, &json);
        }
    }

    fn ensure_parents(&self, path: &str) {
        let norm_p = norm(path);
        let parts: Vec<&str> = norm_p.split('/').collect();
        if parts.len() <= 1 {
            return;
        }
        let mut map = self.entries.borrow_mut();
        let mut cur = String::new();
        for part in &parts[..parts.len() - 1] {
            cur = if cur.is_empty() {
                (*part).to_string()
            } else {
                format!("{cur}/{part}")
            };
            map.entry(cur.clone()).or_insert_with(|| LocalEntry {
                kind: "dir".to_string(),
                text: None,
                data: None,
                mtime: now_ms(),
            });
        }
    }

    pub fn exists(&self, path: &str) -> bool {
        self.entries.borrow().contains_key(&norm(path))
    }

    pub fn read(&self, path: &str) -> Option<String> {
        let map = self.entries.borrow();
        let e = map.get(&norm(path))?;
        if e.kind == "file" {
            Some(e.text.clone().unwrap_or_default())
        } else {
            None
        }
    }

    pub fn read_binary(&self, path: &str) -> Option<String> {
        let map = self.entries.borrow();
        let e = map.get(&norm(path))?;
        if e.kind == "file" {
            e.data.clone()
        } else {
            None
        }
    }

    pub fn write(&self, path: &str, text: &str, mtime: u64) {
        let p = norm(path);
        self.ensure_parents(&p);
        self.entries.borrow_mut().insert(
            p,
            LocalEntry {
                kind: "file".to_string(),
                text: Some(text.to_string()),
                data: None,
                mtime,
            },
        );
        self.persist();
    }

    pub fn write_binary(&self, path: &str, data: &str, mtime: u64) {
        let p = norm(path);
        self.ensure_parents(&p);
        self.entries.borrow_mut().insert(
            p,
            LocalEntry {
                kind: "file".to_string(),
                text: None,
                data: Some(data.to_string()),
                mtime,
            },
        );
        self.persist();
    }

    pub fn mkdir(&self, path: &str) {
        let p = norm(path);
        if p.is_empty() {
            return;
        }
        self.ensure_parents(&p);
        self.entries.borrow_mut().entry(p).or_insert_with(|| LocalEntry {
            kind: "dir".to_string(),
            text: None,
            data: None,
            mtime: now_ms(),
        });
        self.persist();
    }

    pub fn walk(&self) -> Vec<FsEntry> {
        self.entries
            .borrow()
            .iter()
            .map(|(path, e)| FsEntry {
                path: path.clone(),
                kind: e.kind.clone(),
                mtime: e.mtime,
            })
            .collect()
    }

    pub fn rename(&self, from: &str, to: &str) {
        let a = norm(from);
        let b = norm(to);
        if a == b {
            return;
        }
        let moving: Vec<(String, LocalEntry)> = {
            let map = self.entries.borrow();
            map.iter()
                .filter(|(p, _)| *p == &a || p.starts_with(&format!("{a}/")))
                .map(|(p, e)| (p.clone(), e.clone()))
                .collect()
        };
        if moving.is_empty() {
            return;
        }
        self.ensure_parents(&b);
        {
            let mut map = self.entries.borrow_mut();
            for (p, e) in moving {
                map.remove(&p);
                let next = if p == a {
                    b.clone()
                } else {
                    format!("{b}{}", &p[a.len()..])
                };
                map.insert(next, e);
            }
        }
        self.persist();
    }

    pub fn remove(&self, path: &str) {
        let p = norm(path);
        {
            let mut map = self.entries.borrow_mut();
            let keys: Vec<String> = map
                .keys()
                .filter(|k| *k == &p || k.starts_with(&format!("{p}/")))
                .cloned()
                .collect();
            for k in keys {
                map.remove(&k);
            }
        }
        self.persist();
    }
}

/* ---------------- 统一后端抽象 ---------------- */

#[derive(Clone)]
pub enum VaultFs {
    Tauri { root: String },
    Local(LocalFs),
}

impl VaultFs {
    pub fn is_folder(&self) -> bool {
        matches!(self, Self::Tauri { .. })
    }

    pub fn root_path(&self) -> Option<&str> {
        match self {
            Self::Tauri { root } => Some(root.as_str()),
            Self::Local(_) => None,
        }
    }

    pub async fn exists(&self, rel: &str) -> Result<bool, String> {
        match self {
            Self::Tauri { .. } => {
                #[derive(Serialize)]
                struct Arg<'a> {
                    rel: &'a str,
                }
                invoke("vault_exists", &Arg { rel }).await
            }
            Self::Local(fs) => Ok(fs.exists(rel)),
        }
    }

    pub async fn read(&self, rel: &str) -> Result<Option<String>, String> {
        match self {
            Self::Tauri { .. } => {
                #[derive(Serialize)]
                struct Arg<'a> {
                    rel: &'a str,
                }
                invoke("vault_read", &Arg { rel }).await
            }
            Self::Local(fs) => Ok(fs.read(rel)),
        }
    }

    pub async fn read_binary(&self, rel: &str) -> Result<Option<String>, String> {
        match self {
            Self::Tauri { .. } => {
                #[derive(Serialize)]
                struct Arg<'a> {
                    rel: &'a str,
                }
                invoke("vault_read_binary", &Arg { rel }).await
            }
            Self::Local(fs) => Ok(fs.read_binary(rel)),
        }
    }

    pub async fn write(&self, rel: &str, text: &str, mtime: Option<u64>) -> Result<(), String> {
        match self {
            Self::Tauri { .. } => {
                #[derive(Serialize)]
                struct Arg<'a> {
                    rel: &'a str,
                    text: &'a str,
                }
                invoke_unit("vault_write", &Arg { rel, text }).await
            }
            Self::Local(fs) => {
                fs.write(rel, text, mtime.unwrap_or_else(now_ms));
                Ok(())
            }
        }
    }

    pub async fn write_binary(&self, rel: &str, data: &str) -> Result<(), String> {
        match self {
            Self::Tauri { .. } => {
                #[derive(Serialize)]
                struct Arg<'a> {
                    rel: &'a str,
                    data: &'a str,
                }
                invoke_unit("vault_write_binary", &Arg { rel, data }).await
            }
            Self::Local(fs) => {
                fs.write_binary(rel, data, now_ms());
                Ok(())
            }
        }
    }

    pub async fn mkdir(&self, rel: &str) -> Result<(), String> {
        match self {
            Self::Tauri { .. } => {
                #[derive(Serialize)]
                struct Arg<'a> {
                    rel: &'a str,
                }
                invoke_unit("vault_mkdir", &Arg { rel }).await
            }
            Self::Local(fs) => {
                fs.mkdir(rel);
                Ok(())
            }
        }
    }

    pub async fn remove(&self, rel: &str) -> Result<(), String> {
        match self {
            Self::Tauri { .. } => {
                #[derive(Serialize)]
                struct Arg<'a> {
                    rel: &'a str,
                }
                invoke_unit("vault_remove", &Arg { rel }).await
            }
            Self::Local(fs) => {
                fs.remove(rel);
                Ok(())
            }
        }
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        match self {
            Self::Tauri { .. } => {
                #[derive(Serialize)]
                struct Arg<'a> {
                    from: &'a str,
                    to: &'a str,
                }
                invoke_unit("vault_rename", &Arg { from, to }).await
            }
            Self::Local(fs) => {
                fs.rename(from, to);
                Ok(())
            }
        }
    }

    pub async fn walk(&self) -> Result<Vec<FsEntry>, String> {
        match self {
            Self::Tauri { .. } => invoke_empty("vault_walk").await,
            Self::Local(fs) => Ok(fs.walk()),
        }
    }

    pub async fn abs_path(&self, rel: &str) -> Result<String, String> {
        match self {
            Self::Tauri { .. } => {
                #[derive(Serialize)]
                struct Arg<'a> {
                    rel: &'a str,
                }
                invoke("vault_abs_path", &Arg { rel }).await
            }
            Self::Local(_) => Ok(norm(rel)),
        }
    }

    pub async fn open_path(&self, rel: &str) -> Result<(), String> {
        match self {
            Self::Tauri { .. } => {
                #[derive(Serialize)]
                struct Arg<'a> {
                    rel: &'a str,
                }
                invoke_unit("vault_open_path", &Arg { rel }).await
            }
            Self::Local(_) => Err("浏览器沙盒不支持在系统软件中打开".into()),
        }
    }

    pub async fn open_in_explorer(&self, rel: &str) -> Result<(), String> {
        match self {
            Self::Tauri { .. } => {
                #[derive(Serialize)]
                struct Arg<'a> {
                    path: &'a str,
                }
                invoke_unit("open_in_explorer", &Arg { path: rel }).await
            }
            Self::Local(_) => Err("浏览器沙盒不支持打开资源管理器".into()),
        }
    }

    pub async fn system_fonts(&self) -> Vec<String> {
        match self {
            Self::Tauri { .. } => invoke_empty("system_fonts").await.unwrap_or_default(),
            Self::Local(_) => Vec::new(),
        }
    }
}

/* ---------------- 浏览器虚拟仓库登记表 ---------------- */

const REGISTRY_KEY: &str = "inkdown.vaults.v1";
const ACTIVE_KEY: &str = "inkdown.activeVault.v1";
pub const FS_VAULT_KEY: &str = "inkdown.fsVault.v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VaultMeta {
    pub id: String,
    pub name: String,
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(rename = "createdAt", default)]
    pub created_at: u64,
}

fn default_version() -> u32 {
    1
}

pub fn list_vaults() -> Vec<VaultMeta> {
    let Some(raw) = ls_get(REGISTRY_KEY) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_vaults(list: &[VaultMeta]) {
    if let Ok(json) = serde_json::to_string(list) {
        ls_set(REGISTRY_KEY, &json);
    }
}

pub fn active_vault_id() -> String {
    ls_get(ACTIVE_KEY).unwrap_or_default()
}

pub fn set_active_vault_id(id: &str) {
    ls_set(ACTIVE_KEY, id);
}

pub fn fs_key_for(id: &str) -> String {
    format!("inkdown.vfs.{id}")
}
