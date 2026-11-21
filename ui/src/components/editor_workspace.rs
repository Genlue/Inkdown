//! Markdown 编辑与实时预览工作区（含查找替换、双链补全、图片粘贴拖拽、交互式任务与大纲联动）。

use glass_design::prelude::*;
use leptos::html;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::components::icons::{
    ICON_BOLD, ICON_CODE, ICON_HEADING, ICON_ITALIC, ICON_LINK, ICON_LIST_TODO, ICON_QUOTE,
    ICON_STRIKE,
};
use crate::markdown::{abs_date, rel_time, render_markdown};
use crate::state::{AppState, SaveStatus};
use crate::tauri_bridge::{copy_to_clipboard, open_external_url, request_animation_frame};
use crate::vault::fs::{basename, dirname, now_ms};
use crate::vault::links::extract_headings;

#[component]
pub fn EditorWorkspace() -> impl IntoView {
    let state = AppState::get();

    let editor_ref = NodeRef::<html::Textarea>::new();
    let editor_scroll_ref = NodeRef::<html::Div>::new();
    let preview_ref = NodeRef::<html::Article>::new();
    let preview_scroll_ref = NodeRef::<html::Div>::new();
    let image_input_ref = NodeRef::<html::Input>::new();
    let find_input_ref = NodeRef::<html::Input>::new();

    // 查找替换本地状态
    let find_query = RwSignal::new(String::new());
    let replace_query = RwSignal::new(String::new());
    let find_case = RwSignal::new(false);
    let find_matches = RwSignal::new(Vec::<(usize, usize)>::new());
    let find_idx = RwSignal::new(0usize);

    // [[ 双链自动提示状态
    let wiki_open = RwSignal::new(false);
    let wiki_query_start = RwSignal::new(0usize);
    let wiki_candidates = RwSignal::new(Vec::<(String, String)>::new()); // (title, path)
    let wiki_sel = RwSignal::new(0usize);

    let title_draft = RwSignal::new(String::new());
    Effect::new(move |_| {
        title_draft.set(state.editor_title.get());
    });

    // 同步编辑器高度以实现流式滚动
    let resize_editor = move || {
        let (Some(ed), Some(scroll)) = (editor_ref.get_untracked(), editor_scroll_ref.get_untracked()) else {
            return;
        };
        let prev_top = scroll.scroll_top();
        let el: &web_sys::HtmlElement = ed.as_ref();
        let _ = el.style().set_property("height", "auto");
        let h = el.scroll_height().max(360);
        let _ = el.style().set_property("height", &format!("{h}px"));
        if scroll.scroll_top() != prev_top {
            scroll.set_scroll_top(prev_top);
        }
    };

    Effect::new(move |_| {
        let _ = state.editor_content.get();
        let _ = state.editor_path.get();
        let _ = state.view_mode.get();
        let _ = state.editor_font_size.get();
        let resize = resize_editor;
        request_animation_frame(move || resize());
    });

    // 聚焦查找框
    Effect::new(move |_| {
        let tick = state.find_focus_tick.get();
        if tick > 0 {
            // 如果编辑器有选区，预填进查找框
            if let Some(ed) = editor_ref.get_untracked() {
                if let (Ok(Some(start)), Ok(Some(end))) = (ed.selection_start(), ed.selection_end()) {
                    let s = start as usize;
                    let e = end as usize;
                    if e > s && e <= ed.value().len() {
                        let val = ed.value();
                        if let Some(sub) = val.get(s..e) {
                            if !sub.contains('\n') && sub.chars().count() <= 80 {
                                find_query.set(sub.to_string());
                            }
                        }
                    }
                }
            }
            request_animation_frame(move || {
                if let Some(el) = find_input_ref.get_untracked() {
                    let _ = el.focus();
                    let _ = el.select();
                }
            });
        }
    });

    // 重算查找命中（注意用字节范围但转换到 UTF-16 offset 供 textarea selection API 使用）
    let recompute_matches = move || {
        let q = find_query.get_untracked();
        if q.is_empty() {
            find_matches.set(Vec::new());
            find_idx.set(0);
            return;
        }
        let text = state.editor_content.get_untracked();
        let case_sensitive = find_case.get_untracked();
        let matches = find_utf16_matches(&text, &q, case_sensitive);
        find_matches.set(matches);
        find_idx.set(0);
    };

    Effect::new(move |_| {
        let _ = find_query.get();
        let _ = find_case.get();
        let _ = state.editor_content.get();
        if state.find_open.get() {
            recompute_matches();
        }
    });

    let select_match_at = move |idx: usize| {
        let matches = find_matches.get_untracked();
        let Some(&(s, e)) = matches.get(idx) else {
            return;
        };
        if let Some(ed) = editor_ref.get_untracked() {
            let _ = ed.focus();
            let _ = ed.set_selection_range(s as u32, e as u32);
            if let Some(scroll) = editor_scroll_ref.get_untracked() {
                let text = ed.value();
                let line_no = utf16_offset_to_line(&text, s);
                let approx_y = (line_no as f64) * 24.0;
                let vh = scroll.client_height() as f64;
                scroll.set_scroll_top((approx_y - vh / 3.0).max(0.0) as i32);
            }
        }
    };

    let next_match = move || {
        let matches = find_matches.get_untracked();
        if matches.is_empty() {
            return;
        }
        let next = (find_idx.get_untracked() + 1) % matches.len();
        find_idx.set(next);
        select_match_at(next);
    };

    let prev_match = move || {
        let matches = find_matches.get_untracked();
        if matches.is_empty() {
            return;
        }
        let cur = find_idx.get_untracked();
        let prev = if cur == 0 { matches.len() - 1 } else { cur - 1 };
        find_idx.set(prev);
        select_match_at(prev);
    };

    // 跳转到指定行（由右侧大纲点击触发）
    Effect::new(move |_| {
        let Some((line, _seq)) = state.jump_to_line_req.get() else {
            return;
        };
        let mode = state.view_mode.get_untracked();
        if mode != "edit" {
            if let (Some(preview), Some(scroll)) =
                (preview_ref.get_untracked(), preview_scroll_ref.get_untracked())
            {
                if let Ok(Some(target)) =
                    preview.query_selector(&format!("[data-line=\"{line}\"]"))
                {
                    let c_rect = scroll.get_bounding_client_rect();
                    let t_rect = target.get_bounding_client_rect();
                    let next_top = (scroll.scroll_top() as f64) + (t_rect.top() - c_rect.top()) - 64.0;
                    scroll.set_scroll_top(next_top.max(0.0) as i32);
                }
            }
        }
        if mode != "preview" {
            if let (Some(ed), Some(scroll)) =
                (editor_ref.get_untracked(), editor_scroll_ref.get_untracked())
            {
                let text = ed.value();
                let total_lines = text.lines().count().max(1);
                let ratio = (line as f64) / (total_lines as f64);
                let range = (scroll.scroll_height() - scroll.client_height()).max(0) as f64;
                scroll.set_scroll_top((ratio * range).max(0.0) as i32);
                if let Some(utf16_pos) = line_start_utf16(&text, line) {
                    let _ = ed.focus();
                    let _ = ed.set_selection_range(utf16_pos as u32, utf16_pos as u32);
                }
            }
        }
    });

    // 解析预览区里的本地仓库图片
    let rendered_html = Memo::new(move |_| {
        let src = state.editor_content.get();
        render_markdown(&src)
    });

    Effect::new(move |_| {
        let _html = rendered_html.get();
        let current_path = state.editor_path.get().unwrap_or_default();
        let preview_el = preview_ref.get();
        let Some(preview_el) = preview_el else {
            return;
        };
        let cell = state.vault.get_value();
        let fs_opt = cell.borrow().as_ref().map(|v| v.fs.clone());
        let Some(fs) = fs_opt else { return };

        request_animation_frame(move || {
            let root: &web_sys::Element = preview_el.as_ref();
            let Ok(imgs) = root.query_selector_all("img[data-vault-src]") else {
                return;
            };
            let dir = dirname(&current_path);
            for i in 0..imgs.length() {
                let Some(node) = imgs.item(i) else { continue };
                let Ok(img) = node.dyn_into::<web_sys::HtmlImageElement>() else {
                    continue;
                };
                let raw = img.get_attribute("data-vault-src").unwrap_or_default();
                if raw.is_empty()
                    || raw.starts_with("http://")
                    || raw.starts_with("https://")
                    || raw.starts_with("data:")
                {
                    continue;
                }
                let clean = raw.replace('\\', "/").trim_start_matches('/').to_string();
                let mut candidates = vec![clean.clone()];
                if !dir.is_empty() {
                    candidates.push(format!("{dir}/{clean}"));
                }
                candidates.push(format!("assets/{clean}"));
                candidates.push(format!("attachments/{clean}"));

                let fs = fs.clone();
                spawn_local(async move {
                    for cand in candidates {
                        if fs.exists(&cand).await.unwrap_or(false) {
                            if let Ok(Some(b64)) = fs.read_binary(&cand).await {
                                let ext = cand.rsplit('.').next().unwrap_or("png").to_ascii_lowercase();
                                let mime = match ext.as_str() {
                                    "svg" => "image/svg+xml",
                                    "jpg" | "jpeg" => "image/jpeg",
                                    "gif" => "image/gif",
                                    "webp" => "image/webp",
                                    _ => "image/png",
                                };
                                let url = if b64.starts_with("data:") {
                                    b64
                                } else {
                                    format!("data:{mime};base64,{b64}")
                                };
                                img.set_src(&url);
                                break;
                            }
                        }
                    }
                });
            }
        });
    });

    // 插入/包裹辅助函数
    let apply_edit_op = move |op: Box<dyn FnOnce(&str, usize, usize) -> (String, usize, usize)>| {
        let Some(ed) = editor_ref.get_untracked() else {
            return;
        };
        let val = ed.value();
        let s = ed.selection_start().ok().flatten().unwrap_or(0) as usize;
        let e = ed.selection_end().ok().flatten().unwrap_or(s as u32) as usize;
        let (next_val, next_s, next_e) = op(&val, s, e);
        ed.set_value(&next_val);
        let _ = ed.focus();
        let _ = ed.set_selection_range(next_s as u32, next_e as u32);
        state.on_editor_input(next_val);
        resize_editor();
    };

    let wrap_selection = move |prefix: &'static str, suffix: &'static str, placeholder: &'static str| {
        apply_edit_op(Box::new(move |val, s, e| {
            let b_s = utf16_to_byte(val, s);
            let b_e = utf16_to_byte(val, e);
            let sel = &val[b_s..b_e];
            let inner = if sel.is_empty() { placeholder } else { sel };
            let inserted = format!("{prefix}{inner}{suffix}");
            let mut out = String::with_capacity(val.len() + inserted.len());
            out.push_str(&val[..b_s]);
            out.push_str(&inserted);
            out.push_str(&val[b_e..]);
            let p_u16 = prefix.encode_utf16().count();
            let i_u16 = inner.encode_utf16().count();
            (out, s + p_u16, s + p_u16 + i_u16)
        }));
    };

    let prefix_line = move |prefix: &'static str| {
        apply_edit_op(Box::new(move |val, s, _e| {
            let b_s = utf16_to_byte(val, s);
            let line_start = val[..b_s].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let mut out = String::with_capacity(val.len() + prefix.len());
            out.push_str(&val[..line_start]);
            out.push_str(prefix);
            out.push_str(&val[line_start..]);
            let p_u16 = prefix.encode_utf16().count();
            (out, s + p_u16, s + p_u16)
        }));
    };

    let insert_text = move |snippet: String| {
        apply_edit_op(Box::new(move |val, s, e| {
            let b_s = utf16_to_byte(val, s);
            let b_e = utf16_to_byte(val, e);
            let mut out = String::with_capacity(val.len() + snippet.len());
            out.push_str(&val[..b_s]);
            out.push_str(&snippet);
            out.push_str(&val[b_e..]);
            let len_u16 = snippet.encode_utf16().count();
            (out, s + len_u16, s + len_u16)
        }));
    };

    // 检查 [[ 自动补全
    let update_wiki_suggest = move || {
        let Some(ed) = editor_ref.get_untracked() else {
            wiki_open.set(false);
            return;
        };
        let val = ed.value();
        let cursor_u16 = ed.selection_start().ok().flatten().unwrap_or(0) as usize;
        let cursor_b = utf16_to_byte(&val, cursor_u16);
        let before = &val[..cursor_b];
        if let Some(open_b) = before.rfind("[[") {
            let after_open = &before[open_b + 2..];
            if !after_open.contains(']') && !after_open.contains('\n') {
                let q = after_open.to_lowercase();
                let notes = state.flat_notes();
                let list: Vec<(String, String)> = notes
                    .into_iter()
                    .filter(|n| {
                        q.is_empty()
                            || n.title.to_lowercase().contains(&q)
                            || n.path.to_lowercase().contains(&q)
                    })
                    .take(7)
                    .map(|n| (n.title, n.path))
                    .collect();
                if !list.is_empty() {
                    let start_u16 = val[..open_b + 2].encode_utf16().count();
                    wiki_query_start.set(start_u16);
                    wiki_candidates.set(list);
                    wiki_sel.set(0);
                    wiki_open.set(true);
                    return;
                }
            }
        }
        wiki_open.set(false);
    };

    let apply_wiki_choice = move |title: String| {
        let Some(ed) = editor_ref.get_untracked() else {
            return;
        };
        let val = ed.value();
        let start_b = utf16_to_byte(&val, wiki_query_start.get_untracked());
        let cursor_u16 = ed.selection_start().ok().flatten().unwrap_or(0) as usize;
        let cursor_b = utf16_to_byte(&val, cursor_u16);
        let mut out = String::with_capacity(val.len() + title.len() + 2);
        out.push_str(&val[..start_b]);
        out.push_str(&title);
        out.push_str("]]");
        out.push_str(&val[cursor_b..]);
        let next_pos = val[..start_b].encode_utf16().count() + title.encode_utf16().count() + 2;
        ed.set_value(&out);
        let _ = ed.set_selection_range(next_pos as u32, next_pos as u32);
        let _ = ed.focus();
        wiki_open.set(false);
        state.on_editor_input(out);
    };

    let update_cursor_stats = move || {
        let Some(ed) = editor_ref.get_untracked() else {
            return;
        };
        let val = ed.value();
        let pos_u16 = ed.selection_start().ok().flatten().unwrap_or(0) as usize;
        let pos_b = utf16_to_byte(&val, pos_u16);
        let before = &val[..pos_b];
        let line = before.matches('\n').count() + 1;
        let col = before
            .rsplit('\n')
            .next()
            .map(|s| s.chars().count() + 1)
            .unwrap_or(1);
        state.cursor_pos.set((line, col));
    };

    // 滚动监听：更新右侧大纲当前小节高亮
    let update_active_heading = move |from_preview: bool| {
        let headings = extract_headings(&state.editor_content.get_untracked());
        if headings.is_empty() {
            state.active_heading_line.set(None);
            return;
        }
        if from_preview {
            if let (Some(preview), Some(scroll)) =
                (preview_ref.get_untracked(), preview_scroll_ref.get_untracked())
            {
                let threshold = scroll.get_bounding_client_rect().top() + 90.0;
                let mut active = headings[0].line;
                for h in &headings {
                    if let Ok(Some(el)) =
                        preview.query_selector(&format!("[data-line=\"{}\"]", h.line))
                    {
                        if el.get_bounding_client_rect().top() < threshold {
                            active = h.line;
                        }
                    }
                }
                state.active_heading_line.set(Some(active));
            }
        } else if let Some(scroll) = editor_scroll_ref.get_untracked() {
            let range = (scroll.scroll_height() - scroll.client_height()).max(1) as f64;
            let total_lines = state.editor_content.get_untracked().lines().count().max(1) as f64;
            let cur_line = ((scroll.scroll_top() as f64) / range) * total_lines;
            let mut active = headings[0].line;
            for h in &headings {
                if (h.line as f64) <= cur_line + 1.0 {
                    active = h.line;
                }
            }
            state.active_heading_line.set(Some(active));
        }
    };

    // 隐藏图片文件选择器
    let on_pick_image = move |ev: web_sys::Event| {
        let Some(input) = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        let Some(files) = input.files() else { return };
        let Some(file) = files.get(0) else { return };
        let filename = file.name();
        input.set_value("");
        read_image_and_insert(state, filename, file, insert_text);
    };

    let commit_title = move || {
        let Some(path) = state.editor_path.get_untracked() else {
            return;
        };
        let next = title_draft.get_untracked().trim().to_string();
        if !next.is_empty() && next != state.editor_title.get_untracked() {
            spawn_local(async move {
                state.rename_note_to(path, next).await;
            });
        } else {
            title_draft.set(state.editor_title.get_untracked());
        }
    };

    // 笔记当前标签
    let note_tags = move || {
        let Some(path) = state.editor_path.get() else {
            return Vec::new();
        };
        state
            .link_index
            .get()
            .by_path
            .get(&path)
            .map(|n| n.tags.clone())
            .unwrap_or_default()
    };

    let current_note_info = move || {
        state
            .editor_path
            .get()
            .and_then(|p| state.note_at(&p))
    };

    let more_menu_items = move || {
        let mut items = vec![
            MenuItem::new("copy-md", "复制 Markdown 源码").icon(IconName::COPY),
            MenuItem::new("copy-html", "复制渲染 HTML").icon(ICON_CODE),
            MenuItem::new("dup", "创建副本").icon(IconName::PLUS_CIRCLE),
        ];
        if state.is_desktop {
            items.push(
                MenuItem::new("reveal", "在资源管理器中显示")
                    .icon(IconName::FOLDER_OPEN)
                    .separator_before(),
            );
        }
        items.push(
            MenuItem::new("trash", "移到回收站")
                .icon(IconName::TRASH)
                .danger()
                .separator_before(),
        );
        items
    };

    let doc_header = move || {
        let info = current_note_info();
        let is_fav = info.as_ref().map(|i| i.favorite).unwrap_or(false);
        let mtime_label = info
            .as_ref()
            .map(|i| format!("更新于 {} ({})", rel_time(i.mtime), abs_date(i.mtime)))
            .unwrap_or_default();
        let tags = note_tags();

        view! {
            <div class="ink-doc__head">
                <div class="ink-doc__row">
                    <input
                        class="ink-title selectable"
                        type="text"
                        placeholder="无标题笔记"
                        spellcheck="false"
                        aria-label="笔记名称"
                        prop:value=move || title_draft.get()
                        on:input=move |ev| title_draft.set(event_target_value(&ev))
                        on:blur=move |_| commit_title()
                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                            if ev.key() == "Enter" && !ev.is_composing() {
                                ev.prevent_default();
                                commit_title();
                                if let Some(ed) = editor_ref.get_untracked() {
                                    let _ = ed.focus();
                                }
                            } else if ev.key() == "Escape" {
                                title_draft.set(state.editor_title.get_untracked());
                            }
                        }
                    />
                    <div class="ink-doc__tools">
                        <IconButton
                            icon=IconName::SEARCH
                            label="查找与替换 (Ctrl+F)"
                            size=Size::Sm
                            on_click=Callback::new(move |_| {
                                state.find_open.set(true);
                                state.find_focus_tick.update(|n| *n += 1);
                            })
                        />
                        <IconButton
                            icon=IconName::IMAGE
                            label="插入本地图片"
                            size=Size::Sm
                            on_click=Callback::new(move |_| {
                                if let Some(el) = image_input_ref.get_untracked() {
                                    el.click();
                                }
                            })
                        />
                        <IconButton
                            icon=if is_fav { IconName::STAR } else { IconName::STAR_OFF }
                            label=if is_fav { "取消收藏" } else { "收藏笔记" }
                            variant=if is_fav { IconButtonVariant::Tonal } else { IconButtonVariant::Plain }
                            size=Size::Sm
                            on_click=Callback::new(move |_| {
                                if let Some(p) = state.editor_path.get_untracked() {
                                    spawn_local(async move {
                                        state.toggle_favorite(p).await;
                                    });
                                }
                            })
                        />
                        <MenuButton
                            icon=IconName::MORE
                            label="更多操作"
                            items=more_menu_items()
                            on_select=Callback::new(move |id: String| {
                                let Some(p) = state.editor_path.get_untracked() else { return };
                                match id.as_str() {
                                    "copy-md" => {
                                        let txt = state.editor_content.get_untracked();
                                        spawn_local(async move {
                                            let _ = copy_to_clipboard(&txt).await;
                                            toast("已复制 Markdown 源码");
                                        });
                                    }
                                    "copy-html" => {
                                        let html = rendered_html.get_untracked();
                                        spawn_local(async move {
                                            let _ = copy_to_clipboard(&html).await;
                                            toast("已复制 HTML");
                                        });
                                    }
                                    "dup" => {
                                        spawn_local(async move {
                                            state.duplicate_note(p).await;
                                        });
                                    }
                                    "reveal" => state.open_in_explorer(p),
                                    "trash" => {
                                        spawn_local(async move {
                                            state.trash_note(p).await;
                                        });
                                    }
                                    _ => {}
                                }
                            })
                        />
                    </div>
                </div>

                <div class="ink-doc__meta">
                    <span class="ink-doc__time">{mtime_label}</span>
                    {(!tags.is_empty()).then(|| view! {
                        <div class="ink-doc__tags">
                            {tags.into_iter().map(|tag| {
                                let t_click = tag.clone();
                                view! {
                                    <button
                                        type="button"
                                        class="ink-doc__tag-pill"
                                        on:click=move |_| state.filter_by_tag(t_click.clone())
                                    >
                                        {format!("#{tag}")}
                                    </button>
                                }
                            }).collect_view()}
                        </div>
                    })}
                </div>
            </div>
        }
    };

    let format_bar = move || {
        view! {
            <div class="ink-format-bar" role="toolbar" aria-label="Markdown 格式工具">
                <IconButton icon=ICON_HEADING label="二级标题" size=Size::Sm on_click=Callback::new(move |_| prefix_line("## ")) />
                <IconButton icon=ICON_BOLD label="加粗 (Ctrl+B)" size=Size::Sm on_click=Callback::new(move |_| wrap_selection("**", "**", "加粗文本")) />
                <IconButton icon=ICON_ITALIC label="斜体 (Ctrl+I)" size=Size::Sm on_click=Callback::new(move |_| wrap_selection("*", "*", "斜体文本")) />
                <IconButton icon=ICON_STRIKE label="删除线" size=Size::Sm on_click=Callback::new(move |_| wrap_selection("~~", "~~", "删除线")) />
                <IconButton icon=ICON_CODE label="行内代码" size=Size::Sm on_click=Callback::new(move |_| wrap_selection("`", "`", "code")) />
                <span class="c-vline" aria-hidden="true"></span>
                <IconButton icon=ICON_QUOTE label="引用块" size=Size::Sm on_click=Callback::new(move |_| prefix_line("> ")) />
                <IconButton icon=ICON_LIST_TODO label="任务列表" size=Size::Sm on_click=Callback::new(move |_| prefix_line("- [ ] ")) />
                <IconButton icon=ICON_LINK label="插入双链 [[]]" size=Size::Sm on_click=Callback::new(move |_| wrap_selection("[[", "]]", "笔记名称")) />
                <IconButton icon=IconName::FILE_TEXT label="代码块" size=Size::Sm on_click=Callback::new(move |_| insert_text("\n```\n\n```\n".to_string())) />
            </div>
        }
    };

    let editor_pane_view = move || {
        view! {
            <div class="ink-pane ink-pane--edit">
                // 查找替换浮层栏
                <Show when=move || state.find_open.get()>
                    <div class="ink-findbar c-glass-panel" role="search" aria-label="查找和替换">
                        <div class="ink-findbar__row">
                            <div class="ink-findbar__input-wrap">
                                <input
                                    class="ink-findbar__input selectable"
                                    type="text"
                                    placeholder="查找内容…"
                                    spellcheck="false"
                                    node_ref=find_input_ref
                                    prop:value=move || find_query.get()
                                    on:input=move |ev| find_query.set(event_target_value(&ev))
                                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                                        if ev.key() == "Enter" {
                                            ev.prevent_default();
                                            if ev.shift_key() { prev_match(); } else { next_match(); }
                                        } else if ev.key() == "Escape" {
                                            ev.prevent_default();
                                            state.find_open.set(false);
                                            if let Some(ed) = editor_ref.get_untracked() {
                                                let _ = ed.focus();
                                            }
                                        }
                                    }
                                />
                                <span class="ink-findbar__count">
                                    {move || {
                                        let m = find_matches.get();
                                        if m.is_empty() {
                                            if find_query.get().is_empty() { "0/0".to_string() } else { "无结果".to_string() }
                                        } else {
                                            format!("{}/{}", find_idx.get() + 1, m.len())
                                        }
                                    }}
                                </span>
                            </div>
                            <button
                                type="button"
                                class=move || if find_case.get() { "ink-findbar__btn is-active" } else { "ink-findbar__btn" }
                                title="区分大小写"
                                on:click=move |_| find_case.update(|c| *c = !*c)
                            >
                                "Aa"
                            </button>
                            <IconButton icon=IconName::CHEVRON_UP label="上一个 (Shift+Enter)" size=Size::Sm on_click=Callback::new(move |_| prev_match()) />
                            <IconButton icon=IconName::CHEVRON_DOWN label="下一个 (Enter)" size=Size::Sm on_click=Callback::new(move |_| next_match()) />
                            <IconButton
                                icon=IconName::SLIDERS
                                label="切换替换"
                                size=Size::Sm
                                on_click=Callback::new(move |_| state.find_replace_open.update(|r| *r = !*r))
                            />
                            <IconButton
                                icon=IconName::X
                                label="关闭 (Esc)"
                                size=Size::Sm
                                on_click=Callback::new(move |_| {
                                    state.find_open.set(false);
                                    if let Some(ed) = editor_ref.get_untracked() {
                                        let _ = ed.focus();
                                    }
                                })
                            />
                        </div>

                        <Show when=move || state.find_replace_open.get()>
                            <div class="ink-findbar__row">
                                <div class="ink-findbar__input-wrap">
                                    <input
                                        class="ink-findbar__input selectable"
                                        type="text"
                                        placeholder="替换为…"
                                        spellcheck="false"
                                        prop:value=move || replace_query.get()
                                        on:input=move |ev| replace_query.set(event_target_value(&ev))
                                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                                            if ev.key() == "Escape" {
                                                ev.prevent_default();
                                                state.find_open.set(false);
                                            }
                                        }
                                    />
                                </div>
                                <Button
                                    label="替换"
                                    size=Size::Sm
                                    on_click=Callback::new(move |_| {
                                        let matches = find_matches.get_untracked();
                                        let idx = find_idx.get_untracked();
                                        let Some(&(s_u16, e_u16)) = matches.get(idx) else { return };
                                        let rep = replace_query.get_untracked();
                                        apply_edit_op(Box::new(move |val, _, _| {
                                            let b_s = utf16_to_byte(val, s_u16);
                                            let b_e = utf16_to_byte(val, e_u16);
                                            let mut out = String::with_capacity(val.len() + rep.len());
                                            out.push_str(&val[..b_s]);
                                            out.push_str(&rep);
                                            out.push_str(&val[b_e..]);
                                            let next_pos = s_u16 + rep.encode_utf16().count();
                                            (out, next_pos, next_pos)
                                        }));
                                    })
                                />
                                <Button
                                    label="全部"
                                    size=Size::Sm
                                    variant=ButtonVariant::Tonal
                                    on_click=Callback::new(move |_| {
                                        let q = find_query.get_untracked();
                                        if q.is_empty() { return; }
                                        let matches = find_matches.get_untracked();
                                        if matches.is_empty() { return; }
                                        let rep = replace_query.get_untracked();
                                        let count = matches.len();
                                        let case_sensitive = find_case.get_untracked();
                                        let text = state.editor_content.get_untracked();
                                        let replaced = replace_all_text(&text, &q, &rep, case_sensitive);
                                        if let Some(ed) = editor_ref.get_untracked() {
                                            ed.set_value(&replaced);
                                        }
                                        state.on_editor_input(replaced);
                                        toast(format!("已替换 {count} 处"));
                                    })
                                />
                            </div>
                        </Show>
                    </div>
                </Show>

                // 外部文件冲突提示条
                <Show when=move || state.disk_conflict.get().is_some()>
                    <div class="ink-conflict-bar">
                        <InlineMessage
                            tone=Tone::Warning
                            title="磁盘上的笔记已被外部程序修改"
                            description="你有尚未保存的本地修改，请选择要保留的版本"
                        />
                        <div class="ink-conflict-bar__actions">
                            <Button
                                label="加载磁盘版本"
                                size=Size::Sm
                                variant=ButtonVariant::Primary
                                on_click=Callback::new(move |_| {
                                    if let Some(disk) = state.disk_conflict.get_untracked() {
                                        state.editor_content.set(disk.clone());
                                        state.saved_content.set(disk.clone());
                                        state.save_status.set(SaveStatus::Saved);
                                        if let Some(ed) = editor_ref.get_untracked() {
                                            ed.set_value(&disk);
                                        }
                                        state.disk_conflict.set(None);
                                        toast("已加载磁盘最新版本");
                                    }
                                })
                            />
                            <Button
                                label="保留我的修改"
                                size=Size::Sm
                                on_click=Callback::new(move |_| {
                                    state.disk_conflict.set(None);
                                    spawn_local(async move {
                                        state.commit_doc().await;
                                    });
                                })
                            />
                        </div>
                    </div>
                </Show>

                {format_bar()}

                <div
                    class="ink-editor-scroll"
                    node_ref=editor_scroll_ref
                    on:scroll=move |_| {
                        update_active_heading(false);
                        // 分栏模式下按比例同步预览区滚动
                        if state.view_mode.get_untracked() == "split" {
                            if let (Some(ed_sc), Some(pv_sc)) =
                                (editor_scroll_ref.get_untracked(), preview_scroll_ref.get_untracked())
                            {
                                let ed_range = (ed_sc.scroll_height() - ed_sc.client_height()).max(1) as f64;
                                let pv_range = (pv_sc.scroll_height() - pv_sc.client_height()).max(0) as f64;
                                let ratio = (ed_sc.scroll_top() as f64) / ed_range;
                                pv_sc.set_scroll_top((ratio * pv_range) as i32);
                            }
                        }
                    }
                    on:click=move |ev: web_sys::MouseEvent| {
                        if let (Some(scroll), Some(ed)) =
                            (editor_scroll_ref.get_untracked(), editor_ref.get_untracked())
                        {
                            let scroll_node: &web_sys::Node = scroll.as_ref();
                            if ev.target().as_ref().map(|t| scroll_node.is_same_node(Some(t.unchecked_ref()))).unwrap_or(false) {
                                let _ = ed.focus();
                                let len = ed.value().encode_utf16().count() as u32;
                                let _ = ed.set_selection_range(len, len);
                            }
                        }
                    }
                >
                    <div class="ink-doc-inner">
                        {doc_header()}
                        <textarea
                            class="ink-editor selectable"
                            spellcheck="false"
                            placeholder="开始写点什么…（支持 Markdown、[[双链]]、#标签 与粘贴图片）"
                            aria-label="Markdown 编辑器"
                            node_ref=editor_ref
                            prop:value=move || state.editor_content.get()
                            on:input=move |ev| {
                                let next = event_target_value(&ev);
                                state.on_editor_input(next);
                                resize_editor();
                                update_cursor_stats();
                                update_wiki_suggest();
                            }
                            on:click=move |_| {
                                update_cursor_stats();
                                update_wiki_suggest();
                            }
                            on:keyup=move |_| update_cursor_stats()
                            on:paste=move |ev: web_sys::ClipboardEvent| {
                                let Some(data) = ev.clipboard_data() else { return };
                                let items = data.items();
                                for i in 0..items.length() {
                                    let Some(item) = items.get(i) else { continue };
                                    if item.type_().starts_with("image/") {
                                        if let Ok(Some(file)) = item.get_as_file() {
                                            ev.prevent_default();
                                            let filename = format!("image-{}.png", now_ms());
                                            read_image_and_insert(state, filename, file, insert_text);
                                            return;
                                        }
                                    }
                                }
                            }
                            on:dragover=move |ev: web_sys::DragEvent| {
                                if let Some(dt) = ev.data_transfer() {
                                    if dt.types().length() > 0 {
                                        ev.prevent_default();
                                    }
                                }
                            }
                            on:drop=move |ev: web_sys::DragEvent| {
                                let Some(dt) = ev.data_transfer() else { return };
                                let Some(files) = dt.files() else { return };
                                if let Some(file) = files.get(0) {
                                    if file.type_().starts_with("image/") {
                                        ev.prevent_default();
                                        let filename = file.name();
                                        read_image_and_insert(state, filename, file, insert_text);
                                    }
                                }
                            }
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.is_composing() || ev.key_code() == 229 {
                                    return;
                                }
                                // [[ 双链弹窗按键拦截
                                if wiki_open.get_untracked() {
                                    let cands = wiki_candidates.get_untracked();
                                    if !cands.is_empty() {
                                        match ev.key().as_str() {
                                            "ArrowDown" => {
                                                ev.prevent_default();
                                                wiki_sel.update(|i| *i = (*i + 1) % cands.len());
                                                return;
                                            }
                                            "ArrowUp" => {
                                                ev.prevent_default();
                                                wiki_sel.update(|i| *i = if *i == 0 { cands.len() - 1 } else { *i - 1 });
                                                return;
                                            }
                                            "Enter" | "Tab" => {
                                                ev.prevent_default();
                                                if let Some((title, _)) = cands.get(wiki_sel.get_untracked()) {
                                                    apply_wiki_choice(title.clone());
                                                }
                                                return;
                                            }
                                            "Escape" => {
                                                ev.prevent_default();
                                                wiki_open.set(false);
                                                return;
                                            }
                                            _ => {}
                                        }
                                    }
                                }

                                let mod_key = ev.ctrl_key() || ev.meta_key();
                                if mod_key && ev.key().eq_ignore_ascii_case("b") {
                                    ev.prevent_default();
                                    wrap_selection("**", "**", "加粗文本");
                                    return;
                                }
                                if mod_key && ev.key().eq_ignore_ascii_case("i") {
                                    ev.prevent_default();
                                    wrap_selection("*", "*", "斜体文本");
                                    return;
                                }
                                if ev.key() == "Tab" {
                                    ev.prevent_default();
                                    if ev.shift_key() {
                                        apply_edit_op(Box::new(|val, s, _e| {
                                            let b_s = utf16_to_byte(val, s);
                                            let line_start = val[..b_s].rfind('\n').map(|i| i + 1).unwrap_or(0);
                                            if val[line_start..].starts_with("  ") {
                                                let mut out = String::with_capacity(val.len());
                                                out.push_str(&val[..line_start]);
                                                out.push_str(&val[line_start + 2..]);
                                                let ns = s.saturating_sub(2);
                                                (out, ns, ns)
                                            } else {
                                                (val.to_string(), s, s)
                                            }
                                        }));
                                    } else {
                                        insert_text("  ".to_string());
                                    }
                                    return;
                                }
                                if ev.key() == "Enter" && state.auto_continue_lists.get_untracked() {
                                    if let Some(ed) = editor_ref.get_untracked() {
                                        let val = ed.value();
                                        let s = ed.selection_start().ok().flatten().unwrap_or(0) as usize;
                                        let e = ed.selection_end().ok().flatten().unwrap_or(s as u32) as usize;
                                        if s == e {
                                            if let Some((next_val, next_pos)) = try_continue_list(&val, s) {
                                                ev.prevent_default();
                                                ed.set_value(&next_val);
                                                let _ = ed.set_selection_range(next_pos as u32, next_pos as u32);
                                                state.on_editor_input(next_val);
                                                resize_editor();
                                            }
                                        }
                                    }
                                }
                            }
                        ></textarea>
                    </div>
                </div>

                // [[ 双链补全浮层
                <Show when=move || wiki_open.get()>
                    <div class="ink-wiki-popup c-menu" role="listbox" aria-label="双链笔记补全">
                        {move || {
                            let cands = wiki_candidates.get();
                            let sel = wiki_sel.get();
                            cands.into_iter().enumerate().map(|(i, (title, path))| {
                                let t_apply = title.clone();
                                view! {
                                    <div
                                        class=if i == sel { "c-menu__item is-selected" } else { "c-menu__item" }
                                        role="option"
                                        aria-selected=(i == sel).to_string()
                                        on:mousedown=move |ev: web_sys::MouseEvent| {
                                            ev.prevent_default();
                                            apply_wiki_choice(t_apply.clone());
                                        }
                                    >
                                        <Icon name=IconName::FILE_TEXT size=13 />
                                        <span class="c-menu__label">{title}</span>
                                        <span class="c-menu__key">{dirname(&path)}</span>
                                    </div>
                                }
                            }).collect_view()
                        }}
                    </div>
                </Show>
            </div>
        }
    };

    let preview_pane_view = move || {
        view! {
            <div class="ink-pane ink-pane--preview">
                <div
                    class="ink-preview-scroll"
                    node_ref=preview_scroll_ref
                    on:scroll=move |_| update_active_heading(true)
                >
                    <div class="ink-doc-inner">
                        {move || (state.view_mode.get() == "preview").then(|| doc_header())}
                        <article
                            class="md-body selectable"
                            node_ref=preview_ref
                            inner_html=move || rendered_html.get()
                            on:click=move |ev: web_sys::MouseEvent| {
                                let Some(target) = ev
                                    .target()
                                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                                else {
                                    return;
                                };

                                // 1. 代码块复制按钮
                                if let Ok(Some(copy_btn)) = target.closest("button[data-copy-code]") {
                                    if let Ok(Some(box_el)) = copy_btn.closest(".md-code") {
                                        if let Ok(Some(code_el)) = box_el.query_selector("pre code") {
                                            let code_text = code_el.text_content().unwrap_or_default();
                                            let btn_clone = copy_btn.clone();
                                            spawn_local(async move {
                                                let _ = copy_to_clipboard(&code_text).await;
                                                btn_clone.set_text_content(Some("已复制"));
                                                toast("代码已复制到剪贴板");
                                            });
                                        }
                                    }
                                    return;
                                }

                                // 2. 交互式任务列表复选框
                                if let Ok(Some(cb_el)) = target.closest("input[type=\"checkbox\"][data-task-line]") {
                                    if let Some(line_str) = cb_el.get_attribute("data-task-line") {
                                        if let Ok(line_idx) = line_str.parse::<usize>() {
                                            let checked = cb_el
                                                .dyn_ref::<web_sys::HtmlInputElement>()
                                                .map(|i| i.checked())
                                                .unwrap_or(false);
                                            let cur = state.editor_content.get_untracked();
                                            if let Some(updated) = toggle_task_at_line(&cur, line_idx, checked) {
                                                if let Some(ed) = editor_ref.get_untracked() {
                                                    ed.set_value(&updated);
                                                }
                                                state.on_editor_input(updated);
                                            }
                                        }
                                    }
                                    return;
                                }

                                // 3. Obsidian [[双链]]
                                if let Ok(Some(wiki)) = target.closest("a.ink-wikilink") {
                                    ev.prevent_default();
                                    if let Some(tgt) = wiki.get_attribute("data-target") {
                                        spawn_local(async move {
                                            state.open_wikilink(tgt).await;
                                        });
                                    }
                                    return;
                                }

                                // 4. 外部 http/https 链接
                                if let Ok(Some(link)) = target.closest("a[href]") {
                                    let href = link.get_attribute("href").unwrap_or_default();
                                    if href.starts_with("http://") || href.starts_with("https://") {
                                        ev.prevent_default();
                                        spawn_local(async move {
                                            open_external_url(&href).await;
                                        });
                                    }
                                }
                            }
                        ></article>
                    </div>
                </div>
            </div>
        }
    };

    view! {
        <div class="ink-note-view">
            <input
                type="file"
                accept="image/*"
                style="display:none"
                node_ref=image_input_ref
                on:change=on_pick_image
            />
            {move || {
                match state.view_mode.get().as_str() {
                    "edit" => editor_pane_view().into_any(),
                    "preview" => preview_pane_view().into_any(),
                    _ => view! {
                        <SplitView
                            orientation=SplitOrientation::Horizontal
                            initial=0.5
                            min=0.25
                            max=0.75
                            first=ToChildren::to_children(editor_pane_view)
                            second=ToChildren::to_children(preview_pane_view)
                        />
                    }.into_any(),
                }
            }}
        </div>
    }
}

