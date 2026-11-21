//! 弹窗与全局上下文菜单（新建/重命名 Dialog、危险确认 Dialog、快捷键速查、右键 Menu）。

use glass_design::prelude::*;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::state::{
    AppState, ConfirmDialogKind, ContextMenuKind, InputDialogKind, NavPage,
};
use crate::vault::fs::dirname;

#[component]
pub fn AppDialogsAndMenus() -> impl IntoView {
    let state = AppState::get();

    // 输入弹窗值
    let input_val = RwSignal::new(String::new());
    Effect::new(move |_| {
        if let Some(dlg) = state.input_dialog.get() {
            input_val.set(dlg.initial);
        }
    });

    let submit_input = move || {
        let Some(dlg) = state.input_dialog.get_untracked() else {
            return;
        };
        let val = input_val.get_untracked().trim().to_string();
        if val.is_empty() && !matches!(dlg.kind, InputDialogKind::NewNote { .. } | InputDialogKind::NewFolder { .. }) {
            toast("名称不能为空");
            return;
        }
        state.input_dialog.set(None);
        spawn_local(async move {
            match dlg.kind {
                InputDialogKind::NewNote { dir } => {
                    let name = if val.is_empty() { "未命名笔记".to_string() } else { val };
                    state.create_note(dir, name).await;
                }
                InputDialogKind::NewFolder { dir } => {
                    let name = if val.is_empty() { "新建文件夹".to_string() } else { val };
                    state.create_folder(dir, name).await;
                }
                InputDialogKind::RenameNote { path } => {
                    state.rename_note_to(path, val).await;
                }
                InputDialogKind::RenameFolder { path } => {
                    state.rename_folder_to(path, val).await;
                }
                InputDialogKind::CreateVault => {
                    let name = if val.is_empty() { "新仓库".to_string() } else { val };
                    state.create_local_vault(name);
                }
            }
        });
    };

    let submit_confirm = move || {
        let Some(dlg) = state.confirm_dialog.get_untracked() else {
            return;
        };
        state.confirm_dialog.set(None);
        spawn_local(async move {
            match dlg.kind {
                ConfirmDialogKind::DeleteFolder { path } => {
                    state.delete_folder(path).await;
                }
                ConfirmDialogKind::DeleteForever { trash_path } => {
                    state.delete_forever(trash_path).await;
                }
                ConfirmDialogKind::EmptyTrash => {
                    state.empty_trash().await;
                }
            }
        });
    };

    view! {
        // 1. 新建 / 重命名输入弹窗
        {move || {
            let dlg = state.input_dialog.get();
            let is_open = dlg.is_some();
            let title = dlg.as_ref().map(|d| d.title.clone()).unwrap_or_default();
            let desc = dlg.as_ref().map(|d| d.description.clone()).unwrap_or_default();
            let placeholder = dlg.as_ref().map(|d| d.placeholder.clone()).unwrap_or_default();
            let confirm_label = dlg.as_ref().map(|d| d.confirm_label.clone()).unwrap_or_else(|| "确定".to_string());

            view! {
                <Dialog
                    open=is_open
                    title=title
                    description=desc
                    width=380
                    on_close=Callback::new(move |_| state.input_dialog.set(None))
                    actions=ToChildren::to_children(move || view! {
                        <Button
                            label="取消"
                            variant=ButtonVariant::Plain
                            on_click=Callback::new(move |_| state.input_dialog.set(None))
                        />
                        <Button
                            label=confirm_label.clone()
                            variant=ButtonVariant::Primary
                            on_click=Callback::new(move |_| submit_input())
                        />
                    })
                >
                    <Field
                        value=Signal::derive(move || input_val.get())
                        placeholder=placeholder
                        label="名称"
                        on_input=Callback::new(move |v| input_val.set(v))
                        on_enter=Callback::new(move |_| submit_input())
                    />
                </Dialog>
            }
        }}

        // 2. 危险操作确认弹窗
        {move || {
            let dlg = state.confirm_dialog.get();
            let is_open = dlg.is_some();
            let title = dlg.as_ref().map(|d| d.title.clone()).unwrap_or_default();
            let desc = dlg.as_ref().map(|d| d.description.clone()).unwrap_or_default();
            let confirm_label = dlg.as_ref().map(|d| d.confirm_label.clone()).unwrap_or_else(|| "确定".to_string());

            view! {
                <Dialog
                    open=is_open
                    title=title
                    description=desc
                    width=360
                    on_close=Callback::new(move |_| state.confirm_dialog.set(None))
                    actions=ToChildren::to_children(move || view! {
                        <Button
                            label="取消"
                            variant=ButtonVariant::Plain
                            on_click=Callback::new(move |_| state.confirm_dialog.set(None))
                        />
                        <Button
                            label=confirm_label.clone()
                            variant=ButtonVariant::Danger
                            on_click=Callback::new(move |_| submit_confirm())
                        />
                    })
                />
            }
        }}

        // 3. 快捷键速查弹窗
        <Dialog
            open=Signal::derive(move || state.shortcuts_open.get())
            title="快捷键速查"
            description="所有快捷键在 Windows / Linux 上用 Ctrl，在 macOS 上用 ⌘"
            width=480
            on_close=Callback::new(move |_| state.shortcuts_open.set(false))
            actions=ToChildren::to_children(move || view! {
                <Button
                    label="知道了"
                    variant=ButtonVariant::Primary
                    on_click=Callback::new(move |_| state.shortcuts_open.set(false))
                />
            })
        >
            <div class="ink-shortcuts-body">
                <PropList title="导航与窗口">
                    <PropRow label="快速打开 / 命令面板"><span class="c-kbd">"Ctrl+P / Ctrl+O"</span></PropRow>
                    <PropRow label="聚焦文件搜索"><span class="c-kbd">"Ctrl+K"</span></PropRow>
                    <PropRow label="收起 / 展开导航栏"><span class="c-kbd">"Ctrl+\\"</span></PropRow>
                    <PropRow label="收起 / 展开文件列表"><span class="c-kbd">"Ctrl+Shift+B"</span></PropRow>
                    <PropRow label="收起 / 展开右侧检查器"><span class="c-kbd">"Ctrl+Shift+E"</span></PropRow>
                    <PropRow label="历史后退 / 前进"><span class="c-kbd">"Alt+← / Alt+→"</span></PropRow>
                </PropList>
                <PropList title="笔记与编辑">
                    <PropRow label="新建笔记 / 文件夹"><span class="c-kbd">"Ctrl+N / Ctrl+Shift+N"</span></PropRow>
                    <PropRow label="立即保存"><span class="c-kbd">"Ctrl+S"</span></PropRow>
                    <PropRow label="关闭当前标签页"><span class="c-kbd">"Ctrl+W"</span></PropRow>
                    <PropRow label="单篇查找 / 替换"><span class="c-kbd">"Ctrl+F / Ctrl+H"</span></PropRow>
                    <PropRow label="加粗 / 斜体"><span class="c-kbd">"Ctrl+B / Ctrl+I"</span></PropRow>
                    <PropRow label="字号缩放 / 复位"><span class="c-kbd">"Ctrl+± / Ctrl+0"</span></PropRow>
                </PropList>
            </div>
        </Dialog>

        // 4. 上下文右键菜单（使用 GlassDesign Menu 坐标入口，共享 OverlayHost）
        {move || {
            state.context_menu.get().map(|cm| {
                let items = build_menu_items(state, &cm.kind);
                let kind = cm.kind.clone();
                view! {
                    <Menu
                        open=Signal::derive(|| true)
                        anchor=Signal::derive(move || (cm.x, cm.y, 190.0))
                        items=items
                        label="操作菜单"
                        focus_first=false
                        on_select=Callback::new(move |id: String| {
                            state.context_menu.set(None);
                            handle_menu_action(state, &kind, &id);
                        })
                        on_close=Callback::new(move |_| state.context_menu.set(None))
                    />
                }
            })
        }}
    }
}

