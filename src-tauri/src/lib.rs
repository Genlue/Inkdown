use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Mutex;
use std::time::{Duration, Instant, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State, WebviewWindow};

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

/* ---------------- 仓库监听 ---------------- */

/// 一批事件最多带这么多条路径，剩下的只靠 truncated 标记让前端知道「还有」。
const MAX_EVENT_PATHS: usize = 50;
/// 合并窗口：这段时间内的事件攒成一批再发，避免一次保存刷屏。
const WATCH_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Default)]
struct WatchSlot {
    watcher: Option<RecommendedWatcher>,
    root: Option<PathBuf>,
}

/// 监听器句柄。重复打开仓库时先丢掉旧的 —— Drop 会停掉底层线程。
struct FsWatch(Mutex<WatchSlot>);

impl FsWatch {
    fn new() -> Self {
        FsWatch(Mutex::new(WatchSlot::default()))
    }
}

/// 这些路径完全不该出现在事件里：
/// - 应用自己的元数据 / 回收站 / git 内部状态（写 workspace.json 会自激）
/// - 原子写的临时文件、编辑器备份、系统垃圾文件
fn is_ignored_path(rel: &str) -> bool {
    for part in rel.split('/') {
        // Windows 上目录名大小写不敏感，这里也别只认小写
        if part.eq_ignore_ascii_case(".inkdown")
            || part.eq_ignore_ascii_case(".trash")
            || part.eq_ignore_ascii_case(".git")
        {
            return true;
        }
        if part.eq_ignore_ascii_case(".DS_Store") || part.eq_ignore_ascii_case("Thumbs.db") {
            return true;
        }
        if part.ends_with('~') || part.contains(".tmp.") {
            return true;
        }
    }
    false
}

/// 绝对路径 -> 相对仓库根、`/` 分隔。根目录之外和被过滤的路径都返回 None。
fn relative_event_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let rel = rel.to_string_lossy().replace('\\', "/");
    let rel = rel.trim_matches('/');
    if rel.is_empty() || is_ignored_path(rel) {
        return None;
    }
    Some(rel.to_string())
}

/// 记一条事件路径。BTreeSet 顺手完成「同一路径只留一次」。
fn note_event_path(root: &Path, path: &Path, out: &mut BTreeSet<String>) {
    if let Some(rel) = relative_event_path(root, path) {
        out.insert(rel);
    }
}

/// 取出一批事件路径：去重（BTreeSet）、排序、截断，并返回是否发生了截断。
fn take_batch(pending: &mut BTreeSet<String>) -> (Vec<String>, bool) {
    let truncated = pending.len() > MAX_EVENT_PATHS;
    let paths = pending.iter().take(MAX_EVENT_PATHS).cloned().collect();
    pending.clear();
    (paths, truncated)
}

fn emit_batch(app: &AppHandle, pending: &mut BTreeSet<String>) {
    let (paths, truncated) = take_batch(pending);
    if paths.is_empty() {
        return;
    }
    let payload = serde_json::json!({
        "paths": paths,
        "kind": "any",
        "truncated": truncated,
    });
    let _ = app.emit("vault://fs-change", payload);
}