fn read_image_and_insert(
    state: AppState,
    filename: String,
    file: web_sys::File,
    insert_fn: impl Fn(String) + 'static,
) {
    let Ok(reader) = web_sys::FileReader::new() else {
        return;
    };
    let reader_clone = reader.clone();
    let cb = wasm_bindgen::closure::Closure::once_into_js(move || {
        let Ok(res) = reader_clone.result() else {
            return;
        };
        let Some(data_url) = res.as_string() else {
            return;
        };
        spawn_local(async move {
            match state.save_attachment(filename, data_url).await {
                Ok(rel) => {
                    insert_fn(format!("\n![{}]({})\n", basename(&rel), rel));
                    toast(format!("已插入图片：{}", basename(&rel)));
                }
                Err(err) => {
                    toast(format!("保存图片失败：{err}"));
                }
            }
        });
    });
    reader.set_onload(Some(cb.unchecked_ref()));
    let _ = reader.read_as_data_url(&file);
}

fn toggle_task_at_line(src: &str, line_idx: usize, checked: bool) -> Option<String> {
    let normalized = src.replace("\r\n", "\n");
    let mut lines: Vec<String> = normalized.split('\n').map(str::to_string).collect();
    let line = lines.get_mut(line_idx)?;
    if checked {
        if let Some(pos) = line.find("[ ] ") {
            line.replace_range(pos..pos + 3, "[x]");
        } else {
            return None;
        }
    } else if let Some(pos) = line.find("[x] ").or_else(|| line.find("[X] ")) {
        line.replace_range(pos..pos + 3, "[ ]");
    } else {
        return None;
    }
    Some(lines.join("\n"))
}

