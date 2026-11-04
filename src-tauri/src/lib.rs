use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;
use tauri::{State, WebviewWindow};

/// 当前仓库的根目录。所有文件命令都只接受相对路径。
struct VaultRoot(Mutex<Option<PathBuf>>);

#[derive(Serialize)]
struct Entry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    mtime: u64,
}

fn root_of(state: &State<'_, VaultRoot>) -> Result<PathBuf, String> {
    state
        .0
        .lock()
        .map_err(|_| "仓库状态损坏".to_string())?
        .clone()
        .ok_or_else(|| "还没有打开仓库".to_string())
}

/// 把相对路径拼到仓库根上。拒绝 `..`、盘符、UNC 和空段。
fn resolve(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.replace('\\', "/");
    let rel = rel.trim_start_matches('/');

    if rel.is_empty() {
        return Ok(root.to_path_buf());
    }

    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err("非法路径".into());
        }
        if part.contains(':') || part.contains('\0') {
            return Err("非法路径".into());
        }
    }

    Ok(root.join(rel))
}

fn mtime_of(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<Entry>, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Ok(());
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries.flatten() {
        let path = entry.path();

        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_symlink() {
            continue;
        }

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let rel = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");

        if meta.is_dir() {
            out.push(Entry {
                path: rel,
                kind: "dir".into(),
                mtime: mtime_of(&meta),
            });
            walk(root, &path, out, depth + 1)?;
        } else if meta.is_file() {
            out.push(Entry {
                path: rel,
                kind: "file".into(),
                mtime: mtime_of(&meta),
            });
        }
    }

    Ok(())
}

/* ---------------- 仓库 ---------------- */

#[tauri::command]
async fn vault_pick() -> Option<String> {
    rfd::AsyncFileDialog::new()
        .set_title("选择仓库文件夹")
        .pick_folder()
        .await
        .map(|handle| handle.path().to_string_lossy().to_string())
}

/// 开发/命令行入口：INKDOWN_VAULT 指向一个文件夹时直接打开它。
#[tauri::command]
fn vault_boot_path() -> Option<String> {
    if let Ok(p) = std::env::var("INKDOWN_VAULT") {
        let p = p.trim().to_string();
        if !p.is_empty() && Path::new(&p).is_dir() {
            return Some(p);
        }
    }
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        let docs = PathBuf::from(user_profile).join("Documents").join("Inkdown Vault");
        if !docs.exists() {
            let _ = std::fs::create_dir_all(&docs);
        }
        return Some(docs.to_string_lossy().to_string());
    }
    None
}

