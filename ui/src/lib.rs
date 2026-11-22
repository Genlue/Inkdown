//! Inkdown UI 入口（Leptos CSR + GlassDesign）。

mod components;
mod markdown;
mod state;
mod tauri_bridge;
mod vault;

use glass_design::prelude::*;
use leptos::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use components::dialogs::AppDialogsAndMenus;
use components::editor_workspace::EditorWorkspace;
use components::file_pane::FilePane;
use components::file_views::NonMarkdownView;
use components::inspector_pane::InspectorPane;
use components::nav_sidebar::AppNavSidebar;
use components::quick_switcher::QuickSwitcherDialog;
use components::settings_page::SettingsPage;
use components::status_bar::AppStatusBar;
use components::tabs_bar::TabsBar;
use components::toolbar::AppToolbar;
use state::{
    AppState, ContextMenuKind, ContextMenuState, InputDialogKind, InputDialogState, NavPage,
};
use vault::fs::dirname;

#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    // wasm 里 panic 默认只会 trap，控制台一个字都看不到、界面停在原地。
    // 桌面端排查「打开就卡在加载」时全靠这条日志，别删。
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("[inkdown panic] {info}").into());
    }));
    // Inkdown 走的是手写挂载链（不是 bootstrap/GlassApp），库不会替我们恢复
    // 「玻璃效果」档位；漏了这一步，用户选的「性能优先」每次重启都会回到完整效果。
    // 顺序与库内一致：档位先落到 <html>，再注入样式，第一帧就是用户要的那档。
    glass_design::restore_effects_preference();
    glass_design::install();
    leptos::mount::mount_to_body(App);
}

