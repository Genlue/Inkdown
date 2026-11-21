//! 快速打开与命令面板（Ctrl+P / Ctrl+O）：基于 GlassDesign Dialog + SearchField。

use glass_design::prelude::*;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::state::{AppState, InputDialogKind, InputDialogState, NavPage};
use crate::vault::fs::dirname;

#[derive(Clone)]
enum QuickItem {
    Note {
        path: String,
        title: String,
        subtitle: String,
        favorite: bool,
    },
    Command {
        id: &'static str,
        title: &'static str,
        shortcut: &'static str,
        icon: IconName,
    },
}

#[component]
pub fn QuickSwitcherDialog() -> impl IntoView {
    let state = AppState::get();
    let theme = use_context::<Theme>().expect("Theme must be provided");
    let query = RwSignal::new(String::new());
    let selected = RwSignal::new(0usize);

    Effect::new(move |_| {
        if state.quick_open.get() {
            query.set(String::new());
            selected.set(0);
        }
    });

    let results = move || {
        let q_raw = query.get();
        let q = q_raw.trim().to_lowercase();
        let cmd_only = q.starts_with('>');
        let q_clean = q.trim_start_matches('>').trim();

        let mut list: Vec<QuickItem> = Vec::new();

        if !cmd_only {
            let mut notes = state.flat_notes();
            notes.sort_by(|a, b| b.mtime.cmp(&a.mtime));
            for n in notes {
                if q_clean.is_empty()
                    || n.title.to_lowercase().contains(q_clean)
                    || n.path.to_lowercase().contains(q_clean)
                {
                    list.push(QuickItem::Note {
                        subtitle: dirname(&n.path),
                        path: n.path,
                        title: n.title,
                        favorite: n.favorite,
                    });
                    if list.len() >= 18 {
                        break;
                    }
                }
            }
        }

        let commands = [
            ("new-note", "新建笔记", "Ctrl+N", IconName::PLUS),
            ("new-folder", "新建文件夹", "Ctrl+Shift+N", IconName::FOLDER_PLUS),
            ("toggle-mode", "切换编辑 / 分栏 / 预览模式", "", IconName::COLUMNS_2),
            ("toggle-nav", "收起 / 展开导航栏", "Ctrl+\\", IconName::PANEL_LEFT),
            ("toggle-list", "收起 / 展开文件列表", "Ctrl+Shift+B", IconName::LIST),
            ("toggle-insp", "收起 / 展开右侧检查器", "Ctrl+Shift+E", IconName::PANEL_RIGHT),
            ("theme-light", "切换到浅色主题", "", IconName::SUN),
            ("theme-dark", "切换到深色主题", "", IconName::MOON),
            ("theme-system", "主题跟随系统", "", IconName::MONITOR),
            ("settings", "打开偏好设置", "", IconName::SLIDERS),
            ("shortcuts", "查看快捷键速查", "F1", IconName::KEYBOARD),
            ("rescan", "重新扫描仓库", "Ctrl+Alt+R", IconName::REFRESH_CW),
        ];

        for (id, title, shortcut, icon) in commands {
            if cmd_only || !q_clean.is_empty() {
                if q_clean.is_empty() || title.to_lowercase().contains(q_clean) {
                    list.push(QuickItem::Command {
                        id,
                        title,
                        shortcut,
                        icon,
                    });
                }
            }
        }

        list
    };

    let execute = move |item: QuickItem| {
        state.quick_open.set(false);
        match item {
            QuickItem::Note { path, .. } => {
                spawn_local(async move {
                    state.open_file(&path, true).await;
                });
            }
            QuickItem::Command { id, .. } => match id {
                "new-note" => {
                    let dir = state
                        .active_tab
                        .get_untracked()
                        .map(|p| dirname(&p))
                        .unwrap_or_default();
                    state.input_dialog.set(Some(InputDialogState {
                        kind: InputDialogKind::NewNote { dir },
                        title: "新建笔记".to_string(),
                        description: "输入笔记名称".to_string(),
                        placeholder: "未命名笔记".to_string(),
                        initial: String::new(),
                        confirm_label: "创建".to_string(),
                    }));
                }
                "new-folder" => {
                    state.input_dialog.set(Some(InputDialogState {
                        kind: InputDialogKind::NewFolder { dir: String::new() },
                        title: "新建文件夹".to_string(),
                        description: "输入文件夹名称".to_string(),
                        placeholder: "新建文件夹".to_string(),
                        initial: String::new(),
                        confirm_label: "创建".to_string(),
                    }));
                }
                "toggle-mode" => {
                    let next = match state.view_mode.get_untracked().as_str() {
                        "edit" => "split",
                        "split" => "preview",
                        _ => "edit",
                    };
                    state.set_view_mode(next.to_string());
                }
                "toggle-nav" => state.toggle_nav(),
                "toggle-list" => state.toggle_list(),
                "toggle-insp" => state.toggle_inspector(),
                "theme-light" => state.set_theme_mode(theme, "light"),
                "theme-dark" => state.set_theme_mode(theme, "dark"),
                "theme-system" => state.set_theme_mode(theme, "system"),
                "settings" => state.nav_page.set(NavPage::Settings),
                "shortcuts" => state.shortcuts_open.set(true),
                "rescan" => {
                    spawn_local(async move {
                        state.rescan_vault(true).await;
                    });
                }
                _ => {}
            },
        }
    };

    view! {
        <Dialog
            open=Signal::derive(move || state.quick_open.get())
            title="快速打开"
            description="输入笔记名搜索，输入 > 搜索命令 · ↑↓ 选择 · Enter 打开 · Esc 退出"
            width=540
            on_close=Callback::new(move |_| state.quick_open.set(false))
        >
            <div
                class="ink-qs"
                on:keydown=move |ev: web_sys::KeyboardEvent| {
                    let list = results();
                    if list.is_empty() {
                        return;
                    }
                    match ev.key().as_str() {
                        "ArrowDown" => {
                            ev.prevent_default();
                            selected.update(|i| *i = (*i + 1) % list.len());
                        }
                        "ArrowUp" => {
                            ev.prevent_default();
                            selected.update(|i| *i = if *i == 0 { list.len() - 1 } else { *i - 1 });
                        }
                        "Enter" => {
                            ev.prevent_default();
                            let idx = selected.get_untracked().min(list.len() - 1);
                            if let Some(item) = list.get(idx).cloned() {
                                execute(item);
                            }
                        }
                        _ => {}
                    }
                }
            >
                <div class="ink-qs__search">
                    <SearchField
                        value=Signal::derive(move || query.get())
                        placeholder="搜索笔记，或输入 > 执行命令…"
                        width=496
                        on_input=Callback::new(move |v: String| {
                            query.set(v);
                            selected.set(0);
                        })
                    />
                </div>

                <div class="ink-qs__list" role="listbox">
                    {move || {
                        let list = results();
                        let sel = selected.get().min(list.len().saturating_sub(1));
                        if list.is_empty() {
                            return view! {
                                <Empty
                                    icon=IconName::SEARCH
                                    title="没有匹配的笔记或命令"
                                    desc="试试换个关键词"
                                />
                            }.into_any();
                        }
                        list.into_iter().enumerate().map(|(i, item)| {
                            let item_exec = item.clone();
                            let active = i == sel;
                            match item {
                                QuickItem::Note { title, subtitle, favorite, .. } => view! {
                                    <button
                                        type="button"
                                        class=if active { "ink-qs__item is-selected" } else { "ink-qs__item" }
                                        role="option"
                                        aria-selected=active.to_string()
                                        on:mouseenter=move |_| selected.set(i)
                                        on:click=move |_| execute(item_exec.clone())
                                    >
                                        <Icon name=IconName::FILE_TEXT size=14 />
                                        <span class="ink-qs__title">{title}</span>
                                        {favorite.then(|| view! {
                                            <span class="ink-tree__star"><Icon name=IconName::STAR size=11 /></span>
                                        })}
                                        {(!subtitle.is_empty()).then(|| view! {
                                            <span class="ink-qs__sub">{subtitle}</span>
                                        })}
                                    </button>
                                }.into_any(),
                                QuickItem::Command { title, shortcut, icon, .. } => view! {
                                    <button
                                        type="button"
                                        class=if active { "ink-qs__item is-selected" } else { "ink-qs__item" }
                                        role="option"
                                        aria-selected=active.to_string()
                                        on:mouseenter=move |_| selected.set(i)
                                        on:click=move |_| execute(item_exec.clone())
                                    >
                                        <Icon name=icon size=14 />
                                        <span class="ink-qs__title">{title}</span>
                                        <Badge label="命令" variant=BadgeVariant::Accent />
                                        {(!shortcut.is_empty()).then(|| view! {
                                            <span class="ink-qs__key">{shortcut}</span>
                                        })}
                                    </button>
                                }.into_any(),
                            }
                        }).collect_view().into_any()
                    }}
                </div>
            </div>
        </Dialog>
    }
}
