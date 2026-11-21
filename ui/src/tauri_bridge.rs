//! Tauri 宿主桥接与浏览器降级工具。
//!
//! 只通过 `window.__TAURI__` 与桌面宿主通信；浏览器预览环境下自动降级，
//! 保证同一套 WASM 前端可在桌面端与静态预览服务器同时运行。

use js_sys::{Function, Promise, Reflect};
use serde::de::DeserializeOwned;
use serde::Serialize;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

/// 是否运行在 Tauri 桌面宿主中。
pub fn is_tauri() -> bool {
    let Some(win) = web_sys::window() else {
        return false;
    };
    let Ok(tauri) = Reflect::get(&win, &JsValue::from_str("__TAURI__")) else {
        return false;
    };
    if tauri.is_undefined() || tauri.is_null() {
        return false;
    }
    let Ok(core) = Reflect::get(&tauri, &JsValue::from_str("core")) else {
        return false;
    };
    Reflect::get(&core, &JsValue::from_str("invoke"))
        .map(|f| f.is_function())
        .unwrap_or(false)
}

fn tauri_core_invoke() -> Result<(JsValue, Function), String> {
    let win = web_sys::window().ok_or_else(|| "缺少 window".to_string())?;
    let tauri = Reflect::get(&win, &JsValue::from_str("__TAURI__"))
        .map_err(|_| "未检测到 __TAURI__".to_string())?;
    let core = Reflect::get(&tauri, &JsValue::from_str("core"))
        .map_err(|_| "未检测到 __TAURI__.core".to_string())?;
    let invoke = Reflect::get(&core, &JsValue::from_str("invoke"))
        .map_err(|_| "未检测到 invoke".to_string())?
        .dyn_into::<Function>()
        .map_err(|_| "invoke 不是函数".to_string())?;
    Ok((core, invoke))
}

fn js_err_to_string(err: JsValue) -> String {
    if let Some(s) = err.as_string() {
        return s;
    }
    if let Ok(msg) = Reflect::get(&err, &JsValue::from_str("message")) {
        if let Some(s) = msg.as_string() {
            return s;
        }
    }
    format!("{err:?}")
}

/// 调用无返回值或 JSON 返回值的 Tauri 命令。
pub async fn invoke<A: Serialize, R: DeserializeOwned>(cmd: &str, args: &A) -> Result<R, String> {
    let (core, func) = tauri_core_invoke()?;
    let js_args = serde_wasm_to_js(args)?;
    let ret = func
        .call2(&core, &JsValue::from_str(cmd), &js_args)
        .map_err(js_err_to_string)?;
    let promise = ret
        .dyn_into::<Promise>()
        .map_err(|_| "invoke 未返回 Promise".to_string())?;
    let resolved = JsFuture::from(promise).await.map_err(js_err_to_string)?;
    js_to_serde(&resolved)
}

/// 调用无参数 Tauri 命令。
pub async fn invoke_empty<R: DeserializeOwned>(cmd: &str) -> Result<R, String> {
    let (core, func) = tauri_core_invoke()?;
    let ret = func
        .call1(&core, &JsValue::from_str(cmd))
        .map_err(js_err_to_string)?;
    let promise = ret
        .dyn_into::<Promise>()
        .map_err(|_| "invoke 未返回 Promise".to_string())?;
    let resolved = JsFuture::from(promise).await.map_err(js_err_to_string)?;
    js_to_serde(&resolved)
}

/// 调用返回 `()` 的 Tauri 命令（允许后端返回 `undefined`/`null`）。
pub async fn invoke_unit<A: Serialize>(cmd: &str, args: &A) -> Result<(), String> {
    let (core, func) = tauri_core_invoke()?;
    let js_args = serde_wasm_to_js(args)?;
    let ret = func
        .call2(&core, &JsValue::from_str(cmd), &js_args)
        .map_err(js_err_to_string)?;
    let promise = ret
        .dyn_into::<Promise>()
        .map_err(|_| "invoke 未返回 Promise".to_string())?;
    let _ = JsFuture::from(promise).await.map_err(js_err_to_string)?;
    Ok(())
}

