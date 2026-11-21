//! 非 Markdown 文件视图：代码、图片/音视频、CSV 表格（GlassDesign Table）、PDF、Office 文档。

use glass_design::prelude::*;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::markdown::highlight::highlight_code;
use crate::state::AppState;
use crate::tauri_bridge::copy_to_clipboard;
use crate::vault::fs::{basename, get_file_ext, FileType};

#[component]
pub fn NonMarkdownView(path: String, file_type: FileType) -> impl IntoView {
    let state = AppState::get();
    let name = basename(&path);
    let ext = get_file_ext(&path).to_uppercase();
    let path_sv = StoredValue::new(path);
    let name_sv = StoredValue::new(name.clone());
    let ext_sv = StoredValue::new(ext.clone());

    let text_data = RwSignal::new(None::<String>);
    let binary_url = RwSignal::new(None::<String>);
    let err_msg = RwSignal::new(None::<String>);
    let img_full = RwSignal::new(false);

    // 加载文件内容
    {
        let p = path_sv.get_value();
        let ext_lc = ext_sv.get_value().to_ascii_lowercase();
        spawn_local(async move {
            let fs_opt = state
                .vault
                .with_value(|c| c.borrow().as_ref().map(|v| v.fs.clone()));
            let Some(fs) = fs_opt else {
                err_msg.set(Some("仓库未就绪".into()));
                return;
            };
            match file_type {
                FileType::Code | FileType::Other => match fs.read(&p).await {
                    Ok(Some(t)) => text_data.set(Some(t)),
                    Ok(None) => err_msg.set(Some("文件不存在或为空".into())),
                    Err(e) => err_msg.set(Some(e)),
                },
                FileType::Excel if ext_lc == "csv" => match fs.read(&p).await {
                    Ok(Some(t)) => text_data.set(Some(t)),
                    Ok(None) => err_msg.set(Some("CSV 文件为空".into())),
                    Err(e) => err_msg.set(Some(e)),
                },
                FileType::Image | FileType::Video | FileType::Audio | FileType::Pdf => {
                    match fs.read_binary(&p).await {
                        Ok(Some(b64)) => {
                            let mime = match ext_lc.as_str() {
                                "pdf" => "application/pdf",
                                "svg" => "image/svg+xml",
                                "jpg" | "jpeg" => "image/jpeg",
                                "gif" => "image/gif",
                                "webp" => "image/webp",
                                "png" => "image/png",
                                "mp4" => "video/mp4",
                                "webm" => "video/webm",
                                "mov" => "video/quicktime",
                                "mp3" => "audio/mpeg",
                                "wav" => "audio/wav",
                                "ogg" => "audio/ogg",
                                "flac" => "audio/flac",
                                _ => "application/octet-stream",
                            };
                            let url = if b64.starts_with("data:") {
                                b64
                            } else {
                                format!("data:{mime};base64,{b64}")
                            };
                            binary_url.set(Some(url));
                        }
                        Ok(None) => err_msg.set(Some("无法读取文件内容".into())),
                        Err(e) => err_msg.set(Some(e)),
                    }
                }
                _ => {}
            }
        });
    }

    view! {
        <div class="ink-file-view">
            <div class="ink-view-toolbar">
                <div class="ink-view-toolbar__title">
                    <span class="c-title">{name_sv.get_value()}</span>
                    {(!ext_sv.get_value().is_empty()).then(|| view! { <Badge label=ext_sv.get_value() /> })}
                    {move || {
                        if matches!(file_type, FileType::Code | FileType::Other) {
                            text_data.get().map(|t| {
                                let lines = t.lines().count();
                                let chars = t.chars().count();
                                view! { <span class="c-subtitle">{format!("{lines} 行 · {chars} 字符")}</span> }
                            })
                        } else {
                            None
                        }
                    }}
                </div>
                <div class="ink-view-toolbar__actions">
                    <Show when=move || matches!(file_type, FileType::Image)>
                        {move || {
                            let full = img_full.get();
                            view! {
                                <Button
                                    label=if full { "适应窗口" } else { "原始大小 (100%)" }
                                    size=Size::Sm
                                    variant=ButtonVariant::Plain
                                    icon=if full { IconName::ZOOM_OUT } else { IconName::ZOOM_IN }
                                    on_click=Callback::new(move |_| img_full.update(|v| *v = !*v))
                                />
                            }
                        }}
                    </Show>
                    <Show when=move || matches!(file_type, FileType::Code | FileType::Other) && text_data.get().is_some()>
                        <Button
                            label="复制内容"
                            size=Size::Sm
                            variant=ButtonVariant::Plain
                            icon=IconName::COPY
                            on_click=Callback::new(move |_| {
                                if let Some(t) = text_data.get_untracked() {
                                    spawn_local(async move {
                                        let _ = copy_to_clipboard(&t).await;
                                        toast("已复制到剪贴板");
                                    });
                                }
                            })
                        />
                    </Show>
                    <Show when=move || state.is_desktop>
                        <Button
                            label="在系统中打开"
                            size=Size::Sm
                            variant=ButtonVariant::Plain
                            icon=IconName::EXTERNAL_LINK
                            on_click=Callback::new(move |_| {
                                let p = path_sv.get_value();
                                let cell = state.vault.get_value();
                                let fs_opt = cell.borrow().as_ref().map(|v| v.fs.clone());
                                if let Some(fs) = fs_opt {
                                    spawn_local(async move {
                                        let _ = fs.open_path(&p).await;
                                    });
                                }
                            })
                        />
                    </Show>
                </div>
            </div>

            <div class="ink-file-view__stage">
                {move || {
                    if let Some(err) = err_msg.get() {
                        return view! {
                            <Center>
                                <Empty icon=IconName::TRIANGLE_ALERT title="加载失败" desc=err />
                            </Center>
                        }.into_any();
                    }

                    match file_type {
                        FileType::Code | FileType::Other => {
                            let Some(code) = text_data.get() else {
                                return view! { <Center><Spinner /></Center> }.into_any();
                            };
                            let lines: Vec<usize> = (1..=code.lines().count().max(1)).collect();
                            let highlighted = highlight_code(&code, &ext_sv.get_value());
                            view! {
                                <div class="ink-code-view__body selectable">
                                    <div class="ink-code-view__lines" aria-hidden="true">
                                        {lines.into_iter().map(|n| view! { <span>{n}</span> }).collect_view()}
                                    </div>
                                    <pre class="ink-code-view__pre"><code inner_html=highlighted></code></pre>
                                </div>
                            }.into_any()
                        }
                        FileType::Excel if ext_sv.get_value() == "CSV" => {
                            let Some(csv) = text_data.get() else {
                                return view! { <Center><Spinner /></Center> }.into_any();
                            };
                            render_csv_table(&csv)
                        }
                        FileType::Image => {
                            let Some(url) = binary_url.get() else {
                                return view! { <Center><Spinner /></Center> }.into_any();
                            };
                            view! {
                                <div class="ink-media-view__img-wrap">
                                    <img
                                        class=if img_full.get() { "ink-media-view__img is-full" } else { "ink-media-view__img" }
                                        src=url
                                        alt=name_sv.get_value()
                                    />
                                </div>
                            }.into_any()
                        }
                        FileType::Video => {
                            let Some(url) = binary_url.get() else {
                                return view! { <Center><Spinner /></Center> }.into_any();
                            };
                            view! {
                                <div class="ink-media-view__wrap">
                                    <video class="ink-media-view__video" src=url controls=true />
                                </div>
                            }.into_any()
                        }
                        FileType::Audio => {
                            let Some(url) = binary_url.get() else {
                                return view! { <Center><Spinner /></Center> }.into_any();
                            };
                            view! {
                                <Center>
                                    <Card>
                                        <Stack>
                                            <SectionTitle>{name_sv.get_value()}</SectionTitle>
                                            <audio class="ink-media-view__audio" src=url controls=true />
                                        </Stack>
                                    </Card>
                                </Center>
                            }.into_any()
                        }
                        FileType::Pdf => {
                            let Some(url) = binary_url.get() else {
                                return view! { <Center><Spinner /></Center> }.into_any();
                            };
                            view! {
                                <iframe class="ink-pdf-view__frame" src=url title=name_sv.get_value()></iframe>
                            }.into_any()
                        }
                        _ => {
                            let label = match file_type {
                                FileType::Word => "Word 文档",
                                FileType::Excel => "Excel 电子表格",
                                FileType::Ppt => "PowerPoint 演示文稿",
                                _ => "本地文件",
                            };
                            view! {
                                <Center>
                                    <div style="width: 420px; max-width: 90%;">
                                        <Card>
                                            <Stack>
                                                <Cluster between=true>
                                                    <span class="c-title">{name_sv.get_value()}</span>
                                                    <Badge label=ext_sv.get_value() variant=BadgeVariant::Accent />
                                                </Cluster>
                                                <span class="c-subtitle">{format!("{label} · 本地文件")}</span>
                                                <Divider />
                                                <Cluster>
                                                    <Button
                                                        label="在默认软件中打开"
                                                        variant=ButtonVariant::Primary
                                                        icon=IconName::EXTERNAL_LINK
                                                        on_click=Callback::new(move |_| {
                                                            let p = path_sv.get_value();
                                                            let cell = state.vault.get_value();
                                                            let fs_opt = cell.borrow().as_ref().map(|v| v.fs.clone());
                                                            if let Some(fs) = fs_opt {
                                                                spawn_local(async move {
                                                                    if let Err(err) = fs.open_path(&p).await {
                                                                        toast(err);
                                                                    }
                                                                });
                                                            }
                                                        })
                                                    />
                                                    <Show when=move || state.is_desktop>
                                                        <Button
                                                            label="在资源管理器中定位"
                                                            variant=ButtonVariant::Secondary
                                                            icon=IconName::FOLDER_OPEN
                                                            on_click=Callback::new(move |_| state.open_in_explorer(path_sv.get_value()))
                                                        />
                                                    </Show>
                                                </Cluster>
                                            </Stack>
                                        </Card>
                                    </div>
                                </Center>
                            }.into_any()
                        }
                    }
                }}
            </div>
        </div>
    }
}