fn try_continue_list(val: &str, cursor_u16: usize) -> Option<(String, usize)> {
    let b_pos = utf16_to_byte(val, cursor_u16);
    let line_start = val[..b_pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_prefix = &val[line_start..b_pos];

    let indent_len = line_prefix.chars().take_while(|&c| c == ' ' || c == '\t').count();
    let indent: String = line_prefix.chars().take(indent_len).collect();
    let rest = &line_prefix[indent.len()..];

    let next_prefix = if let Some(after) = rest
        .strip_prefix("- [ ] ")
        .or_else(|| rest.strip_prefix("- [x] "))
        .or_else(|| rest.strip_prefix("- [X] "))
    {
        if after.trim().is_empty() && val[b_pos..].starts_with('\n').then_some(true).unwrap_or(val[b_pos..].is_empty()) {
            // 空任务项按回车：退出列表
            return Some((
                format!("{}{}", &val[..line_start], &val[b_pos..]),
                val[..line_start].encode_utf16().count(),
            ));
        }
        format!("{indent}- [ ] ")
    } else if let Some(after) = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))
    {
        let marker = &rest[..2];
        if after.trim().is_empty() && (val[b_pos..].is_empty() || val[b_pos..].starts_with('\n')) {
            return Some((
                format!("{}{}", &val[..line_start], &val[b_pos..]),
                val[..line_start].encode_utf16().count(),
            ));
        }
        format!("{indent}{marker}")
    } else if let Some(after) = rest.strip_prefix("> ") {
        if after.trim().is_empty() && (val[b_pos..].is_empty() || val[b_pos..].starts_with('\n')) {
            return Some((
                format!("{}{}", &val[..line_start], &val[b_pos..]),
                val[..line_start].encode_utf16().count(),
            ));
        }
        format!("{indent}> ")
    } else {
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            let after_num = &rest[digits.len()..];
            if let Some(after) = after_num.strip_prefix(". ") {
                if after.trim().is_empty() && (val[b_pos..].is_empty() || val[b_pos..].starts_with('\n')) {
                    return Some((
                        format!("{}{}", &val[..line_start], &val[b_pos..]),
                        val[..line_start].encode_utf16().count(),
                    ));
                }
                let n: usize = digits.parse().unwrap_or(1);
                format!("{indent}{}. ", n + 1)
            } else {
                return None;
            }
        } else {
            return None;
        }
    };

    let insert = format!("\n{next_prefix}");
    let mut out = String::with_capacity(val.len() + insert.len());
    out.push_str(&val[..b_pos]);
    out.push_str(&insert);
    out.push_str(&val[b_pos..]);
    let next_u16 = cursor_u16 + insert.encode_utf16().count();
    Some((out, next_u16))
}