#[tauri::command]
fn vault_get_root(state: State<'_, VaultRoot>) -> Result<Option<String>, String> {
    let guard = state.0.lock().map_err(|_| "仓库状态损坏".to_string())?;
    Ok(guard.as_ref().map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
fn vault_open(path: String, state: State<'_, VaultRoot>) -> Result<String, String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err("这不是一个文件夹".into());
    }

    *state
        .0
        .lock()
        .map_err(|_| "仓库状态损坏".to_string())? = Some(root.clone());

    Ok(root.to_string_lossy().to_string())
}

#[tauri::command]
fn vault_walk(state: State<'_, VaultRoot>) -> Result<Vec<Entry>, String> {
    let root = root_of(&state)?;
    let mut out = Vec::new();
    walk(&root, &root, &mut out, 0)?;
    Ok(out)
}

#[tauri::command]
fn vault_read(rel: String, state: State<'_, VaultRoot>) -> Result<Option<String>, String> {
    let root = root_of(&state)?;
    let path = resolve(&root, &rel)?;
    if !path.is_file() {
        return Ok(None);
    }
    fs::read_to_string(&path).map(Some).map_err(|e| e.to_string())
}

const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn to_base64(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        out.push(B64_CHARS[(b0 >> 2) as usize] as char);
        out.push(B64_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_CHARS[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_CHARS[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}


/// 安全原子写入：写入临时文件 -> sync_all 强制刷盘 -> 原子替换，防止系统断电/崩溃损坏原始文件
fn atomic_write_bytes(target: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let tmp_path = target.with_extension(format!("tmp.{}", std::process::id()));
    {
        use std::io::Write;
        let mut file = fs::File::create(&tmp_path).map_err(|e| e.to_string())?;
        file.write_all(data).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        if target.exists() {
            let _ = fs::remove_file(target);
        }
    }

    fs::rename(&tmp_path, target).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        e.to_string()
    })?;

    Ok(())
}

fn from_base64(s: &str) -> Result<Vec<u8>, String> {
    let clean = s.trim();
    let raw = if let Some(idx) = clean.find(";base64,") {
        &clean[idx + 8..]
    } else {
        clean
    };
    let mut table = [255u8; 256];
    for (i, &c) in B64_CHARS.iter().enumerate() {
        table[c as usize] = i as u8;
    }
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in raw.as_bytes() {
        if b == b'=' || b.is_ascii_whitespace() { continue; }
        let val = table[b as usize];
        if val == 255 { return Err("Invalid base64 character".into()); }
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

#[tauri::command]
fn vault_read_binary(rel: String, state: State<'_, VaultRoot>) -> Result<Option<String>, String> {
    let root = root_of(&state)?;
    let path = resolve(&root, &rel)?;
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    Ok(Some(to_base64(&bytes)))
}

#[tauri::command]
fn vault_write_binary(rel: String, data: String, state: State<'_, VaultRoot>) -> Result<(), String> {
    let root = root_of(&state)?;
    let path = resolve(&root, &rel)?;
    let bytes = from_base64(&data)?;
    atomic_write_bytes(&path, &bytes)
}

#[tauri::command]
fn vault_abs_path(rel: String, state: State<'_, VaultRoot>) -> Result<String, String> {
    let root = root_of(&state)?;
    let path = resolve(&root, &rel)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn vault_open_path(rel: String, state: State<'_, VaultRoot>) -> Result<(), String> {
    let root = root_of(&state)?;
    let path = resolve(&root, &rel)?;
    opener::open(path).map_err(|e| e.to_string())
}

#[tauri::command]
fn vault_write(rel: String, text: String, state: State<'_, VaultRoot>) -> Result<(), String> {
    let root = root_of(&state)?;
    let path = resolve(&root, &rel)?;
    atomic_write_bytes(&path, text.as_bytes())
}

#[tauri::command]
fn vault_mkdir(rel: String, state: State<'_, VaultRoot>) -> Result<(), String> {
    let root = root_of(&state)?;
    let path = resolve(&root, &rel)?;
    fs::create_dir_all(path).map_err(|e| e.to_string())
}

#[tauri::command]
fn vault_remove(rel: String, state: State<'_, VaultRoot>) -> Result<(), String> {
    let root = root_of(&state)?;
    let path = resolve(&root, &rel)?;

    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(path).map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn vault_rename(from: String, to: String, state: State<'_, VaultRoot>) -> Result<(), String> {
    let root = root_of(&state)?;
    let src = resolve(&root, &from)?;
    let dst = resolve(&root, &to)?;

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::rename(src, dst).map_err(|e| e.to_string())
}

#[tauri::command]
fn vault_exists(rel: String, state: State<'_, VaultRoot>) -> Result<bool, String> {
    let root = root_of(&state)?;
    Ok(resolve(&root, &rel)?.exists())
}

/* ---------------- 系统与字体 ---------------- */

#[tauri::command]
fn system_fonts() -> Vec<String> {
    use std::collections::BTreeSet;
    let mut families = BTreeSet::new();

    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("reg")
            .args(["query", "HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if !line.contains("REG_SZ") { continue; }
                let parts: Vec<&str> = line.split("REG_SZ").collect();
                if parts.is_empty() { continue; }
                let mut name = parts[0].trim();
                if let Some(idx) = name.rfind('(') {
                    let suffix = &name[idx..];
                    if suffix.contains("TrueType") || suffix.contains("OpenType") || suffix.contains("PostScript") {
                        name = name[..idx].trim();
                    }
                }
                let styles = [
                    " Regular", " Bold", " Italic", " Light", " Semibold", " Semi-Bold",
                    " Black", " Medium", " Thin", " ExtraLight", " ExtraBold", " Heavy",
                    " Oblique", " Condensed", " SemiCondensed", " DemiBold", " Demibold"
                ];
                let mut cleaned = name.to_string();
                let mut changed = true;
                while changed {
                    changed = false;
                    for style in &styles {
                        if cleaned.ends_with(style) {
                            cleaned.truncate(cleaned.len() - style.len());
                            cleaned = cleaned.trim().to_string();
                            changed = true;
                            break;
                        }
                    }
                }
                if cleaned.contains('&') {
                    for sub in cleaned.split('&') {
                        let sub = sub.trim();
                        if !sub.is_empty() {
                            families.insert(sub.to_string());
                        }
                    }
                } else if !cleaned.is_empty() {
                    families.insert(cleaned);
                }
            }
        }
    }

    if families.is_empty() {
        for f in ["Microsoft YaHei", "Segoe UI", "PingFang SC", "Cascadia Code", "Consolas", "Arial", "SimSun"] {
            families.insert(f.to_string());
        }
    }

    families.into_iter().collect()
}

#[tauri::command]
fn open_in_explorer(path: String, state: State<'_, VaultRoot>) -> Result<(), String> {
    let root = root_of(&state)?;
    let target = resolve(&root, &path)?;

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let win_path = target.to_string_lossy().replace('/', "\\");
        if target.is_file() {
            Command::new("explorer")
                .arg("/select,")
                .arg(&win_path)
                .spawn()
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
    }

    opener::open(&target).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:")) {
        return Err("只支持 http / https / mailto 链接".into());
    }
    opener::open(url).map_err(|e| e.to_string())
}

#[tauri::command]
fn window_set_titlebar(window: WebviewWindow, mode: String) -> Result<(), String> {
    window
        .set_decorations(mode == "native")
        .map_err(|e| e.to_string())
}

/// 窗口主题。DWM 的 Acrylic/Mica 染色跟着窗口主题走，不跟 CSS 走 ——
/// 不设这一条，深色界面下系统材质仍然是浅色，导航栏会白得像一块纸。
#[tauri::command]
fn window_set_theme(window: WebviewWindow, theme: String) -> Result<(), String> {
    let t = match theme.as_str() {
        "dark" => tauri::Theme::Dark,
        "light" => tauri::Theme::Light,
        _ => return Ok(()),
    };
    window.set_theme(Some(t)).map_err(|e| e.to_string())
}

/// 真实系统材质入口（Windows DWM Acrylic / Mica / Blur）
#[tauri::command]
fn window_set_effect(
    window: WebviewWindow,
    enabled: bool,
    effect_type: Option<String>,
) -> Result<(), String> {
    use tauri::window::{EffectsBuilder, Effect};
    if enabled {
        let eff = match effect_type.as_deref() {
            Some("mica") => Effect::Mica,
            Some("blur") => Effect::Blur,
            _ => Effect::Acrylic,
        };
        let effects = EffectsBuilder::new().effect(eff).build();
        window.set_effects(Some(effects)).map_err(|e| e.to_string())
    } else {
        window.set_effects(None).map_err(|e| e.to_string())
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(VaultRoot(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            vault_pick,
            vault_boot_path,
            vault_get_root,
            vault_open,
            vault_walk,
            vault_read,
            vault_read_binary,
            vault_write,
            vault_write_binary,
            vault_abs_path,
            vault_open_path,
            vault_mkdir,
            vault_remove,
            vault_rename,
            vault_exists,
            system_fonts,
            open_in_explorer,
            open_url,
            window_set_titlebar,
            window_set_theme,
            window_set_effect
        ])
        .run(tauri::generate_context!())
        .expect("Inkdown 启动失败");
}