fn build_menu_items(state: AppState, kind: &ContextMenuKind) -> Vec<MenuItem> {
    match kind {
        ContextMenuKind::TreeFile {
            is_md, favorite, ..
        } => {
            let mut items = vec![MenuItem::new("open", "打开").icon(IconName::FILE_TEXT)];
            if *is_md {
                items.push(
                    MenuItem::new("fav", if *favorite { "取消收藏" } else { "添加到收藏" })
                        .icon(if *favorite {
                            IconName::STAR_OFF
                        } else {
                            IconName::STAR
                        }),
                );
                items.push(MenuItem::new("dup", "创建副本").icon(IconName::COPY));
                items.push(MenuItem::new("rename", "重命名").icon(IconName::SQUARE_PEN));
            }
            if state.is_desktop {
                items.push(
                    MenuItem::new("reveal", "在资源管理器中显示")
                        .icon(IconName::FOLDER_OPEN)
                        .separator_before(),
                );
                items.push(MenuItem::new("copy-path", "复制绝对路径").icon(IconName::COPY));
            }
            items.push(
                MenuItem::new("trash", "移到回收站")
                    .icon(IconName::TRASH)
                    .danger()
                    .separator_before(),
            );
            items
        }
        ContextMenuKind::TreeDir { .. } => {
            let mut items = vec![
                MenuItem::new("new-note", "在此新建笔记").icon(IconName::PLUS),
                MenuItem::new("new-folder", "在此新建子文件夹").icon(IconName::FOLDER_PLUS),
                MenuItem::new("rename", "重命名文件夹")
                    .icon(IconName::SQUARE_PEN)
                    .separator_before(),
            ];
            if state.is_desktop {
                items.push(
                    MenuItem::new("reveal", "在资源管理器中打开").icon(IconName::FOLDER_OPEN),
                );
            }
            items.push(
                MenuItem::new("delete-dir", "删除文件夹")
                    .icon(IconName::TRASH)
                    .danger()
                    .separator_before(),
            );
            items
        }
        ContextMenuKind::TreeRoot => {
            let mut items = vec![
                MenuItem::new("new-note", "新建笔记")
                    .icon(IconName::PLUS)
                    .shortcut("Ctrl+N"),
                MenuItem::new("new-folder", "新建文件夹")
                    .icon(IconName::FOLDER_PLUS)
                    .shortcut("Ctrl+Shift+N"),
                MenuItem::new("rescan", "刷新仓库")
                    .icon(IconName::REFRESH_CW)
                    .separator_before(),
            ];
            if state.is_desktop {
                items.push(
                    MenuItem::new("reveal", "在资源管理器中打开仓库").icon(IconName::FOLDER_OPEN),
                );
            }
            items
        }
        ContextMenuKind::Tab { .. } => {
            let mut items = vec![
                MenuItem::new("close", "关闭标签页")
                    .icon(IconName::X)
                    .shortcut("Ctrl+W"),
                MenuItem::new("close-others", "关闭其他标签页").icon(IconName::LAYERS),
            ];
            if state.is_desktop {
                items.push(
                    MenuItem::new("reveal", "在资源管理器中定位")
                        .icon(IconName::FOLDER_OPEN)
                        .separator_before(),
                );
            }
            items
        }
        ContextMenuKind::VaultSwitcher => {
            let mut items = Vec::new();
            if state.is_desktop {
                items.push(MenuItem::new("open-folder", "打开本地文件夹…").icon(IconName::FOLDER_OPEN));
                items.push(MenuItem::new("reveal-vault", "在资源管理器中打开").icon(IconName::EXTERNAL_LINK));
                items.push(MenuItem::new("copy-vault", "复制仓库路径").icon(IconName::COPY));
            } else {
                for v in state.list_browser_vaults() {
                    let active = v.name == state.vault_name();
                    items.push(
                        MenuItem::new(format!("switch:{}", v.id), v.name)
                            .icon(IconName::HARD_DRIVE)
                            .checked(active),
                    );
                }
                items.push(
                    MenuItem::new("create-vault", "新建浏览器仓库…")
                        .icon(IconName::PLUS)
                        .separator_before(),
                );
            }
            items.push(
                MenuItem::new("settings", "偏好设置…")
                    .icon(IconName::SLIDERS)
                    .separator_before(),
            );
            items
        }
        ContextMenuKind::Global => vec![
            MenuItem::new("quick-open", "快速打开笔记…")
                .icon(IconName::SEARCH)
                .shortcut("Ctrl+P"),
            MenuItem::new("new-note", "新建笔记")
                .icon(IconName::PLUS)
                .shortcut("Ctrl+N"),
            MenuItem::new("rescan", "刷新仓库")
                .icon(IconName::REFRESH_CW)
                .separator_before(),
            MenuItem::new("settings", "偏好设置")
                .icon(IconName::SLIDERS)
                .separator_before(),
        ],
    }
}