fn render_csv_table(text: &str) -> AnyView {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return view! {
            <Center><Empty icon=IconName::FILE_TEXT title="CSV 文件为空" /></Center>
        }
        .into_any();
    }

    let parse_row = |line: &str| -> Vec<String> {
        line.split(',')
            .map(|c| c.trim().trim_matches('"').trim_matches('\'').to_string())
            .collect()
    };

    let headers = parse_row(lines[0]);
    let cols: Vec<TableColumn> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let title = if h.is_empty() {
                format!("列 {}", i + 1)
            } else {
                h.clone()
            };
            TableColumn::new(format!("c{i}"), title).sortable()
        })
        .collect();

    let rows: Vec<TableRow> = lines
        .iter()
        .skip(1)
        .enumerate()
        .map(|(r_idx, line)| {
            let parsed = parse_row(line);
            let cells: Vec<String> = (0..cols.len())
                .map(|i| parsed.get(i).cloned().unwrap_or_default())
                .collect();
            TableRow::new(format!("r{r_idx}"), cells)
        })
        .collect();

    let row_count = rows.len();
    let col_count = cols.len();

    view! {
        <div class="ink-csv-wrap">
            <div class="ink-csv-meta c-subtitle">
                {format!("共 {row_count} 行 · {col_count} 列")}
            </div>
            <Table columns=cols rows=rows dense=true />
        </div>
    }
    .into_any()
}
