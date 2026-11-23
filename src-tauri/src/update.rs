//! 更新检查：以 GitHub Releases 为准，比较版本号并取回更新说明。
//!
//! 仓库的原则是「不引入外部网络请求」，更新检查是唯一例外：只在用户点
//! 「检查更新」时联网（外加设置里可选的每日一次静默检查）。除了版本号与
//! 更新说明，不发送也不上报任何内容。

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

const API_LATEST: &str = "https://api.github.com/repos/Genlue/Inkdown/releases/latest";
const USER_AGENT: &str = concat!("Inkdown/", env!("CARGO_PKG_VERSION"));

/// 当前编译进去的版本号（四处版本号之一，与 tauri.conf.json 一致）。
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 一次检查的结果。字段名走 camelCase，前端直接用同名结构接收。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub notes: String,
    pub page_url: String,
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
    pub asset_size: u64,
    pub published_at: String,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

/// 解析 `v1.2.3` / `1.2.3` / `1.2.3-beta.1` 这类版本串：去掉 v 前缀与预发布后缀，
/// 只留三段数字。段数不足的按 0 补齐，便于比较。
pub fn parse_version(raw: &str) -> Option<[u64; 3]> {
    let s = raw.trim();
    let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
    let core = s.split(['-', '+']).next().unwrap_or(s);
    let mut out = [0u64; 3];
    let mut seen = 0usize;
    for (i, part) in core.split('.').take(3).enumerate() {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return None;
        }
        out[i] = digits.parse().ok()?;
        seen += 1;
    }
    (seen > 0).then_some(out)
}

/// 新版本是否真的更新。按数字逐段比较，所以 1.0.10 比 1.0.9 新。
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// 安装包优先取 NSIS 的 setup.exe；没有就退回发行版里的任意 exe。
fn pick_installer(assets: &[GhAsset]) -> Option<&GhAsset> {
    let is = |a: &GhAsset, suffix: &str| a.name.to_ascii_lowercase().ends_with(suffix);
    assets
        .iter()
        .find(|a| is(a, "setup.exe"))
        .or_else(|| assets.iter().find(|a| is(a, ".exe")))
}

/// 把 GitHub 的发行版 JSON 转成界面要用的结构。
pub fn release_info(json: &str, current: &str) -> Result<ReleaseInfo, String> {
    let rel: GhRelease =
        serde_json::from_str(json).map_err(|e| format!("更新信息解析失败：{e}"))?;
    let tag = rel.tag_name.trim();
    if tag.is_empty() {
        return Err("更新信息里没有版本号".into());
    }
    let installer = pick_installer(&rel.assets);
    Ok(ReleaseInfo {
        current: current.to_string(),
        latest: tag.trim_start_matches(['v', 'V']).to_string(),
        has_update: is_newer(tag, current),
        notes: rel.body.unwrap_or_default(),
        page_url: rel.html_url,
        asset_name: installer.map(|a| a.name.clone()),
        asset_url: installer
            .map(|a| a.browser_download_url.clone())
            .filter(|u| !u.is_empty()),
        asset_size: installer.map(|a| a.size).unwrap_or(0),
        published_at: rel.published_at.unwrap_or_default(),
    })
}

fn agent() -> ureq::Agent {
    // 只编进了 native-tls（Windows 上就是系统证书库），但 ureq 的默认 provider 是
    // Rustls：不显式指定的话，一遇到 https 就直接 panic「feature is not enabled」。
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into()
}

/// 问 GitHub 要最新发行版。连不上、超时、404 都在这里翻成人话。
/// 这是阻塞调用，必须放在 spawn_blocking 里跑。
pub fn fetch_latest(current: &str) -> Result<ReleaseInfo, String> {
    let mut res = agent()
        .get(API_LATEST)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(|e| format!("连不上更新服务器：{e}"))?;
    let body = res
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("读取更新信息失败：{e}"))?;
    release_info(&body, current)
}

/// 把安装包下载到 dest。total_hint 来自发行版信息里的资产大小，用来报进度；
/// 服务器没给 content-length 也不影响。
pub fn download(
    url: &str,
    dest: &Path,
    total_hint: u64,
    mut progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    use std::io::{Read, Write};

    let mut res = agent()
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("下载失败：{e}"))?;
    // 上限兜底：万一地址被换成了一个巨大的文件，不要一路写满磁盘
    let mut reader = res.body_mut().with_config().limit(512 * 1024 * 1024).reader();
    let mut file = std::fs::File::create(dest).map_err(|e| format!("写入安装包失败：{e}"))?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut received = 0u64;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("下载中断：{e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("写入安装包失败：{e}"))?;
        received += n as u64;
        progress(received, total_hint);
    }
    file.flush().map_err(|e| e.to_string())?;
    if received == 0 {
        return Err("下载到的安装包是空的".into());
    }
    Ok(())
}

/// 拉起安装包，并在装完之后把应用重新打开。
///
/// 安装程序要替换正在运行的 exe，所以这一步必须脱离本进程：起一个隐藏的 cmd，
/// 先等本进程退出并放开文件句柄，再静默安装（/S），最后重新启动应用。
/// 拼出「等一会儿 → 静默跑安装包 → 重新打开应用」的 cmd 脚本。
///
/// 两处路径都加了引号：默认安装目录在用户目录下，用户名带空格是常态。
pub fn installer_script(setup: &Path, exe: &Path) -> String {
    format!(
        "timeout /t 2 /nobreak >nul & start \"\" /wait \"{}\" /S & start \"\" \"{}\"",
        setup.display(),
        exe.display()
    )
}