fn handle_menu_action(state: AppState, kind: &ContextMenuKind, id: &str) {
    match kind {
        ContextMenuKind::TreeFile { path, .. } => {
            let p = path.clone();
            match id {
                "open" => spawn_local(async move {
                    state.open_file(&p, true).await;
                }),
                "fav" => spawn_local(async move {
                    state.toggle_favorite(p).await;
                }),
                "dup" => spawn_local(async move {
                    state.duplicate_note(p).await;
                }),
                "rename" => {
                    let cur = state.note_at(&p).map(|n| n.title).unwrap_or_default();
                    state.input_dialog.set(Some(crate::state::InputDialogState {
                        kind: InputDialogKind::RenameNote { path: p },
                        title: "重命名笔记".to_string(),
                        description: "输入新的笔记名称".to_string(),
                        placeholder: "笔记名称".to_string(),
                        initial: cur,
                        confirm_label: "重命名".to_string(),
                    }));
                }
                "reveal" => state.open_in_explorer(p),
                "copy-path" => state.copy_vault_path(p),
                "trash" => spawn_local(async move {
                    state.trash_note(p).await;
                }),
                _ => {}
            }
        }
        ContextMenuKind::TreeDir { path } => {
            let p = path.clone();
            match id {
                "new-note" => state.input_dialog.set(Some(crate::state::InputDialogState {
                    kind: InputDialogKind::NewNote { dir: p },
                    title: "新建笔记".to_string(),
                    description: "输入笔记名称".to_string(),
                    placeholder: "未命名笔记".to_string(),
                    initial: String::new(),
                    confirm_label: "创建".to_string(),
                })),
                "new-folder" => state.input_dialog.set(Some(crate::state::InputDialogState {
                    kind: InputDialogKind::NewFolder { dir: p },
                    title: "新建子文件夹".to_string(),
                    description: "输入文件夹名称".to_string(),
                    placeholder: "新建文件夹".to_string(),
                    initial: String::new(),
                    confirm_label: "创建".to_string(),
                })),
                "rename" => {
                    let cur = crate::vault::fs::basename(&p);
                    state.input_dialog.set(Some(crate::state::InputDialogState {
                        kind: InputDialogKind::RenameFolder { path: p },
                        title: "重命名文件夹".to_string(),
                        description: "输入新的文件夹名称".to_string(),
                        placeholder: "文件夹名称".to_string(),
                        initial: cur,
                        confirm_label: "重命名".to_string(),
                    }));
                }
                "reveal" => state.open_in_explorer(p),
                "delete-dir" => state.confirm_dialog.set(Some(crate::state::ConfirmDialogState {
                    kind: ConfirmDialogKind::DeleteFolder { path: p.clone() },
                    title: format!("删除文件夹「{}」？", crate::vault::fs::basename(&p)),
                    description: "将永久删除此文件夹及其内部的全部文件，无法恢复。".to_string(),
                    confirm_label: "永久删除".to_string(),
                })),
                _ => {}
            }
        }
        ContextMenuKind::TreeRoot => match id {
            "new-note" => state.input_dialog.set(Some(crate::state::InputDialogState {
                kind: InputDialogKind::NewNote { dir: String::new() },
                title: "新建笔记".to_string(),
                description: "输入笔记名称".to_string(),
                placeholder: "未命名笔记".to_string(),
                initial: String::new(),
                confirm_label: "创建".to_string(),
            })),
            "new-folder" => state.input_dialog.set(Some(crate::state::InputDialogState {
                kind: InputDialogKind::NewFolder { dir: String::new() },
                title: "新建文件夹".to_string(),
                description: "输入文件夹名称".to_string(),
                placeholder: "新建文件夹".to_string(),
                initial: String::new(),
                confirm_label: "创建".to_string(),
            })),
            "rescan" => spawn_local(async move {
                state.rescan_vault(true).await;
            }),
            "reveal" => state.open_in_explorer(String::new()),
            _ => {}
        },
        ContextMenuKind::Tab { path } => {
            let p = path.clone();
            match id {
                "close" => spawn_local(async move {
                    state.close_tab(&p).await;
                }),
                "close-others" => spawn_local(async move {
                    state.close_other_tabs(&p).await;
                }),
                "reveal" => state.open_in_explorer(p),
                _ => {}
            }
        }
        ContextMenuKind::VaultSwitcher => match id {
            "open-folder" => state.pick_folder(),
            "reveal-vault" => state.open_in_explorer(String::new()),
            "copy-vault" => state.copy_vault_path(String::new()),
            "create-vault" => state.input_dialog.set(Some(crate::state::InputDialogState {
                kind: InputDialogKind::CreateVault,
                title: "新建浏览器仓库".to_string(),
                description: "在浏览器本地存储中创建一个新的示例知识库".to_string(),
                placeholder: "我的知识库".to_string(),
                initial: String::new(),
                confirm_label: "创建".to_string(),
            })),
            "settings" => state.nav_page.set(NavPage::Settings),
            other if other.starts_with("switch:") => {
                let vid = other.trim_start_matches("switch:").to_string();
                state.switch_local_vault(vid);
            }
            _ => {}
        },
        ContextMenuKind::Global => match id {
            "quick-open" => state.quick_open.set(true),
            "new-note" => {
                let dir = state
                    .active_tab
                    .get_untracked()
                    .map(|p| dirname(&p))
                    .unwrap_or_default();
                state.input_dialog.set(Some(crate::state::InputDialogState {
                    kind: InputDialogKind::NewNote { dir },
                    title: "新建笔记".to_string(),
                    description: "输入笔记名称".to_string(),
                    placeholder: "未命名笔记".to_string(),
                    initial: String::new(),
                    confirm_label: "创建".to_string(),
                }));
            }
            "rescan" => spawn_local(async move {
                state.rescan_vault(true).await;
            }),
            "settings" => state.nav_page.set(NavPage::Settings),
            _ => {}
        },
    }
}