/// 调用无参数且返回 `()` 的 Tauri 命令。
pub async fn invoke_empty_unit(cmd: &str) -> Result<(), String> {
    let (core, func) = tauri_core_invoke()?;
    let ret = func
        .call1(&core, &JsValue::from_str(cmd))
        .map_err(js_err_to_string)?;
    let promise = ret
        .dyn_into::<Promise>()
        .map_err(|_| "invoke 未返回 Promise".to_string())?;
    let _ = JsFuture::from(promise).await.map_err(js_err_to_string)?;
    Ok(())
}

fn serde_wasm_to_js<T: Serialize>(value: &T) -> Result<JsValue, String> {
    let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
    js_sys::JSON::parse(&json).map_err(js_err_to_string)
}

fn js_to_serde<R: DeserializeOwned>(value: &JsValue) -> Result<R, String> {
    if value.is_undefined() || value.is_null() {
        return serde_json::from_str("null").map_err(|e| e.to_string());
    }
    let json = js_sys::JSON::stringify(value)
        .map_err(js_err_to_string)?
        .as_string()
        .unwrap_or_else(|| "null".to_string());
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

/// 监听 Tauri 后端事件（如 `vault://fs-change`）。
pub fn listen_tauri_event<F>(event_name: &str, mut handler: F)
where
    F: FnMut(serde_json::Value) + 'static,
{
    let Some(win) = web_sys::window() else {
        return;
    };
    let Ok(tauri) = Reflect::get(&win, &JsValue::from_str("__TAURI__")) else {
        return;
    };
    let Ok(event_mod) = Reflect::get(&tauri, &JsValue::from_str("event")) else {
        return;
    };
    let Ok(listen_val) = Reflect::get(&event_mod, &JsValue::from_str("listen")) else {
        return;
    };
    let Ok(listen_fn) = listen_val.dyn_into::<Function>() else {
        return;
    };

    let cb = Closure::wrap(Box::new(move |ev: JsValue| {
        let payload = Reflect::get(&ev, &JsValue::from_str("payload")).unwrap_or(JsValue::NULL);
        if let Ok(val) = js_to_serde::<serde_json::Value>(&payload) {
            handler(val);
        }
    }) as Box<dyn FnMut(JsValue)>);

    let _ = listen_fn.call2(
        &event_mod,
        &JsValue::from_str(event_name),
        cb.as_ref().unchecked_ref(),
    );
    // 根监听与进程同寿命
    std::mem::forget(cb);
}

/// 写入系统剪贴板文本。
pub async fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let win = web_sys::window().ok_or_else(|| "缺少 window".to_string())?;
    let clipboard = win.navigator().clipboard();
    let promise = clipboard.write_text(text);
    JsFuture::from(promise).await.map_err(js_err_to_string)?;
    Ok(())
}

/// 读取/写入 localStorage 辅助。
pub fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

pub fn ls_get(key: &str) -> Option<String> {
    storage()?.get_item(key).ok().flatten()
}

pub fn ls_set(key: &str, value: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(key, value);
    }
}

pub fn reload_page() {
    if let Some(win) = web_sys::window() {
        let _ = win.location().reload();
    }
}

pub fn request_animation_frame(f: impl FnOnce() + 'static) {
    let Some(win) = web_sys::window() else {
        return;
    };
    let cb = Closure::once_into_js(Box::new(f) as Box<dyn FnOnce()>);
    let _ = win.request_animation_frame(cb.unchecked_ref());
}

/// 同步窗口明暗主题（让 Windows DWM 与 prefers-color-scheme 跟上切换）。
pub async fn sync_window_theme(mode: &str) {
    if !is_tauri() {
        return;
    }
    #[derive(Serialize)]
    struct Arg<'a> {
        theme: &'a str,
    }
    let _ = invoke_unit("window_set_theme", &Arg { theme: mode }).await;
}

pub async fn open_external_url(url: &str) {
    if is_tauri() {
        #[derive(Serialize)]
        struct Arg<'a> {
            url: &'a str,
        }
        let _ = invoke_unit("open_url", &Arg { url }).await;
    } else if let Some(win) = web_sys::window() {
        let _ = win.open_with_url_and_target(url, "_blank");
    }
}