/// 去抖线程：把 250ms 窗口内的原始事件合并成一批。
/// watcher 被丢掉后 channel 会断开，线程把尾巴发完就退出。
fn spawn_debounce(app: AppHandle, root: PathBuf, rx: mpsc::Receiver<notify::Result<Event>>) {
    let spawned = std::thread::Builder::new()
        .name("inkdown-vault-watch".into())
        .spawn(move || {
            let mut pending: BTreeSet<String> = BTreeSet::new();
            let mut deadline: Option<Instant> = None;

            loop {
                // 没有待发事件时用长超时阻塞，别空转
                let wait = deadline
                    .map(|d| d.saturating_duration_since(Instant::now()))
                    .unwrap_or(Duration::from_secs(3600));

                match rx.recv_timeout(wait) {
                    Ok(Ok(event)) => {
                        for path in &event.paths {
                            note_event_path(&root, path, &mut pending);
                        }
                        if !pending.is_empty() && deadline.is_none() {
                            // 从第一条有效事件开始计时：持续写入时也不会无限推迟
                            deadline = Some(Instant::now() + WATCH_DEBOUNCE);
                        }
                    }
                    Ok(Err(_)) => {}
                    Err(RecvTimeoutError::Timeout) => {
                        if deadline.take().is_some() {
                            emit_batch(&app, &mut pending);
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        emit_batch(&app, &mut pending);
                        return;
                    }
                }
            }
        });

    if let Err(err) = spawned {
        eprintln!("[watch] 去抖线程启动失败：{err}");
    }
}

/// 开始监听（重复调用会先停掉旧的）。监听失败不阻断打开仓库：
/// 网络盘 / 权限不足时 notify 会报错，但读写本身仍然可用。
fn start_watch(app: &AppHandle, watch: &FsWatch, root: &Path) -> Result<(), String> {
    let mut slot = watch.0.lock().map_err(|_| "监听状态损坏".to_string())?;
    slot.watcher = None;
    slot.root = None;

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })
    .map_err(|e| format!("创建监听器失败：{e}"))?;

    watcher
        .watch(root, RecursiveMode::Recursive)
        .map_err(|e| format!("监听仓库失败：{e}"))?;

    slot.watcher = Some(watcher);
    slot.root = Some(root.to_path_buf());
    drop(slot);

    spawn_debounce(app.clone(), root.to_path_buf(), rx);
    Ok(())
}

#[derive(Serialize)]
struct WatchStatus {
    watching: bool,
    root: Option<String>,
}

/// 给前端确认 / 排错用：现在到底有没有在听，听的是哪个根。
#[tauri::command]
fn vault_watch_status(watch: State<'_, FsWatch>) -> WatchStatus {
    match watch.0.lock() {
        Ok(slot) => WatchStatus {
            watching: slot.watcher.is_some(),
            root: slot.root.as_ref().map(|p| p.to_string_lossy().to_string()),
        },
        Err(_) => WatchStatus {
            watching: false,
            root: None,
        },
    }
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
fn vault_open(
    app: AppHandle,
    path: String,
    state: State<'_, VaultRoot>,
    watch: State<'_, FsWatch>,
) -> Result<String, String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err("这不是一个文件夹".into());
    }

    *state
        .0
        .lock()
        .map_err(|_| "仓库状态损坏".to_string())? = Some(root.clone());

    // 监听失败只写日志：仓库还能正常打开，前端可以用手动刷新兜底
    if let Err(err) = start_watch(&app, &watch, &root) {
        eprintln!("[watch] {err}");
    }

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
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let val = table[b as usize];
        if val == 255 {
            return Err("Invalid base64 character".into());
        }
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

/* ---------------- 系统与窗口 ---------------- */

