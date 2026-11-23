//! 窗口顶栏：红绿灯、导航开关、前进/后退、面包屑、视图模式分段与面板开关。

use glass_design::prelude::*;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::state::{AppState, NavPage, SaveStatus};

#[component]
pub fn AppToolbar() -> impl IntoView {
    let state = AppState::get();
    let theme = use_context::<Theme>().expect("Theme must be provided");

    let view_options = StoredValue::new_local(vec![
        SegmentedOption::new("edit", "")
            .icon(IconName::SQUARE_PEN)
            .title("仅编辑"),
        SegmentedOption::new("split", "")
            .icon(IconName::COLUMNS_2)
            .title("双栏分屏"),
        SegmentedOption::new("preview", "")
            .icon(IconName::EYE)
            .title("仅预览"),
    ]);

    let view_value = Signal::derive(move || state.view_mode.get());
    let set_view_mode = Callback::new(move |m: String| state.set_view_mode(m));

    // 主题与设置页共用同一套三档值。顶栏原来只有一颗「深浅互换」的按钮，
    // 一旦切到 System 就再也没有入口能切回去，界面上等于缺了「跟随系统」这一档。
    let theme_options = StoredValue::new_local(vec![
        SegmentedOption::new("light", "")
            .icon(IconName::SUN)
            .title("浅色"),
        SegmentedOption::new("dark", "")
            .icon(IconName::MOON)
            .title("深色"),
        SegmentedOption::new("system", "")
            .icon(IconName::MONITOR)
            .title("跟随系统"),
    ]);
    let theme_value = Signal::derive(move || theme.mode.get().as_str().to_string());
    let set_theme_mode = Callback::new(move |m: String| state.set_theme_mode(theme, &m));

    let active_is_markdown = move || {
        let on_settings = matches!(state.nav_page.get(), NavPage::Settings);
        if on_settings {
            return false;
        }
        state.editor_path.get().is_some()
    };

    let crumbs = move || {
        let page = state.nav_page.get();
        if matches!(page, NavPage::Settings) {
            return vec![
                Crumb::new(state.vault_name()).icon(IconName::HARD_DRIVE),
                Crumb::new("偏好设置").icon(IconName::SLIDERS),
            ];
        }
        if let Some(path) = state.active_tab.get() {
            let mut list = vec![Crumb::new(state.vault_name())];
            let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            for (idx, seg) in segs.iter().enumerate() {
                if idx + 1 == segs.len() {
                    let display = state
                        .note_at(&path)
                        .map(|n| n.title)
                        .unwrap_or_else(|| (*seg).to_string());
                    list.push(Crumb::new(display));
                } else {
                    list.push(Crumb::new(*seg));
                }
            }
            return list;
        }
        match page {
            NavPage::Favorites => vec![Crumb::new(state.vault_name()), Crumb::new("收藏")],
            NavPage::Trash => vec![Crumb::new(state.vault_name()), Crumb::new("回收站")],
            _ => vec![Crumb::new(state.vault_name()), Crumb::new("文件")],
        }
    };

    view! {
        <Toolbar>
            // 必须是 Toolbar 的第一个子项，标题组位移按它算宽度
            <ToolbarChrome>
                <TrafficLights
                    focused=Signal::derive(move || state.window_focused.get())
                    dirty=Signal::derive(move || state.save_status.get() == SaveStatus::Dirty)
                    maximized=Signal::derive(move || state.window_maximized.get())
                    on_close=Callback::new(move |_| state.window_close())
                    on_minimize=Callback::new(move |_| state.window_minimize())
                    on_zoom=Callback::new(move |_| state.window_toggle_maximize())
                />
                {move || {
                    let hidden = state.nav_hidden.get();
                    let (icon, label) = if hidden {
                        (IconName::PANEL_RIGHT, "展开导航栏 (Ctrl+\\)")
                    } else {
                        (IconName::PANEL_LEFT, "收起导航栏 (Ctrl+\\)")
                    };
                    view! {
                        <Capsule class="l-chrome__toggle c-capsule--circle">
                            <IconButton
                                icon=icon
                                label=label
                                on_click=Callback::new(move |_| state.toggle_nav())
                            />
                        </Capsule>
                    }
                }}
            </ToolbarChrome>

            <ToolbarLead>
                <NavPair
                    can_back=Signal::derive(move || state.can_go_back())
                    can_forward=Signal::derive(move || state.can_go_forward())
                    back_label="后退 (Alt+←)"
                    forward_label="前进 (Alt+→)"
                    on_back=Callback::new(move |_| {
                        spawn_local(async move {
                            state.navigate_back().await;
                        });
                    })
                    on_forward=Callback::new(move |_| {
                        spawn_local(async move {
                            state.navigate_forward().await;
                        });
                    })
                />
                <Capsule class="c-capsule--circle">
                    {move || {
                        let list_open = !state.list_hidden.get();
                        view! {
                            <IconButton
                                icon=IconName::LIST
                                label=if list_open { "收起文件列表 (Ctrl+Shift+B)" } else { "展开文件列表 (Ctrl+Shift+B)" }
                                toggle=true
                                active=list_open
                                on_click=Callback::new(move |_| state.toggle_list())
                            />
                        }
                    }}
                </Capsule>
                <Capsule class="ink-crumb-capsule">
                    // 胶囊外壳自带 data-tauri-drag-region="false"，会把整条面包屑
                    // 变成拖动死区；在内容层重新声明 deep，让空白与标题仍能拖窗，
                    // 面包屑按钮仍按「可点击元素」优先拦下点击。
                    <div class="ink-crumbs-wrap" data-tauri-drag-region="deep">
                        {move || view! {
                            <Breadcrumb
                                items=crumbs()
                                on_pick=Callback::new(move |idx: usize| {
                                    if idx == 0 {
                                        state.nav_page.set(NavPage::Files);
                                        if state.list_hidden.get_untracked() {
                                            state.toggle_list();
                                        }
                                    }
                                })
                            />
                        }}
                    </div>
                </Capsule>
            </ToolbarLead>

            <ToolbarPills>
                <Show when=active_is_markdown>
                    <Capsule>
                        <Segmented
                            options=view_options.get_value()
                            value=view_value
                            label="视图模式"
                            on_change=set_view_mode
                        />
                    </Capsule>
                </Show>

                <Capsule class="c-capsule--circle">
                    <IconButton
                        icon=IconName::SEARCH
                        label="快速打开笔记或命令 (Ctrl+P)"
                        on_click=Callback::new(move |_| state.quick_open.set(true))
                    />
                </Capsule>

                <Capsule class="c-capsule--circle">
                    {move || {
                        let insp_open = !state.inspector_hidden.get();
                        view! {
                            <IconButton
                                icon=IconName::PANEL_RIGHT
                                label=if insp_open { "收起右侧检查器 (Ctrl+Shift+E)" } else { "展开右侧检查器 (Ctrl+Shift+E)" }
                                toggle=true
                                active=insp_open
                                on_click=Callback::new(move |_| state.toggle_inspector())
                            />
                        }
                    }}
                </Capsule>

                <Capsule>
                    <Segmented
                        options=theme_options.get_value()
                        value=theme_value
                        label="主题模式"
                        on_change=set_theme_mode
                    />
                </Capsule>

                <Capsule class="c-capsule--circle">
                    {move || {
                        let is_settings = matches!(state.nav_page.get(), NavPage::Settings);
                        view! {
                            <IconButton
                                icon=IconName::SLIDERS
                                label="偏好设置"
                                toggle=true
                                active=is_settings
                                on_click=Callback::new(move |_| {
                                    if is_settings {
                                        state.nav_page.set(NavPage::Files);
                                    } else {
                                        state.nav_page.set(NavPage::Settings);
                                    }
                                })
                            />
                        }
                    }}
                </Capsule>
            </ToolbarPills>
        </Toolbar>
    }
}
