//! 多文档标签页栏（复用 GlassDesign .c-documenttabs 样式契约，完全响应式）。

use glass_design::prelude::*;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::state::{AppState, ContextMenuKind, ContextMenuState, InputDialogKind, InputDialogState};
use crate::vault::fs::dirname;

#[component]
pub fn TabsBar() -> impl IntoView {
    let state = AppState::get();
    let drag_path = RwSignal::new(None::<String>);

    view! {
        <div class="ink-tabs-wrap">
            <div
                class="c-documenttabs"
                role="tablist"
                aria-label="已打开的文档"
                data-dragging=move || drag_path.get().is_some().then_some("1")
            >
                <div class="c-documenttabs__scroll">
                    {move || {
                        let tabs = state.open_tabs.get();
                        if tabs.is_empty() {
                            return view! {
                                <div class="c-documenttabs__empty">"没有打开的文档"</div>
                            }.into_any();
                        }
                        tabs.into_iter().map(|tab| {
                            let path_sel = tab.path.clone();
                            let path_click = tab.path.clone();
                            let path_mid = tab.path.clone();
                            let path_close = tab.path.clone();
                            let path_ctx = tab.path.clone();
                            let path_drag = tab.path.clone();
                            let path_drop = tab.path.clone();
                            let label = tab.label.clone();
                            let label_close = tab.label.clone();
                            let dirty = tab.dirty;
                            let title = if dirty {
                                format!("{} — 未保存", tab.label)
                            } else {
                                tab.path.clone()
                            };

                            view! {
                                <div
                                    class=move || {
                                        if drag_path.get().as_deref() == Some(path_drag.as_str()) {
                                            "c-documenttabs__tab is-dragging"
                                        } else {
                                            "c-documenttabs__tab"
                                        }
                                    }
                                    role="tab"
                                    draggable="true"
                                    title=title
                                    aria-selected=move || (state.active_tab.get().as_deref() == Some(path_sel.as_str())).to_string()
                                    on:click=move |_| {
                                        let p = path_click.clone();
                                        spawn_local(async move {
                                            state.open_file(&p, true).await;
                                        });
                                    }
                                    on:auxclick=move |ev: web_sys::MouseEvent| {
                                        if ev.button() == 1 {
                                            ev.prevent_default();
                                            let p = path_mid.clone();
                                            spawn_local(async move {
                                                state.close_tab(&p).await;
                                            });
                                        }
                                    }
                                    on:contextmenu=move |ev: web_sys::MouseEvent| {
                                        ev.prevent_default();
                                        ev.stop_propagation();
                                        state.context_menu.set(Some(ContextMenuState {
                                            x: ev.client_x() as f64,
                                            y: ev.client_y() as f64,
                                            kind: ContextMenuKind::Tab { path: path_ctx.clone() },
                                        }));
                                    }
                                    on:dragstart={
                                        let p = tab.path.clone();
                                        move |_| drag_path.set(Some(p.clone()))
                                    }
                                    on:dragend=move |_| drag_path.set(None)
                                    on:dragover=move |ev: web_sys::DragEvent| {
                                        if drag_path.get_untracked().is_some() {
                                            ev.prevent_default();
                                        }
                                    }
                                    on:drop=move |ev: web_sys::DragEvent| {
                                        ev.prevent_default();
                                        let Some(src) = drag_path.get_untracked() else { return };
                                        drag_path.set(None);
                                        if src == path_drop {
                                            return;
                                        }
                                        let mut order: Vec<String> = state
                                            .open_tabs
                                            .get_untracked()
                                            .into_iter()
                                            .map(|t| t.path)
                                            .collect();
                                        if let (Some(from_idx), Some(to_idx)) = (
                                            order.iter().position(|p| p == &src),
                                            order.iter().position(|p| p == &path_drop),
                                        ) {
                                            let item = order.remove(from_idx);
                                            order.insert(to_idx, item);
                                            state.reorder_tabs(order);
                                        }
                                    }
                                >
                                    <span class="c-documenttabs__label">
                                        <span class="c-documenttabs__title">{label}</span>
                                        {dirty.then(|| view! {
                                            <span class="c-documenttabs__dirty" aria-hidden="true"></span>
                                        })}
                                    </span>
                                    <button
                                        class="c-documenttabs__close"
                                        type="button"
                                        tabindex="-1"
                                        aria-label=format!("关闭 {label_close}")
                                        title=format!("关闭 {label_close} (Ctrl+W)")
                                        on:click=move |ev: web_sys::MouseEvent| {
                                            ev.stop_propagation();
                                            let p = path_close.clone();
                                            spawn_local(async move {
                                                state.close_tab(&p).await;
                                            });
                                        }
                                    >
                                        <Icon name=IconName::X size=9 />
                                    </button>
                                </div>
                            }
                        }).collect_view().into_any()
                    }}
                </div>
                <button
                    class="c-documenttabs__new"
                    type="button"
                    aria-label="新建笔记"
                    title="新建笔记 (Ctrl+N)"
                    on:click=move |_| {
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
                >
                    <Icon name=IconName::PLUS size=12 />
                </button>
            </div>
        </div>
    }
}
