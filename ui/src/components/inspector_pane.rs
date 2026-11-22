//! 右侧检查器：大纲（GlassDesign Outline）、关联（反链/出链/标签/悬空链接）、信息（PropList/PropRow）。

use glass_design::prelude::*;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::markdown::{abs_date, char_count, line_count};
use crate::state::AppState;
use crate::vault::links::{all_dangling_sorted, extract_headings, find_backlinks, find_outlinks};

#[component]
pub fn InspectorPane() -> impl IntoView {
    let state = AppState::get();

    let selected_tab = Signal::derive(move || state.inspector_tab.get());
    let on_tab = Callback::new(move |tab: String| state.set_inspector_tab(tab));
    let is_empty = Signal::derive(move || state.editor_path.get().is_none());

    let headings = Memo::new(move |_| {
        let content = state.editor_content.get();
        extract_headings(&content)
    });

    let outline_items = move || {
        let hs = headings.get();
        let min_lvl = hs.iter().map(|h| h.level).min().unwrap_or(1);
        hs.into_iter()
            .map(|h| {
                let depth = (h.level.saturating_sub(min_lvl)) as u32;
                OutlineItem::new(h.line.to_string(), h.text, depth)
            })
            .collect::<Vec<_>>()
    };

    let selected_heading_id = Signal::derive(move || {
        state
            .active_heading_line
            .get()
            .map(|l| l.to_string())
            .unwrap_or_default()
    });

    view! {
        <div class="ink-inspector-wrap">
            <Inspector
                title="笔记检查器"
                tabs=vec!["大纲".to_string(), "关联".to_string(), "信息".to_string()]
                selected_tab=selected_tab
                on_tab=on_tab
                empty=is_empty
                actions=ToChildren::to_children(move || {
                    view! {
                        <IconButton
                            icon=IconName::X
                            label="收起检查器"
                            size=Size::Sm
                            on_click=Callback::new(move |_| state.toggle_inspector())
                        />
                    }
                })
            >
                {move || match state.inspector_tab.get().as_str() {
                    "关联" => render_relations(state),
                    "信息" => render_info(state),
                    _ => {
                        let items = outline_items();
                        if items.is_empty() {
                            view! {
                                <Empty
                                    icon=IconName::LIST
                                    title="没有标题"
                                    desc="在笔记中写下 # 标题，这里会自动生成结构大纲"
                                />
                            }.into_any()
                        } else {
                            view! {
                                <Outline
                                    items=items
                                    selected=selected_heading_id
                                    on_select=Callback::new(move |id: String| {
                                        if let Ok(line) = id.parse::<usize>() {
                                            state.jump_to_line(line);
                                        }
                                    })
                                />
                            }.into_any()
                        }
                    }
                }}
            </Inspector>
        </div>
    }
}