fn utf16_to_byte(s: &str, u16_offset: usize) -> usize {
    let mut cur_u16 = 0usize;
    for (byte_idx, ch) in s.char_indices() {
        if cur_u16 >= u16_offset {
            return byte_idx;
        }
        cur_u16 += ch.len_utf16();
    }
    s.len()
}

fn utf16_offset_to_line(s: &str, u16_offset: usize) -> usize {
    let b = utf16_to_byte(s, u16_offset);
    s[..b].matches('\n').count()
}

fn line_start_utf16(s: &str, target_line: usize) -> Option<usize> {
    if target_line == 0 {
        return Some(0);
    }
    let mut line = 0usize;
    let mut u16_pos = 0usize;
    for ch in s.chars() {
        u16_pos += ch.len_utf16();
        if ch == '\n' {
            line += 1;
            if line == target_line {
                return Some(u16_pos);
            }
        }
    }
    None
}

fn find_utf16_matches(text: &str, query: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if query.is_empty() {
        return out;
    }
    let q_chars: Vec<char> = if case_sensitive {
        query.chars().collect()
    } else {
        query.to_lowercase().chars().collect()
    };
    let t_chars: Vec<char> = if case_sensitive {
        text.chars().collect()
    } else {
        text.to_lowercase().chars().collect()
    };
    if q_chars.is_empty() || t_chars.len() < q_chars.len() {
        return out;
    }
    // 预计算每个字符的 UTF-16 偏移
    let mut u16_prefix = Vec::with_capacity(t_chars.len() + 1);
    let mut acc = 0usize;
    for ch in text.chars() {
        u16_prefix.push(acc);
        acc += ch.len_utf16();
    }
    u16_prefix.push(acc);

    let qlen = q_chars.len();
    let mut i = 0;
    while i + qlen <= t_chars.len() {
        if t_chars[i..i + qlen] == q_chars[..] {
            out.push((u16_prefix[i], u16_prefix[i + qlen]));
            i += qlen;
        } else {
            i += 1;
        }
    }
    out
}

fn replace_all_text(text: &str, query: &str, rep: &str, case_sensitive: bool) -> String {
    if query.is_empty() {
        return text.to_string();
    }
    if case_sensitive {
        return text.replace(query, rep);
    }
    let q_lc: Vec<char> = query.to_lowercase().chars().collect();
    let orig: Vec<char> = text.chars().collect();
    let lc: Vec<char> = text.to_lowercase().chars().collect();
    let qlen = q_lc.len();
    if qlen == 0 || orig.len() < qlen || orig.len() != lc.len() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < orig.len() {
        if i + qlen <= lc.len() && lc[i..i + qlen] == q_lc[..] {
            out.push_str(rep);
            i += qlen;
        } else {
            out.push(orig[i]);
            i += 1;
        }
    }
    out
}
