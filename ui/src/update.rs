//! 更新检查：界面状态与宿主调用。
//!
//! 真正的联网与版本比较都在 Rust 宿主里（src-tauri/src/update.rs），
//! 前端只负责把结果变成可显示的状态。

use serde::{Deserialize, Serialize};

use crate::tauri_bridge::{invoke, invoke_empty, invoke_unit, is_tauri};

/// 一次检查的结果。字段与宿主 update::ReleaseInfo 一一对应。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
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

/// 设置页要显示的检查状态。
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CheckState {
    #[default]
    Idle,
    Checking,
    Latest(ReleaseInfo),
    Available(ReleaseInfo),
    Failed(String),
}

impl CheckState {
    pub fn is_checking(&self) -> bool {
        matches!(self, Self::Checking)
    }

    /// 状态行文案。放在版本那一行的说明里，用户一眼能看到结果。
    pub fn summary(&self) -> String {
        match self {
            Self::Idle => "点右侧按钮向 GitHub Releases 查询最新版本".to_string(),
            Self::Checking => "正在检查…".to_string(),
            Self::Latest(info) => format!("已是最新版本（v{}）", info.current),
            Self::Available(info) => format!("发现新版本 v{}，当前 v{}", info.latest, info.current),
            Self::Failed(err) => format!("检查失败：{err}"),
        }
    }
}

/// 问宿主要最新发行版。浏览器预览没有这个能力，直接给一句人话。
pub async fn check() -> Result<ReleaseInfo, String> {
    if !is_tauri() {
        return Err("浏览器预览不检查更新".into());
    }
    invoke_empty::<ReleaseInfo>("update_check").await
}

/// 把字节数说成 MB，用于显示安装包大小。
pub fn human_size(bytes: u64) -> String {
    if bytes == 0 {
        return String::new();
    }
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

/// 下载/安装进度。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Busy {
    pub stage: String,
    pub received: u64,
    pub total: u64,
}

impl Busy {
    pub fn percent(&self) -> Option<f64> {
        (self.total > 0).then(|| (self.received as f64 / self.total as f64).clamp(0.0, 1.0))
    }

    /// 有总量就带百分比，没有就只显示阶段。
    pub fn label(&self) -> String {
        match self.percent() {
            Some(p) => format!("{} {:.0}%", self.stage, p * 100.0),
            None => self.stage.clone(),
        }
    }
}

/// 把安装包下载到宿主缓存目录，返回本地路径。
pub async fn download(url: &str, name: &str, total: u64) -> Result<String, String> {
    #[derive(Serialize)]
    struct Arg<'a> {
        url: &'a str,
        name: &'a str,
        total: u64,
    }
    invoke::<_, String>("update_download", &Arg { url, name, total }).await
}

/// 让宿主拉起安装包并退出应用。
pub async fn install(path: &str) -> Result<(), String> {
    #[derive(Serialize)]
    struct Arg<'a> {
        path: &'a str,
    }
    invoke_unit("update_install", &Arg { path }).await
}

/// 记住「今天是否已经静默检查过」的键。
const CHECKED_KEY: &str = "inkdown.update.checkedOn";

/// 本地日期串 `YYYY-MM-DD`。用本地时区算「今天」，跨时区也不会错一天。
pub fn today_key() -> String {
    let d = js_sys::Date::new_0();
    format!(
        "{:04}-{:02}-{:02}",
        d.get_full_year(),
        d.get_month() + 1,
        d.get_date()
    )
}

/// 今天是否已经静默检查过。
pub fn checked_today() -> bool {
    crate::tauri_bridge::ls_get(CHECKED_KEY).as_deref() == Some(today_key().as_str())
}

/// 记下今天已经查过。成功失败都记，保证一天最多联网一次。
pub fn mark_checked() {
    crate::tauri_bridge::ls_set(CHECKED_KEY, &today_key());
}