fn install_window_listeners(state: AppState) {
    let Some(win) = web_sys::window() else {
        return;
    };
    let Some(doc) = win.document() else {
        return;
    };

    // 1. 窗口焦点状态（驱动红绿灯失焦退灰 + 切回窗口自动兜底刷新）
    {
        let on_focus = Closure::<dyn FnMut()>::new(move || {
            state.window_focused.set(true);
            if state.ready.get_untracked() {
                spawn_local(async move {
                    state.handle_external_fs_change(Vec::new()).await;
                });
            }
        });
        let _ = win.add_event_listener_with_callback("focus", on_focus.as_ref().unchecked_ref());
        std::mem::forget(on_focus);

        let on_blur = Closure::<dyn FnMut()>::new(move || {
            state.window_focused.set(false);
        });
        let _ = win.add_event_listener_with_callback("blur", on_blur.as_ref().unchecked_ref());
        std::mem::forget(on_blur);
    }

    // 2. 全局右键菜单接管：输入框放行系统菜单，其余走 GlassDesign Menu
    {
        let doc_for_blur = doc.clone();
        let on_ctx = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |ev: web_sys::MouseEvent| {
            if let Some(target) = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            {
                if target
                    .closest("input, textarea, [contenteditable='true']")
                    .ok()
                    .flatten()
                    .is_some()
                {
                    return;
                }
            }
            ev.prevent_default();
            if let Some(active) = doc_for_blur.active_element() {
                if let Some(html) = active.dyn_ref::<web_sys::HtmlElement>() {
                    let _ = html.blur();
                }
            }
            state.context_menu.set(Some(ContextMenuState {
                x: ev.client_x() as f64,
                y: ev.client_y() as f64,
                kind: ContextMenuKind::Global,
            }));
        });
        let _ = doc.add_event_listener_with_callback("contextmenu", on_ctx.as_ref().unchecked_ref());
        std::mem::forget(on_ctx);
    }

    // 3. 全局快捷键
    {
        let on_key = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
            move |ev: web_sys::KeyboardEvent| {
                if ev.is_composing() || ev.key_code() == 229 {
                    return;
                }
                let mod_key = ev.ctrl_key() || ev.meta_key();
                let key_lc = ev.key().to_ascii_lowercase();

                if ev.key() == "F1" {
                    ev.prevent_default();
                    state.shortcuts_open.set(true);
                    return;
                }

                if mod_key && ev.alt_key() && key_lc == "r" {
                    ev.prevent_default();
                    spawn_local(async move {
                        state.rescan_vault(true).await;
                    });
                    return;
                }

                if mod_key && (key_lc == "p" || key_lc == "o") {
                    ev.prevent_default();
                    state.quick_open.update(|v| *v = !*v);
                    return;
                }

                if mod_key && key_lc == "f" {
                    if state.editor_path.get_untracked().is_some() {
                        ev.prevent_default();
                        state.find_open.set(true);
                        state.find_replace_open.set(false);
                        state.find_focus_tick.update(|n| *n += 1);
                    }
                    return;
                }

                if mod_key && key_lc == "h" {
                    if state.editor_path.get_untracked().is_some() {
                        ev.prevent_default();
                        state.find_open.set(true);
                        state.find_replace_open.set(true);
                        state.find_focus_tick.update(|n| *n += 1);
                    }
                    return;
                }

                if mod_key && (ev.key() == "=" || ev.key() == "+") {
                    ev.prevent_default();
                    let next = state.editor_font_size.get_untracked() + 1.0;
                    state.set_editor_font_size(next);
                    toast(format!("字号：{}px", next.round() as i32));
                    return;
                }

                if mod_key && (ev.key() == "-" || ev.key() == "_") {
                    ev.prevent_default();
                    let next = state.editor_font_size.get_untracked() - 1.0;
                    state.set_editor_font_size(next);
                    toast(format!("字号：{}px", next.round() as i32));
                    return;
                }

                if mod_key && ev.key() == "0" {
                    ev.prevent_default();
                    state.set_editor_font_size(14.0);
                    toast("字号已复位 (14px)");
                    return;
                }

                if ev.alt_key() && ev.key() == "ArrowLeft" {
                    ev.prevent_default();
                    spawn_local(async move {
                        state.navigate_back().await;
                    });
                    return;
                }

                if ev.alt_key() && ev.key() == "ArrowRight" {
                    ev.prevent_default();
                    spawn_local(async move {
                        state.navigate_forward().await;
                    });
                    return;
                }

                if mod_key && key_lc == "n" {
                    ev.prevent_default();
                    let dir = state
                        .active_tab
                        .get_untracked()
                        .map(|p| dirname(&p))
                        .unwrap_or_default();
                    if ev.shift_key() {
                        state.input_dialog.set(Some(InputDialogState {
                            kind: InputDialogKind::NewFolder { dir },
                            title: "新建文件夹".to_string(),
                            description: "输入文件夹名称".to_string(),
                            placeholder: "新建文件夹".to_string(),
                            initial: String::new(),
                            confirm_label: "创建".to_string(),
                        }));
                    } else {
                        state.input_dialog.set(Some(InputDialogState {
                            kind: InputDialogKind::NewNote { dir },
                            title: "新建笔记".to_string(),
                            description: "输入笔记名称".to_string(),
                            placeholder: "未命名笔记".to_string(),
                            initial: String::new(),
                            confirm_label: "创建".to_string(),
                        }));
                    }
                    return;
                }

                if mod_key && key_lc == "s" {
                    ev.prevent_default();
                    spawn_local(async move {
                        state.commit_doc().await;
                        toast("已保存");
                    });
                    return;
                }

                if mod_key && key_lc == "w" {
                    if let Some(active) = state.active_tab.get_untracked() {
                        ev.prevent_default();
                        spawn_local(async move {
                            state.close_tab(&active).await;
                        });
                    }
                    return;
                }

                if mod_key && key_lc == "k" {
                    ev.prevent_default();
                    if state.list_hidden.get_untracked() {
                        state.toggle_list();
                    }
                    state.search_focus_tick.update(|n| *n += 1);
                    return;
                }

                if mod_key && ev.key() == "\\" {
                    ev.prevent_default();
                    state.toggle_nav();
                    return;
                }

                if mod_key && ev.shift_key() && key_lc == "b" {
                    ev.prevent_default();
                    state.toggle_list();
                    return;
                }

                if mod_key && ev.shift_key() && key_lc == "e" {
                    ev.prevent_default();
                    state.toggle_inspector();
                }
            },
        );
        let _ = doc.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
        std::mem::forget(on_key);
    }
}

