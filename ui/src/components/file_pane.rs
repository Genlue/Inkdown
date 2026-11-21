//! 中间文件列表栏：搜索、最近打开、文件树（拖拽移动 + 右键菜单 + 引导线）、收藏、标签聚合、回收站。

use glass_design::prelude::*;
use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::components::icons::{ICON_CHEVRONS_UP_DOWN, ICON_CODE, ICON_HASH, ICON_TABLE};
use crate::markdown::{plain_excerpt, rel_time};
use crate::state::{
    AppState, ConfirmDialogKind, ConfirmDialogState, ContextMenuKind, ContextMenuState,
    InputDialogKind, InputDialogState, NavPage,
};
use crate::vault::fs::{dirname, FileType};
use crate::vault::VaultTreeNode;

fn icon_for_file(ft: FileType) -> IconName {
    match ft {
        FileType::Markdown => IconName::FILE_TEXT,
        FileType::Image => IconName::IMAGE,
        FileType::Video => IconName::PLAY,
        FileType::Audio => IconName::VOLUME_2,
        FileType::Code => ICON_CODE,
        FileType::Excel => ICON_TABLE,
        _ => IconName::FILE_TEXT,
    }
}

#[component]
pub fn FilePane() -> impl IntoView {
    let state = AppState::get();
    let search_input_ref = NodeRef::<html::Input>::new();
    let drag_src = RwSignal::new(None::<String>);
    let drop_target = RwSignal::new(None::<String>);
    let refreshing = RwSignal::new(false);

    // Ctrl+K 聚焦搜索框
    Effect::new(move |_| {
        let tick = state.search_focus_tick.get();
        if tick > 0 {
            if let Some(el) = search_input_ref.get_untracked() {
                let _ = el.focus();
                let _ = el.select();
            }
        }
    });

    let open_new_note_dialog = move |dir: String| {
        state.input_dialog.set(Some(InputDialogState {
            kind: InputDialogKind::NewNote { dir },
            title: "新建笔记".to_string(),
            description: "输入笔记名称，将自动创建 .md 文件".to_string(),
            placeholder: "未命名笔记".to_string(),
            initial: String::new(),
            confirm_label: "创建".to_string(),
        }));
    };

    let open_new_folder_dialog = move |dir: String| {
        state.input_dialog.set(Some(InputDialogState {
            kind: InputDialogKind::NewFolder { dir },
            title: "新建文件夹".to_string(),
            description: "输入文件夹名称".to_string(),
            placeholder: "新建文件夹".to_string(),
            initial: String::new(),
            confirm_label: "创建".to_string(),
        }));
    };

    let sort_items = move || {
        let cur = state.sort_mode.get();
        vec![
            MenuItem::new("updated", "最近编辑")
                .icon(IconName::CLOCK)
                .checked(cur == "updated"),
            MenuItem::new("created", "最近创建")
                .icon(IconName::CALENDAR_RANGE)
                .checked(cur == "created"),
            MenuItem::new("title", "按名称")
                .icon(IconName::LIST)
                .checked(cur == "title"),
        ]
    };

    let header_title = move || match state.nav_page.get() {
        NavPage::Favorites => ("收藏".to_string(), state.counts().favorites),
        NavPage::Trash => ("回收站".to_string(), state.counts().trash),
        NavPage::Tag(t) => {
            let idx = state.link_index.get();
            let cnt = idx.tags.get(&t).map(|s| s.len()).unwrap_or(0);
            (format!("#{t}"), cnt)
        }
        _ => ("文件".to_string(), state.counts().all),
    };

    view! {
        <aside class="ink-list" aria-label="文件列表">
            <div class="ink-list__head">
                <div class="ink-list__search">
                    <div class="l-fill">
                        <div class="c-field">
                            <span class="c-field__icon">
                                <Icon name=IconName::SEARCH size=14 />
                            </span>
                            <input
                                class="c-field__input selectable"
                                type="text"
                                placeholder="搜索笔记 (Ctrl+K)"
                                aria-label="搜索笔记"
                                node_ref=search_input_ref
                                prop:value=move || state.search_query.get()
                                on:input=move |ev| state.run_search(event_target_value(&ev))
                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                    if ev.key() == "Escape" {
                                        ev.prevent_default();
                                        state.run_search(String::new());
                                        if let Some(el) = search_input_ref.get_untracked() {
                                            let _ = el.blur();
                                        }
                                    }
                                }
                            />
                            <Show when=move || !state.search_query.get().is_empty()>
                                <button
                                    class="c-iconbtn c-iconbtn--sm"
                                    type="button"
                                    aria-label="清除搜索"
                                    on:click=move |_| {
                                        state.run_search(String::new());
                                        if let Some(el) = search_input_ref.get_untracked() {
                                            let _ = el.focus();
                                        }
                                    }
                                >
                                    <Icon name=IconName::X_CIRCLE size=14 />
                                </button>
                            </Show>
                        </div>
                    </div>
                    <IconButton
                        icon=IconName::PLUS
                        label="新建笔记 (Ctrl+N)"
                        variant=IconButtonVariant::Tonal
                        on_click=Callback::new(move |_| {
                            let dir = state
                                .active_tab
                                .get_untracked()
                                .map(|p| dirname(&p))
                                .unwrap_or_default();
                            open_new_note_dialog(dir);
                        })
                    />
                    <IconButton
                        icon=IconName::FOLDER_PLUS
                        label="新建文件夹 (Ctrl+Shift+N)"
                        on_click=Callback::new(move |_| open_new_folder_dialog(String::new()))
                    />
                </div>

                <div class="ink-list__bar">
                    {move || {
                        let (title, count) = header_title();
                        view! {
                            <span class="ink-list__label">{title}</span>
                            <span class="ink-list__count">{count}</span>
                        }
                    }}
                    <span class="l-push"></span>
                    <Show when=move || matches!(state.nav_page.get(), NavPage::Files) && state.search_query.get().is_empty()>
                        <IconButton
                            icon=ICON_CHEVRONS_UP_DOWN
                            label="全部折叠 / 全部展开"
                            size=Size::Sm
                            on_click=Callback::new(move |_| state.fold_or_expand_all())
                        />
                        {move || view! {
                            <MenuButton
                                icon=IconName::SORT
                                label="排序"
                                class="c-btn--sm"
                                items=sort_items()
                                on_select=Callback::new(move |id: String| state.set_sort(id))
                            />
                        }}
                    </Show>
                    <button
                        class=move || if refreshing.get() { "c-iconbtn c-iconbtn--sm is-spinning" } else { "c-iconbtn c-iconbtn--sm" }
                        type="button"
                        title="重新扫描仓库 (Ctrl+Alt+R)"
                        aria-label="重新扫描仓库"
                        on:click=move |_| {
                            refreshing.set(true);
                            spawn_local(async move {
                                state.rescan_vault(true).await;
                                refreshing.set(false);
                            });
                        }
                    >
                        <Icon name=IconName::REFRESH_CW size=13 />
                    </button>
                    <Show when=move || state.is_desktop>
                        <IconButton
                            icon=IconName::FOLDER_OPEN
                            label="在文件资源管理器中打开仓库"
                            size=Size::Sm
                            on_click=Callback::new(move |_| state.open_in_explorer(String::new()))
                        />
                    </Show>
                </div>
            </div>

            <div
                class="ink-list__scroll"
                on:contextmenu=move |ev: web_sys::MouseEvent| {
                    // 空白处右键：新建笔记 / 文件夹
                    if let Some(target) = ev.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                        if target.closest(".ink-tree__row, .ink-note-card").ok().flatten().is_some() {
                            return;
                        }
                    }
                    ev.prevent_default();
                    ev.stop_propagation();
                    state.context_menu.set(Some(ContextMenuState {
                        x: ev.client_x() as f64,
                        y: ev.client_y() as f64,
                        kind: ContextMenuKind::TreeRoot,
                    }));
                }
            >
                {move || {
                    let query = state.search_query.get();
                    if !query.trim().is_empty() {
                        let hits = state.search_hits.get();
                        if hits.is_empty() {
                            return view! {
                                <div class="ink-list__empty">
                                    <Empty
                                        icon=IconName::SEARCH
                                        title="没有匹配的笔记"
                                        desc="试试换个关键词，或清除搜索条件"
                                    />
                                </div>
                            }.into_any();
                        }
                        return view! {
                            <div class="ink-note-cards">
                                {hits.into_iter().map(|hit| {
                                    let path = hit.note.path.clone();
                                    let path_sel = path.clone();
                                    let dir_label = dirname(&path);
                                    view! {
                                        <button
                                            type="button"
                                            class="ink-note-card"
                                            data-selected=move || (state.active_tab.get().as_deref() == Some(path_sel.as_str())).then_some("1")
                                            on:click=move |_| {
                                                let p = path.clone();
                                                spawn_local(async move {
                                                    state.open_file(&p, true).await;
                                                });
                                            }
                                        >
                                            <div class="ink-note-card__top">
                                                <span class="ink-note-card__title">{hit.note.title}</span>
                                                <span class="ink-note-card__time">{rel_time(hit.note.mtime)}</span>
                                            </div>
                                            {(!hit.snippet.is_empty()).then(|| view! {
                                                <div class="ink-note-card__snippet">{hit.snippet}</div>
                                            })}
                                            {(!dir_label.is_empty()).then(|| view! {
                                                <div class="ink-note-card__path">{dir_label}</div>
                                            })}
                                        </button>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any();
                    }

                    match state.nav_page.get() {
                        NavPage::Favorites => render_favorites_list(state),
                        NavPage::Trash => render_trash_list(state),
                        NavPage::Tag(tag) => render_tag_list(state, tag),
                        _ => render_files_tree(state, drag_src, drop_target),
                    }
                }}
            </div>
        </aside>
    }
}

fn render_files_tree(
    state: AppState,
    drag_src: RwSignal<Option<String>>,
    drop_target: RwSignal<Option<String>>,
) -> AnyView {
    let tree = state.tree();
    let recents = state.recents();

    view! {
        <div class="ink-files-body">
            {(!recents.is_empty()).then(|| {
                let is_open = state.recents_open.get();
                view! {
                    <div class="ink-recents">
                        <button
                            type="button"
                            class="ink-recents__head"
                            on:click=move |_| {
                                let next = !state.recents_open.get_untracked();
                                state.recents_open.set(next);
                                state.patch_workspace(|w| w.recents_open = next);
                            }
                        >
                            <span class=if is_open { "ink-tree__chevron is-open" } else { "ink-tree__chevron" }>
                                <Icon name=IconName::CHEVRON_RIGHT size=11 />
                            </span>
                            <span class="ink-recents__title">"最近打开"</span>
                            <span class="ink-list__count">{recents.len()}</span>
                        </button>
                        <Show when=move || state.recents_open.get()>
                            <div class="ink-recents__list">
                                {recents.iter().map(|n| {
                                    let p = n.path.clone();
                                    let p_sel = n.path.clone();
                                    let title = n.title.clone();
                                    let time = rel_time(n.mtime);
                                    view! {
                                        <button
                                            type="button"
                                            class="ink-recents__item"
                                            data-selected=move || (state.active_tab.get().as_deref() == Some(p_sel.as_str())).then_some("1")
                                            on:click=move |_| {
                                                let path = p.clone();
                                                spawn_local(async move {
                                                    state.open_file(&path, true).await;
                                                });
                                            }
                                        >
                                            <Icon name=icon_for_file(n.file_type) size=13 />
                                            <span class="ink-recents__name">{title}</span>
                                            <span class="ink-recents__time">{time}</span>
                                        </button>
                                    }
                                }).collect_view()}
                            </div>
                        </Show>
                    </div>
                }
            })}

            {if tree.is_empty() {
                view! {
                    <div class="ink-list__empty">
                        <Empty
                            icon=IconName::FOLDER
                            title="仓库还是空的"
                            desc="点击上方 + 按钮创建第一篇笔记"
                        />
                    </div>
                }.into_any()
            } else {
                view! {
                    <div
                        class="ink-tree"
                        role="tree"
                        aria-label="仓库文件树"
                        data-indent=move || state.tree_indent.get()
                        data-root-drop=move || (drop_target.get().as_deref() == Some("")).then_some("1")
                        on:dragover=move |ev: web_sys::DragEvent| {
                            if drag_src.get_untracked().is_some() {
                                ev.prevent_default();
                                drop_target.set(Some(String::new()));
                            }
                        }
                        on:dragleave=move |_| {
                            if drop_target.get_untracked().as_deref() == Some("") {
                                drop_target.set(None);
                            }
                        }
                        on:drop=move |ev: web_sys::DragEvent| {
                            ev.prevent_default();
                            drop_target.set(None);
                            if let Some(src) = drag_src.get_untracked() {
                                drag_src.set(None);
                                spawn_local(async move {
                                    state.move_item(src, String::new()).await;
                                });
                            }
                        }
                    >
                        {tree.into_iter().map(|node| render_tree_node(state, node, 0, drag_src, drop_target)).collect_view()}
                    </div>
                }.into_any()
            }}
        </div>
    }.into_any()
}

fn render_tree_node(
    state: AppState,
    node: VaultTreeNode,
    depth: usize,
    drag_src: RwSignal<Option<String>>,
    drop_target: RwSignal<Option<String>>,
) -> AnyView {
    if node.is_dir {
        let path = node.path.clone();
        let path_click = path.clone();
        let path_ctx = path.clone();
        let path_drag = path.clone();
        let path_over = path.clone();
        let path_leave = path.clone();
        let path_drop = path.clone();
        let path_drop_style = path.clone();
        let path_open = path.clone();
        let path_children = path.clone();
        let children = node.children;
        let name = node.name;

        return view! {
            <div class="ink-tree__branch" style=format!("--tree-depth:{depth}")>
                <div
                    class="ink-tree__row"
                    role="treeitem"
                    draggable="true"
                    data-type="dir"
                    data-path=path.clone()
                    data-drop=move || (drop_target.get().as_deref() == Some(path_drop_style.as_str())).then_some("1")
                    style=format!("--tree-depth:{depth}")
                    on:click=move |_| state.toggle_dir(&path_click)
                    on:contextmenu=move |ev: web_sys::MouseEvent| {
                        ev.prevent_default();
                        ev.stop_propagation();
                        state.context_menu.set(Some(ContextMenuState {
                            x: ev.client_x() as f64,
                            y: ev.client_y() as f64,
                            kind: ContextMenuKind::TreeDir { path: path_ctx.clone() },
                        }));
                    }
                    on:dragstart=move |ev: web_sys::DragEvent| {
                        ev.stop_propagation();
                        drag_src.set(Some(path_drag.clone()));
                    }
                    on:dragend=move |_| {
                        drag_src.set(None);
                        drop_target.set(None);
                    }
                    on:dragover=move |ev: web_sys::DragEvent| {
                        if let Some(src) = drag_src.get_untracked() {
                            if src != path_over && !path_over.starts_with(&format!("{src}/")) {
                                ev.prevent_default();
                                ev.stop_propagation();
                                drop_target.set(Some(path_over.clone()));
                            }
                        }
                    }
                    on:dragleave=move |_| {
                        if drop_target.get_untracked().as_deref() == Some(path_leave.as_str()) {
                            drop_target.set(None);
                        }
                    }
                    on:drop=move |ev: web_sys::DragEvent| {
                        ev.prevent_default();
                        ev.stop_propagation();
                        drop_target.set(None);
                        if let Some(src) = drag_src.get_untracked() {
                            drag_src.set(None);
                            let dst = path_drop.clone();
                            spawn_local(async move {
                                state.move_item(src, dst).await;
                            });
                        }
                    }
                >
                    <span class=move || {
                        if state.expanded_dirs.get().contains(&path_open) {
                            "ink-tree__chevron is-open"
                        } else {
                            "ink-tree__chevron"
                        }
                    }>
                        <Icon name=IconName::CHEVRON_RIGHT size=11 />
                    </span>
                    <span class="ink-tree__icon">
                        <Icon name=IconName::FOLDER size=14 />
                    </span>
                    <span class="ink-tree__name">{name}</span>
                </div>
                <Show when=move || state.expanded_dirs.get().contains(&path_children)>
                    <div class="ink-tree__children">
                        {children
                            .iter()
                            .cloned()
                            .map(|child| render_tree_node(state, child, depth + 1, drag_src, drop_target))
                            .collect_view()}
                    </div>
                </Show>
            </div>
        }
        .into_any();
    }

    let path = node.path.clone();
    let path_sel = path.clone();
    let path_click = path.clone();
    let path_dbl = path.clone();
    let path_ctx = path.clone();
    let path_drag = path.clone();
    let is_md = node.is_markdown;
    let favorite = node.favorite;
    let name = node.name.clone();
    let badge = (!is_md && !node.ext.is_empty()).then(|| node.ext.to_uppercase());

    view! {
        <div
            class="ink-tree__row"
            role="treeitem"
            draggable="true"
            data-type="file"
            data-path=path
            data-selected=move || (state.active_tab.get().as_deref() == Some(path_sel.as_str())).then_some("1")
            style=format!("--tree-depth:{depth}")
            on:click=move |_| {
                let p = path_click.clone();
                spawn_local(async move {
                    state.open_file(&p, true).await;
                });
            }
            on:dblclick=move |_| {
                if is_md {
                    let cur_title = state.note_at(&path_dbl).map(|n| n.title).unwrap_or_default();
                    state.input_dialog.set(Some(InputDialogState {
                        kind: InputDialogKind::RenameNote { path: path_dbl.clone() },
                        title: "重命名笔记".to_string(),
                        description: "输入新的笔记标题".to_string(),
                        placeholder: "笔记名称".to_string(),
                        initial: cur_title,
                        confirm_label: "重命名".to_string(),
                    }));
                }
            }
            on:contextmenu=move |ev: web_sys::MouseEvent| {
                ev.prevent_default();
                ev.stop_propagation();
                state.context_menu.set(Some(ContextMenuState {
                    x: ev.client_x() as f64,
                    y: ev.client_y() as f64,
                    kind: ContextMenuKind::TreeFile {
                        path: path_ctx.clone(),
                        is_md,
                        favorite,
                    },
                }));
            }
            on:dragstart=move |ev: web_sys::DragEvent| {
                ev.stop_propagation();
                drag_src.set(Some(path_drag.clone()));
            }
            on:dragend=move |_| {
                drag_src.set(None);
                drop_target.set(None);
            }
        >
            <span class="ink-tree__icon">
                <Icon name=icon_for_file(node.file_type) size=14 />
            </span>
            <span class="ink-tree__name">{name}</span>
            {favorite.then(|| view! {
                <span class="ink-tree__star" title="已收藏">
                    <Icon name=IconName::STAR size=11 />
                </span>
            })}
            {badge.map(|ext| view! { <span class="ink-tree__ext">{ext}</span> })}
        </div>
    }
    .into_any()
}

fn render_favorites_list(state: AppState) -> AnyView {
    let favs = state.favorites();
    if favs.is_empty() {
        return view! {
            <div class="ink-list__empty">
                <Empty
                    icon=IconName::STAR
                    title="还没有收藏笔记"
                    desc="在笔记右上角或右键菜单里点「收藏」，重要笔记就会固定在这里"
                />
            </div>
        }
        .into_any();
    }
    view! {
        <div class="ink-note-cards">
            {favs.into_iter().map(|note| {
                let p = note.path.clone();
                let p_sel = note.path.clone();
                let excerpt = state.vault.with_value(|cell| {
                    cell.borrow()
                        .as_ref()
                        .and_then(|v| v.cache.get(&p))
                        .map(|c| plain_excerpt(c, 58))
                        .unwrap_or_default()
                });
                view! {
                    <button
                        type="button"
                        class="ink-note-card"
                        data-selected=move || (state.active_tab.get().as_deref() == Some(p_sel.as_str())).then_some("1")
                        on:click=move |_| {
                            let path = p.clone();
                            spawn_local(async move {
                                state.open_file(&path, true).await;
                            });
                        }
                    >
                        <div class="ink-note-card__top">
                            <span class="ink-note-card__title">{note.title}</span>
                            <span class="ink-note-card__time">{rel_time(note.mtime)}</span>
                        </div>
                        {(!excerpt.is_empty()).then(|| view! {
                            <div class="ink-note-card__snippet">{excerpt}</div>
                        })}
                    </button>
                }
            }).collect_view()}
        </div>
    }
    .into_any()
}

fn render_tag_list(state: AppState, tag: String) -> AnyView {
    let idx = state.link_index.get();
    let paths: Vec<String> = idx
        .tags
        .get(&tag)
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default();

    view! {
        <div class="ink-files-body">
            <div class="ink-tag-banner">
                <Chip
                    label=format!("#{tag}")
                    icon=ICON_HASH
                    on_remove=Callback::new(move |_| state.nav_page.set(NavPage::Files))
                />
            </div>
            {if paths.is_empty() {
                view! {
                    <div class="ink-list__empty">
                        <Empty
                            icon=ICON_HASH
                            title="没有带此标签的笔记"
                            desc="标签已从笔记中移除"
                        />
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="ink-note-cards">
                        {paths.into_iter().filter_map(|p| state.note_at(&p)).map(|note| {
                            let p = note.path.clone();
                            let p_sel = note.path.clone();
                            let dir = dirname(&p);
                            view! {
                                <button
                                    type="button"
                                    class="ink-note-card"
                                    data-selected=move || (state.active_tab.get().as_deref() == Some(p_sel.as_str())).then_some("1")
                                    on:click=move |_| {
                                        let path = p.clone();
                                        spawn_local(async move {
                                            state.open_file(&path, true).await;
                                        });
                                    }
                                >
                                    <div class="ink-note-card__top">
                                        <span class="ink-note-card__title">{note.title}</span>
                                        <span class="ink-note-card__time">{rel_time(note.mtime)}</span>
                                    </div>
                                    {(!dir.is_empty()).then(|| view! {
                                        <div class="ink-note-card__path">{dir}</div>
                                    })}
                                </button>
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </div>
    }
    .into_any()
}

fn render_trash_list(state: AppState) -> AnyView {
    let items = state.trash();
    if items.is_empty() {
        return view! {
            <div class="ink-list__empty">
                <Empty
                    icon=IconName::TRASH
                    title="回收站是空的"
                    desc="移到回收站的笔记会暂存在 .trash/ 目录，可随时恢复"
                />
            </div>
        }
        .into_any();
    }

    view! {
        <div class="ink-files-body">
            <div class="ink-trash-bar">
                <span class="c-subtitle">"暂存在 .trash/ 中"</span>
                <Button
                    label="清空回收站"
                    variant=ButtonVariant::Danger
                    size=Size::Sm
                    on_click=Callback::new(move |_| {
                        state.confirm_dialog.set(Some(ConfirmDialogState {
                            kind: ConfirmDialogKind::EmptyTrash,
                            title: "清空回收站？".to_string(),
                            description: "回收站内的全部笔记将被永久删除，无法恢复。".to_string(),
                            confirm_label: "永久清空".to_string(),
                        }));
                    })
                />
            </div>
            <div class="ink-note-cards">
                {items.into_iter().map(|item| {
                    let tp_restore = item.path.clone();
                    let tp_del = item.path.clone();
                    let name_del = item.name.clone();
                    view! {
                        <div class="ink-trash-card">
                            <div class="ink-note-card__top">
                                <span class="ink-note-card__title">{item.name}</span>
                                <span class="ink-note-card__time">{rel_time(item.deleted_at)}</span>
                            </div>
                            <div class="ink-note-card__path">{item.original_path}</div>
                            <div class="ink-trash-card__actions">
                                <Button
                                    label="恢复"
                                    icon=IconName::UNDO
                                    size=Size::Sm
                                    variant=ButtonVariant::Tonal
                                    on_click=Callback::new(move |_| {
                                        let p = tp_restore.clone();
                                        spawn_local(async move {
                                            state.restore_trash(p).await;
                                        });
                                    })
                                />
                                <Button
                                    label="彻底删除"
                                    icon=IconName::TRASH
                                    size=Size::Sm
                                    variant=ButtonVariant::Plain
                                    on_click=Callback::new(move |_| {
                                        state.confirm_dialog.set(Some(ConfirmDialogState {
                                            kind: ConfirmDialogKind::DeleteForever { trash_path: tp_del.clone() },
                                            title: format!("彻底删除「{name_del}」？"),
                                            description: "此操作将从磁盘永久移除该文件，无法恢复。".to_string(),
                                            confirm_label: "永久删除".to_string(),
                                        }));
                                    })
                                />
                            </div>
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }
    .into_any()
}
