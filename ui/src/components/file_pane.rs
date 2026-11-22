//! 中间文件列表栏：搜索、最近打开、文件树（拖拽移动 + 右键菜单 + 引导线）、收藏、标签聚合、回收站。

use glass_design::prelude::*;
use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::components::icons::{ICON_CHEVRONS_UP_DOWN, ICON_CODE, ICON_IMMERSIVE, ICON_TABLE};
use crate::markdown::{plain_excerpt, rel_time};
use crate::state::{
    AppState, ConfirmDialogKind, ConfirmDialogState, ContextMenuKind, ContextMenuState,
    InputDialogKind, InputDialogState, NavPage,
};
use crate::vault::fs::{dirname, FileType};
use crate::vault::VaultTreeNode;

/// 沉浸模式下的「可读范围」。树是递归渲染的，每行只问一句：
/// 这个节点该不该被盖住。
#[derive(Clone, Default)]
struct CoverCtx {
    on: bool,
    focus: String,
    keep_ancestors: bool,
    keep_inside: bool,
    active_file: Option<String>,
}

impl CoverCtx {
    fn covered(&self, path: &str, is_dir: bool) -> bool {
        if !self.on {
            return false;
        }
        if !is_dir && self.active_file.as_deref() == Some(path) {
            return false;
        }
        let parent = if is_dir {
            path.to_string()
        } else {
            dirname(path)
        };
        if parent == self.focus {
            return false;
        }
        if self.keep_inside && parent.starts_with(&format!("{}/", self.focus)) {
            return false;
        }
        if self.keep_ancestors {
            let mut cur = self.focus.clone();
            while !cur.is_empty() {
                if cur == parent {
                    return false;
                }
                cur = dirname(&cur);
            }
        }
        true
    }
}

/// 屏幕上从上到下能看到的节点顺序，Shift 连选按它取区间。
fn flatten_visible(
    nodes: &[VaultTreeNode],
    expanded: &std::collections::HashSet<String>,
    out: &mut Vec<String>,
) {
    for node in nodes {
        out.push(node.path.clone());
        if node.is_dir && expanded.contains(&node.path) {
            flatten_visible(&node.children, expanded, out);
        }
    }
}