fn render_relations(state: AppState) -> AnyView {
    let Some(path) = state.editor_path.get() else {
        return view! { <div></div> }.into_any();
    };
    if state.indexing.get() {
        return view! {
            <Center><Spinner /></Center>
        }
        .into_any();
    }

    let idx = state.link_index.get();
    let backlinks = find_backlinks(&idx, &path);
    let outlinks = find_outlinks(&idx, &path);
    let dangling = all_dangling_sorted(&idx);

    view! {
        <div class="ink-rel">
            <section class="ink-rel__section">
                <header class="ink-rel__head">
                    <span class="ink-rel__label">"反向链接"</span>
                    <span class="ink-rel__count">{backlinks.len()}</span>
                </header>
                {if backlinks.is_empty() {
                    view! { <div class="ink-rel__empty">"还没有其他笔记链接到这里"</div> }.into_any()
                } else {
                    view! {
                        <div class="ink-rel__body">
                            {backlinks.into_iter().map(|b| {
                                let p = b.path.clone();
                                let sub = b.snippets.first().cloned().unwrap_or_else(|| b.path.clone());
                                view! {
                                    <button
                                        type="button"
                                        class="ink-rel__row"
                                        on:click=move |_| {
                                            let target = p.clone();
                                            spawn_local(async move {
                                                state.open_file(&target, true).await;
                                            });
                                        }
                                    >
                                        <span class="ink-rel__row-title">{b.title}</span>
                                        <span class="ink-rel__row-sub">{sub}</span>
                                    </button>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
            </section>

            <section class="ink-rel__section">
                <header class="ink-rel__head">
                    <span class="ink-rel__label">"出链"</span>
                    <span class="ink-rel__count">{outlinks.len()}</span>
                </header>
                {if outlinks.is_empty() {
                    view! { <div class="ink-rel__empty">"这篇笔记里没有 [[双链]]"</div> }.into_any()
                } else {
                    view! {
                        <div class="ink-rel__body">
                            {outlinks.into_iter().map(|o| {
                                let tgt = o.target.clone();
                                let resolved = o.path.is_some();
                                let sub = o.path.unwrap_or_else(|| "点击新建这篇笔记".to_string());
                                view! {
                                    <button
                                        type="button"
                                        class=if resolved { "ink-rel__row" } else { "ink-rel__row is-dangling" }
                                        on:click=move |_| {
                                            let t = tgt.clone();
                                            spawn_local(async move {
                                                state.open_wikilink(t).await;
                                            });
                                        }
                                    >
                                        <span class="ink-rel__row-title">{o.title}</span>
                                        <span class="ink-rel__row-sub">{sub}</span>
                                    </button>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
            </section>

            {(!dangling.is_empty()).then(|| {
                view! {
                    <section class="ink-rel__section">
                        <header class="ink-rel__head">
                            <span class="ink-rel__label">"全库待建链接"</span>
                            <span class="ink-rel__count">{dangling.len()}</span>
                        </header>
                        <div class="ink-rel__body">
                            {dangling.into_iter().take(15).map(|d| {
                                let tgt = d.target.clone();
                                view! {
                                    <button
                                        type="button"
                                        class="ink-rel__row is-dangling"
                                        on:click=move |_| {
                                            let t = tgt.clone();
                                            spawn_local(async move {
                                                state.open_wikilink(t).await;
                                            });
                                        }
                                    >
                                        <span class="ink-rel__row-title">{d.target}</span>
                                        <span class="ink-rel__row-sub">{format!("被 {} 篇笔记引用", d.count)}</span>
                                    </button>
                                }
                            }).collect_view()}
                        </div>
                    </section>
                }
            })}
        </div>
    }
    .into_any()
}

fn render_info(state: AppState) -> AnyView {
    let Some(path) = state.editor_path.get() else {
        return view! { <div></div> }.into_any();
    };
    let Some(note) = state.note_at(&path) else {
        return view! { <div></div> }.into_any();
    };
    let content = state.editor_content.get();
    let chars = char_count(&content);
    let raw_len = content.chars().count();
    let lines = line_count(&content);
    let headings_cnt = extract_headings(&content).len();
    let minutes = (chars / 400).max(1);

    view! {
        <Stack>
            <PropList title="统计">
                <PropRow label="字数">{chars.to_string()}</PropRow>
                <PropRow label="字符">{raw_len.to_string()}</PropRow>
                <PropRow label="行数">{lines.to_string()}</PropRow>
                <PropRow label="标题">{headings_cnt.to_string()}</PropRow>
                <PropRow label="阅读时长">{format!("约 {minutes} 分钟")}</PropRow>
            </PropList>

            <PropList title="时间">
                <PropRow label="创建于">{abs_date(note.created_at)}</PropRow>
                <PropRow label="更新于">{abs_date(note.mtime)}</PropRow>
            </PropList>

            <PropList title="属性">
                <PropRow label="收藏">{if note.favorite { "是" } else { "否" }}</PropRow>
                <PropRow label="格式">"Markdown"</PropRow>
                <PropRow label="存储">{if state.is_desktop { "本地文件" } else { "浏览器沙盒" }}</PropRow>
            </PropList>

            <PropList title="位置">
                <PropRow label="路径">
                    <span class="c-props__value is-mono" title=note.path.clone()>{note.path.clone()}</span>
                </PropRow>
            </PropList>
        </Stack>
    }
    .into_any()
}