#[cfg(target_os = "windows")]
pub fn run_installer_then_restart(setup: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let exe = std::env::current_exe().map_err(|e| format!("找不到当前程序路径：{e}"))?;
    let script = installer_script(setup, &exe);
    std::process::Command::new("cmd")
        .args(["/c", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("启动安装程序失败：{e}"))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn run_installer_then_restart(_setup: &Path) -> Result<(), String> {
    Err("当前平台还不支持自动安装更新".into())
}

#[cfg(test)]
mod update_tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("v1.0.10"), Some([1, 0, 10]));
        assert_eq!(parse_version("1.2.3"), Some([1, 2, 3]));
        assert_eq!(parse_version("1.2.3-beta.4"), Some([1, 2, 3]));
        assert_eq!(parse_version("2"), Some([2, 0, 0]));
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("夜版"), None);
    }

    #[test]
    fn compares_numerically_not_lexically() {
        assert!(is_newer("v1.0.10", "1.0.9"), "1.0.10 应该比 1.0.9 新");
        assert!(!is_newer("v1.0.9", "1.0.10"));
        assert!(!is_newer("v1.0.10", "1.0.10"), "同版本不算有更新");
    }

    const SAMPLE: &str = r#"{
        "tag_name": "v1.0.11",
        "html_url": "https://github.com/Genlue/Inkdown/releases/tag/v1.0.11",
        "published_at": "2026-11-23T04:00:00Z",
        "body": "修掉启动闪窗，重写沉浸设置",
        "assets": [
            { "name": "Inkdown_1.0.11_windows-x64-portable.zip", "size": 100, "browser_download_url": "https://example.com/portable.zip" },
            { "name": "Inkdown_1.0.11_x64-setup.exe", "size": 200, "browser_download_url": "https://example.com/setup.exe" }
        ]
    }"#;

    #[test]
    fn reads_release_json() {
        let info = release_info(SAMPLE, "1.0.10").expect("样例应当可解析");
        assert!(info.has_update);
        assert_eq!(info.latest, "1.0.11");
        assert_eq!(info.notes, "修掉启动闪窗，重写沉浸设置");
        assert_eq!(info.asset_name.as_deref(), Some("Inkdown_1.0.11_x64-setup.exe"));
        assert_eq!(info.asset_url.as_deref(), Some("https://example.com/setup.exe"));
        assert_eq!(info.asset_size, 200);
    }

    #[test]
    fn same_release_has_no_update() {
        let info = release_info(SAMPLE, "1.0.11").expect("样例应当可解析");
        assert!(!info.has_update);
    }


    /// 真联网的一条：默认不跑，需要时 `cargo test -p inkdown --lib -- --ignored`。
    #[test]
    #[ignore = "需要网络"]
    fn fetches_live_release() {
        let info = fetch_latest(current_version()).expect("应当能连上 GitHub Releases");
        println!(
            "current={} latest={} has_update={} asset={:?} size={}",
            info.current, info.latest, info.has_update, info.asset_name, info.asset_size
        );
        assert!(!info.latest.is_empty(), "最新版本号不该为空");
        assert!(
            info.asset_url.is_none()
                || info.asset_url.as_deref().unwrap().contains("https://github.com/"),
            "安装包地址应当来自 GitHub"
        );

        // 同一个响应，把自己当成更老的版本时必须报「有新版本」
        let older = fetch_latest("1.0.0").expect("第二次请求应当成功");
        assert!(older.has_update, "1.0.0 相对最新版应当有新版本");
        let url = older.asset_url.clone().expect("发行版应当带安装包");

        // 真把安装包拉下来，校验字节数与发行版声明一致
        let dest = std::env::temp_dir().join("inkdown-update-test.exe");
        let _ = std::fs::remove_file(&dest);
        let mut last = 0u64;
        download(&url, &dest, older.asset_size, |received, _total| last = received)
            .expect("安装包应当能下载");
        let bytes = std::fs::metadata(&dest).expect("下载后文件应当存在").len();
        println!("downloaded {bytes} bytes (asset size {})", older.asset_size);
        assert!(bytes > 1_000_000, "安装包不该这么小：{bytes}");
        assert_eq!(bytes, last, "进度回调的最终值应当等于文件大小");
        if older.asset_size > 0 {
            assert_eq!(bytes, older.asset_size, "下载字节数应当与发行版声明一致");
        }
        let _ = std::fs::remove_file(&dest);
    }


    #[test]
    fn installer_script_quotes_paths() {
        let script = installer_script(
            Path::new(r"C:\Users\A B\AppData\Local\Inkdown\updates\Inkdown_1.0.11_x64-setup.exe"),
            Path::new(r"C:\Users\A B\AppData\Local\Inkdown\inkdown.exe"),
        );
        assert!(script.contains("timeout /t 2"), "先等本进程退出");
        assert!(
            script.contains(r#"/S"#),
            "安装包要静默运行，否则会卡在向导上等点击"
        );
        assert_eq!(
            script.matches('"').count(),
            8,
            "两个 start 各自的两个空标题引号 + 两条带引号路径"
        );
        assert!(script.contains(r#""C:\Users\A B\AppData\Local\Inkdown\inkdown.exe""#));
    }

    #[test]
    fn rejects_json_without_tag() {
        assert!(release_info(r#"{"body": "x"}"#, "1.0.10").is_err());
    }
}