fn collect_dirs(nodes: &[VaultTreeNode], out: &mut Vec<(String, String)>) {
    for node in nodes {
        if node.is_dir {
            out.push((node.path.clone(), node.path.clone()));
            collect_dirs(&node.children, out);
        }
    }
}

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
                        {move || {
                            let on = state.immersive.get();
                            view! {
                                <IconButton
                                    icon=ICON_IMMERSIVE
                                    label=if on { "退出沉浸文件夹" } else { "沉浸文件夹：只留当前目录" }
                                    size=Size::Sm
                                    toggle=true
                                    active=on
                                    on_click=Callback::new(move |_| state.toggle_immersive())
                                />
                            }
                        }}
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

            <Show when=move || state.select_mode.get() || !state.selection.get().is_empty()>
                {move || {
                    let mut paths: Vec<String> = state.selection.get().into_iter().collect();
                    paths.sort();
                    let count = paths.len();
                    let count_label = if count == 0 {
                        "点选文件或文件夹".to_string()
                    } else {
                        format!("已选 {count}")
                    };
                    let exit_label = if state.select_mode.get() {
                        "退出批量选择"
                    } else {
                        "取消选择"
                    };
                    let for_move = paths.clone();
                    let for_fav = paths.clone();
                    let for_trash = paths.clone();
                    let dirs = {
                        let mut out = Vec::new();
                        collect_dirs(&state.tree(), &mut out);
                        out
                    };
                    let move_items: Vec<MenuItem> = if dirs.is_empty() {
                        vec![MenuItem::new("__root", "仓库根目录").icon(IconName::HARD_DRIVE)]
                    } else {
                        let mut items = vec![MenuItem::new("__root", "仓库根目录").icon(IconName::HARD_DRIVE)];
                        for (path, label) in dirs {
                            items.push(MenuItem::new(path, label).icon(IconName::FOLDER));
                        }
                        items
                    };
                    view! {
                        <div class="ink-batch">
                            <span class="ink-batch__count">{count_label}</span>
                            <span class="l-push"></span>
                            <MenuButton
                                icon=IconName::MOVE
                                label="移动"
                                class="c-btn--sm"
                                items=move_items
                                on_select=Callback::new(move |dst: String| {
                                    let target = if dst == "__root" { String::new() } else { dst };
                                    let picked = for_move.clone();
                                    spawn_local(async move {
                                        state.move_items(picked, target).await;
                                    });
                                })
                            />
                            <IconButton
                                icon=IconName::STAR
                                label="批量收藏"
                                size=Size::Sm
                                on_click=Callback::new(move |_| {
                                    let picked = for_fav.clone();
                                    spawn_local(async move {
                                        state.batch_favorite(picked, true).await;
                                    });
                                })
                            />
                            <IconButton
                                icon=IconName::TRASH
                                label="批量移到回收站"
                                size=Size::Sm
                                on_click=Callback::new(move |_| {
                                    let picked = for_trash.clone();
                                    spawn_local(async move {
                                        state.batch_trash(picked).await;
                                    });
                                })
                            />
                            <IconButton
                                icon=IconName::X
                                label=exit_label
                                size=Size::Sm
                                on_click=Callback::new(move |_| state.clear_selection())
                            />
                        </div>
                    }
                }}
            </Show>

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
    let expanded = state.expanded_dirs.get();
    let mut visible: Vec<String> = Vec::new();
    flatten_visible(&tree, &expanded, &mut visible);
    let visible = StoredValue::new_local(visible);

    // 沉浸范围只算一次，递归渲染时每行查表
    let focus = state.focus_dir();
    let cover = CoverCtx {
        on: state.immersive.get() && !focus.is_empty(),
        focus,
        keep_ancestors: state.immersive_keep_ancestors.get(),
        keep_inside: state.immersive_keep_inside.get(),
        active_file: state.active_tab.get(),
    };

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
                        data-immersive=if cover.on { "1" } else { "0" }
                        data-color-style=move || state.folder_color_style.get()
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
                                    let picked = batch_payload(state, &src);
                                    if picked.len() > 1 {
                                        state.move_items(picked, String::new()).await;
                                    } else {
                                        state.move_item(src, String::new()).await;
                                    }
                                });
                            }
                        }
                    >
                        {tree.into_iter().map(|node| render_tree_node(state, node, 0, drag_src, drop_target, cover.clone(), visible)).collect_view()}
                    </div>
                }.into_any()
            }}
        </div>
    }.into_any()
}

fn batch_payload(state: AppState, src: &str) -> Vec<String> {
    let sel = state.selection.get_untracked();
    if sel.len() > 1 && sel.contains(src) {
        let mut picked: Vec<String> = sel.into_iter().collect();
        picked.sort();
        picked
    } else {
        vec![src.to_string()]
    }
}

