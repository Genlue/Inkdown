//! 底部状态栏（基于 GlassDesign StatusBar + StatusItem）。

use glass_design::prelude::*;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::markdown::{char_count, line_count};
use crate::state::{AppState, SaveStatus};

#[component]
pub fn AppStatusBar() -> impl IntoView {
    let state = AppState::get();

    view! {
        <StatusBar
            right=ToChildren::to_children(move || {
                view! {
                    {move || {
                        let has_md = state.editor_path.get().is_some();
                        if !has_md {
                            return None;
                        }
                        let content = state.editor_content.get();
                        let chars = char_count(&content);
                        let lines = line_count(&content);
                        let (cur_l, cur_c) = state.cursor_pos.get();
                        let fs = state.editor_font_size.get().round() as i32;
                        Some(view! {
                            <div class="l-row l-row--2">
                                <StatusItem
                                    label="光标"
                                    value=format!("第 {cur_l} 行, 第 {cur_c} 列")
                                />
                                <StatusItem
                                    label="统计"
                                    value=format!("{chars} 字 · {lines} 行")
                                />
                                <StatusItem
                                    icon=IconName::ZOOM_IN
                                    label="字号"
                                    value=format!("{fs}px")
                                    on_click=Callback::new(move |_| {
                                        state.set_editor_font_size(14.0);
                                        toast("编辑器字号已复位 (14px)");
                                    })
                                />
                            </div>
                        })
                    }}
                }
            })
        >
            {move || {
                let (label, val, tone) = match state.save_status.get() {
                    SaveStatus::Saved => ("状态", "已保存", StatusTone::Success),
                    SaveStatus::Saving => ("状态", "保存中…", StatusTone::Accent),
                    SaveStatus::Dirty => ("状态", "未保存", StatusTone::Warning),
                };
                view! {
                    <StatusItem
                        icon=IconName::CIRCLE_CHECK
                        label=label
                        value=val
                        live=tone
                        on_click=Callback::new(move |_| {
                            spawn_local(async move {
                                state.commit_doc().await;
                                toast("已保存");
                            });
                        })
                    />
                    <StatusItem
                        icon=IconName::HARD_DRIVE
                        label="仓库"
                        value=state.vault_name()
                        on_click=Callback::new(move |_| {
                            if state.is_desktop {
                                state.open_in_explorer(String::new());
                            } else {
                                state.pick_folder();
                            }
                        })
                    />
                }
            }}
        </StatusBar>
    }
}