#[tauri::command]
fn system_fonts() -> Vec<String> {
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
                if !line.contains("REG_SZ") {
                    continue;
                }
                let parts: Vec<&str> = line.split("REG_SZ").collect();
                if parts.is_empty() {
                    continue;
                }
                let mut name = parts[0].trim();
                if let Some(idx) = name.rfind('(') {
                    let suffix = &name[idx..];
                    if suffix.contains("TrueType")
                        || suffix.contains("OpenType")
                        || suffix.contains("PostScript")
                    {
                        name = name[..idx].trim();
                    }
                }
                let styles = [
                    " Regular",
                    " Bold",
                    " Italic",
                    " Light",
                    " Semibold",
                    " Semi-Bold",
                    " Black",
                    " Medium",
                    " Thin",
                    " ExtraLight",
                    " ExtraBold",
                    " Heavy",
                    " Oblique",
                    " Condensed",
                    " SemiCondensed",
                    " DemiBold",
                    " Demibold",
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
        for f in [
            "Microsoft YaHei",
            "Segoe UI",
            "PingFang SC",
            "Cascadia Code",
            "Consolas",
            "Arial",
            "SimSun",
        ] {
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
fn window_set_theme(window: WebviewWindow, theme: String) -> Result<(), String> {
    match theme.as_str() {
        "dark" => window.set_theme(Some(tauri::Theme::Dark)),
        "light" => window.set_theme(Some(tauri::Theme::Light)),
        _ => window.set_theme(None),
    }
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn window_get_theme(window: WebviewWindow) -> Option<String> {
    window.theme().ok().map(|t| match t {
        tauri::Theme::Dark => "dark".to_string(),
        _ => "light".to_string(),
    })
}

#[tauri::command]
fn window_minimize(window: WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
fn window_toggle_maximize(window: WebviewWindow) -> Result<bool, String> {
    let maximized = window.is_maximized().map_err(|e| e.to_string())?;
    if maximized {
        window.unmaximize().map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        window.maximize().map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[tauri::command]
fn window_is_maximized(window: WebviewWindow) -> bool {
    window.is_maximized().unwrap_or(false)
}

#[tauri::command]
fn window_close(window: WebviewWindow) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

#[cfg(test)]
mod watch_tests {
    use super::*;
    use std::collections::BTreeSet;

    fn root() -> PathBuf {
        PathBuf::from("vault-root")
    }

    #[test]
    fn drops_app_internal_dirs() {
        let r = root();
        for rel in [
            ".inkdown/workspace.json",
            ".trash/1-旧笔记.md",
            ".git/index",
            ".Inkdown/x.json",
        ] {
            assert_eq!(
                relative_event_path(&r, &r.join(rel)),
                None,
                "{rel} 不该被上报"
            );
        }
        assert_eq!(
            relative_event_path(&r, &r.join("Notes/欢迎.md")),
            Some("Notes/欢迎.md".into())
        );
    }

    #[test]
    fn drops_atomic_temp_and_noise_files() {
        let r = root();
        for rel in [
            "Notes/a.tmp.4242",
            "Notes/a.md~",
            "Notes/.DS_Store",
            "Notes/Thumbs.db",
        ] {
            assert_eq!(
                relative_event_path(&r, &r.join(rel)),
                None,
                "{rel} 不该被上报"
            );
        }
        assert_eq!(
            relative_event_path(&r, &r.join("Notes/b.md")),
            Some("Notes/b.md".into())
        );
    }

    #[test]
    fn merges_duplicates_and_truncates() {
        let r = root();
        let mut pending = BTreeSet::new();

        note_event_path(&r, &r.join("Notes/a.md"), &mut pending);
        note_event_path(&r, &r.join("Notes/a.md"), &mut pending);
        assert_eq!(pending.len(), 1, "同一路径只保留一次");

        for i in 0..80 {
            note_event_path(&r, &r.join(format!("Notes/{i}.md")), &mut pending);
        }
        let (paths, truncated) = take_batch(&mut pending);
        assert_eq!(paths.len(), MAX_EVENT_PATHS);
        assert!(truncated);
        assert!(pending.is_empty(), "取走后必须清空，否则下一批会重复上报");
    }

    #[test]
    fn ignores_paths_outside_root() {
        let r = root();
        assert_eq!(relative_event_path(&r, Path::new("other/notes/a.md")), None);
        assert_eq!(relative_event_path(&r, &r), None);
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(VaultRoot(Mutex::new(None)))
        .manage(FsWatch::new())
        .setup(|app| {
            glass_design_host::install(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            glass_design_host::capture_backdrop,
            vault_pick,
            vault_boot_path,
            vault_get_root,
            vault_open,
            vault_watch_status,
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
            window_set_theme,
            window_get_theme,
            window_minimize,
            window_toggle_maximize,
            window_is_maximized,
            window_close,
        ])
        .run(tauri::generate_context!())
        .expect("Inkdown 启动失败");
}