fn render_tree_node(
    state: AppState,
    node: VaultTreeNode,
    depth: usize,
    drag_src: RwSignal<Option<String>>,
    drop_target: RwSignal<Option<String>>,
    cover: CoverCtx,
    visible: StoredValue<Vec<String>, leptos::reactive::owner::LocalStorage>,
) -> AnyView {
    if node.is_dir {
        let path = node.path.clone();
        let path_click = path.clone();
        let path_shift = path.clone();
        let path_ctx = path.clone();
        let path_drag = path.clone();
        let path_over = path.clone();
        let path_leave = path.clone();
        let path_drop = path.clone();
        let path_drop_style = path.clone();
        let path_open = path.clone();
        let path_chev = path.clone();
        let path_children = path.clone();
        let children = node.children;
        let name = node.name;
        let style = match node.color.as_deref() {
            Some(hex) => format!("--tree-depth:{depth};--folder-color:{hex}"),
            None => format!("--tree-depth:{depth}"),
        };
        let covered = cover.covered(&path, true);
        let cover_for_children = cover.clone();

        return view! {
            <div class="ink-tree__branch" style=format!("--tree-depth:{depth}")>
                <div
                    class="ink-tree__row"
                    role="treeitem"
                    draggable="true"
                    data-type="dir"
                    data-path=path.clone()
                    data-cover=if covered { "1" } else { "0" }
                    data-drop=move || (drop_target.get().as_deref() == Some(path_drop_style.as_str())).then_some("1")
                    style=style
                    on:click=move |ev: web_sys::MouseEvent| {
                        // Ctrl / Cmd 加选、Shift 连选；其余保持「点一下展开」
                        if ev.ctrl_key() || ev.meta_key() {
                            ev.prevent_default();
                            state.toggle_selection(&path_click);
                            return;
                        }
                        if ev.shift_key() {
                            ev.prevent_default();
                            let order = visible.get_value();
                            state.select_range(&path_shift, &order);
                            return;
                        }
                        // 批量选择模式：不用按修饰键，单击即勾选
                        if state.select_mode.get_untracked() {
                            state.toggle_selection(&path_click);
                            return;
                        }
                        if !state.selection.get_untracked().is_empty() {
                            state.clear_selection();
                        }
                        state.toggle_dir(&path_click);
                    }
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
                            let picked = batch_payload(state, &src);
                            let blocked = picked.iter().any(|p| {
                                p == &path_over || path_over.starts_with(&format!("{p}/"))
                            });
                            if !blocked {
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
                                let picked = batch_payload(state, &src);
                                if picked.len() > 1 {
                                    state.move_items(picked, dst).await;
                                } else {
                                    state.move_item(src, dst).await;
                                }
                            });
                        }
                    }
                >
                    <span
                        class=move || {
                            if state.expanded_dirs.get().contains(&path_open) {
                                "ink-tree__chevron is-open"
                            } else {
                                "ink-tree__chevron"
                            }
                        }
                        // 批量选择模式下整行是「勾选」，折叠交给箭头单独处理，
                        // 否则进不了折叠目录。
                        on:click=move |ev: web_sys::MouseEvent| {
                            ev.stop_propagation();
                            state.toggle_dir(&path_chev);
                        }
                    >
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
                            .map(|child| render_tree_node(state, child, depth + 1, drag_src, drop_target, cover_for_children.clone(), visible))
                            .collect_view()}
                    </div>
                </Show>
            </div>
        }
        .into_any();
    }

    let path = node.path.clone();
    let path_sel = path.clone();
    let path_multi = path.clone();
    let path_click = path.clone();
    let path_shift = path.clone();
    let path_dbl = path.clone();
    let path_ctx = path.clone();
    let path_drag = path.clone();
    let is_md = node.is_markdown;
    let favorite = node.favorite;
    let name = node.name.clone();
    let badge = (!is_md && !node.ext.is_empty()).then(|| node.ext.to_uppercase());
    let covered = cover.covered(&path, false);

    view! {
        <div
            class="ink-tree__row"
            role="treeitem"
            draggable="true"
            data-type="file"
            data-path=path
            data-cover=if covered { "1" } else { "0" }
            data-selected=move || {
                let picked = state.selection.get();
                (state.active_tab.get().as_deref() == Some(path_sel.as_str()) || picked.contains(&path_sel))
                    .then_some("1")
            }
            data-multi=move || {
                let picked = state.selection.get();
                (picked.contains(&path_multi)
                    && state.active_tab.get().as_deref() != Some(path_multi.as_str()))
                    .then_some("1")
            }
            style=format!("--tree-depth:{depth}")
            on:click=move |ev: web_sys::MouseEvent| {
                if ev.ctrl_key() || ev.meta_key() {
                    ev.prevent_default();
                    state.toggle_selection(&path_click);
                    return;
                }
                if ev.shift_key() {
                    ev.prevent_default();
                    let order = visible.get_value();
                    state.select_range(&path_shift, &order);
                    return;
                }
                if state.select_mode.get_untracked() {
                    state.toggle_selection(&path_click);
                    return;
                }
                if !state.selection.get_untracked().is_empty() {
                    state.clear_selection();
                }
                let p = path_click.clone();
                spawn_local(async move {
                    state.open_file(&p, true).await;
                });
            }
            on:dblclick=move |_| {
                if is_md && !state.select_mode.get_untracked() {
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