#[component]
fn App() -> impl IntoView {
    let theme = Theme::provide();
    let state = AppState::provide(theme);
    install_window_listeners(state);

    let document_area = move || {
        view! {
            <div class="ink-doc-area">
                <TabsBar />
                <div class="ink-doc-stage">
                    {move || {
                        let Some(path) = state.active_tab.get() else {
                            return view! {
                                <Center>
                                    <Empty
                                        icon=IconName::FILE_TEXT
                                        title="没有打开的笔记"
                                        desc="从左侧文件树选择笔记，或按下 Ctrl+P 快速打开"
                                    >
                                        <Cluster>
                                            <Button
                                                label="新建笔记"
                                                icon=IconName::PLUS
                                                variant=ButtonVariant::Primary
                                                on_click=Callback::new(move |_| {
                                                    state.input_dialog.set(Some(InputDialogState {
                                                        kind: InputDialogKind::NewNote { dir: String::new() },
                                                        title: "新建笔记".to_string(),
                                                        description: "输入笔记名称".to_string(),
                                                        placeholder: "未命名笔记".to_string(),
                                                        initial: String::new(),
                                                        confirm_label: "创建".to_string(),
                                                    }));
                                                })
                                            />
                                            <Button
                                                label="快速打开 (Ctrl+P)"
                                                icon=IconName::SEARCH
                                                on_click=Callback::new(move |_| state.quick_open.set(true))
                                            />
                                        </Cluster>
                                    </Empty>
                                </Center>
                            }.into_any();
                        };

                        if state.editor_path.get().as_deref() == Some(path.as_str()) {
                            return view! { <EditorWorkspace /> }.into_any();
                        }

                        let ft = state
                            .note_at(&path)
                            .map(|n| n.file_type)
                            .unwrap_or(vault::fs::get_file_type(&path));
                        view! { <NonMarkdownView path=path file_type=ft /> }.into_any()
                    }}
                </div>
            </div>
        }
    };

    let doc_plus_inspector = move || {
        view! {
            <div class="ink-doc-shell">
                <div class="ink-doc-shell__main">
                    {document_area()}
                </div>
                <Show when=move || !state.inspector_hidden.get()>
                    <InspectorPane />
                </Show>
            </div>
        }
    };

    view! {
        <AppShell nav_hidden=Signal::derive(move || state.nav_hidden.get())>
            <AppToolbar />
            <AppNavSidebar />

            <Main>
                {move || {
                    if !state.ready.get() {
                        return view! {
                            <Center>
                                <Stack>
                                    <Spinner />
                                    <span class="c-subtitle">"正在打开知识库…"</span>
                                </Stack>
                            </Center>
                        }.into_any();
                    }

                    if let Some(err) = state.load_error.get() {
                        return view! {
                            <Center>
                                <Empty
                                    icon=IconName::TRIANGLE_ALERT
                                    title="仓库初始化失败"
                                    desc=err
                                />
                            </Center>
                        }.into_any();
                    }

                    if matches!(state.nav_page.get(), NavPage::Settings) {
                        return view! {
                            <SettingsPage />
                            <div class="c-topveil"></div>
                        }.into_any();
                    }

                    view! {
                        <div class="ink-workspace">
                            <div class="ink-workspace__body">
                                {move || {
                                    if state.list_hidden.get() {
                                        doc_plus_inspector().into_any()
                                    } else {
                                        view! {
                                            <SplitView
                                                orientation=SplitOrientation::Horizontal
                                                initial=0.20
                                                min=0.15
                                                max=0.38
                                                first=ToChildren::to_children(move || view! { <FilePane /> })
                                                second=ToChildren::to_children(doc_plus_inspector)
                                            />
                                        }.into_any()
                                    }
                                }}
                            </div>
                            <AppStatusBar />
                        </div>
                    }.into_any()
                }}
            </Main>

            <QuickSwitcherDialog />
            <AppDialogsAndMenus />
        </AppShell>
    }
}
