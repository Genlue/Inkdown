//! 偏好设置页面（完全使用 GlassDesign Page / ScrollArea / Pad / Stack / Card / Row 拼装）。

use glass_design::prelude::*;
use leptos::prelude::*;

use crate::state::{AppState, NavPage};
use crate::update::{CheckState, ReleaseInfo};
use wasm_bindgen_futures::spawn_local;

const ACCENT_PRESETS: &[(&str, &str)] = &[
    ("#0a84ff", "海蓝"),
    ("#5e5ce6", "靛蓝"),
    ("#bf5af2", "紫罗兰"),
    ("#ff375f", "品红"),
    ("#ff9f0a", "琥珀"),
    ("#30d158", "翠绿"),
    ("#64d2ff", "天青"),
    ("#8e8e93", "石墨"),
];

const FALLBACK_FONTS: &[&str] = &[
    "Microsoft YaHei",
    "PingFang SC",
    "Segoe UI",
    "Segoe UI Variable Text",
    "Inter",
    "Cascadia Code",
    "Cascadia Mono",
    "Consolas",
    "JetBrains Mono",
    "Fira Code",
    "Georgia",
    "SimSun",
];

#[component]
pub fn SettingsPage() -> impl IntoView {
    let state = AppState::get();
    let theme = use_context::<Theme>().expect("Theme must be provided");

    // 玻璃效果档位由库写在 <html data-gd-effects> 上，这里只是镜像成 Select 的值
    let effects_value = RwSignal::new(effects().as_str().to_string());
    let effects_options = vec![
        SelectOption::new("auto", "完整效果"),
        SelectOption::new("blur", "仅模糊"),
        SelectOption::new("solid", "性能优先"),
    ];

    let mode_options = vec![
        SegmentedOption::new("light", "浅色").icon(IconName::SUN),
        SegmentedOption::new("dark", "深色").icon(IconName::MOON),
        SegmentedOption::new("system", "跟随系统").icon(IconName::MONITOR),
    ];
    let mode_val = Signal::derive(move || theme.mode.get().as_str().to_string());

    let indent_options = vec![
        SegmentedOption::new("lines", "连续引导线"),
        SegmentedOption::new("boxes", "简约框"),
        SegmentedOption::new("none", "无"),
    ];

    let folder_color_options = vec![
        SegmentedOption::new("auto", "按层级上色"),
        SegmentedOption::new("off", "关闭"),
    ];

    let folder_style_options = vec![
        SegmentedOption::new("icon", "仅图标"),
        SegmentedOption::new("text", "图标与名称"),
        SegmentedOption::new("rail", "左侧色条"),
    ];

    let md_color_options = vec![
        SegmentedOption::new("off", "关闭"),
        SegmentedOption::new("soft", "克制"),
        SegmentedOption::new("vivid", "鲜明"),
    ];

    let width_options = vec![
        SegmentedOption::new("standard", "标准 (720px)"),
        SegmentedOption::new("wide", "宽屏 (920px)"),
        SegmentedOption::new("full", "铺满"),
    ];

    let view_options = vec![
        SegmentedOption::new("edit", "纯编辑"),
        SegmentedOption::new("split", "双栏分屏"),
        SegmentedOption::new("preview", "仅预览"),
    ];

    let swatches: Vec<SwatchItem> = ACCENT_PRESETS
        .iter()
        .map(|(hex, label)| SwatchItem::new(*hex, *label))
        .collect();

    let font_options = move || {
        let sys = state.system_fonts.get();
        let mut names: Vec<String> = if sys.is_empty() {
            FALLBACK_FONTS.iter().map(|s| (*s).to_string()).collect()
        } else {
            let mut merged = sys;
            for f in FALLBACK_FONTS {
                if !merged.iter().any(|x| x == f) {
                    merged.push((*f).to_string());
                }
            }
            merged.sort();
            merged
        };
        names.dedup();
        let mut opts = vec![SelectOption::new("", "系统默认 (System Default)")];
        for n in names {
            opts.push(SelectOption::new(n.clone(), n));
        }
        opts
    };

    view! {
        <Page>
            <ScrollArea>
                <PageInset>
                    <Pad wide=true>
                        <div class="ink-settings-container">
                            <Stack>
                                <Cluster between=true>
                                    <div class="l-stack l-stack--2">
                                        <h1 class="c-headline">"偏好设置"</h1>
                                        <span class="c-subtitle">"配置知识库、液态玻璃外观、字体排版与编辑器行为"</span>
                                    </div>
                                    <Button
                                        label="返回笔记"
                                        icon=IconName::ARROW_LEFT
                                        variant=ButtonVariant::Tonal
                                        on_click=Callback::new(move |_| state.nav_page.set(NavPage::Files))
                                    />
                                </Cluster>

                                // 1. 仓库与存储
                                <SectionTitle>"仓库与存储"</SectionTitle>
                                <Card>
                                    <Row label="当前知识库" hint="一个文件夹就是一个知识库，纯文本 Markdown 存储">
                                        <Badge label=state.vault_name() variant=BadgeVariant::Accent />
                                    </Row>
                                    <Divider />
                                    <Row
                                        label="存储位置"
                                        hint=if state.is_desktop {
                                            state.vault_root_path().unwrap_or_default()
                                        } else {
                                            "浏览器本地存储 (localStorage 演示沙盒)".to_string()
                                        }
                                    >
                                        <Cluster>
                                            <Show when=move || state.is_desktop>
                                                <Button
                                                    label="在资源管理器中打开"
                                                    icon=IconName::FOLDER_OPEN
                                                    size=Size::Sm
                                                    variant=ButtonVariant::Primary
                                                    on_click=Callback::new(move |_| state.open_in_explorer(String::new()))
                                                />
                                                <Button
                                                    label="复制路径"
                                                    icon=IconName::COPY
                                                    size=Size::Sm
                                                    on_click=Callback::new(move |_| state.copy_vault_path(String::new()))
                                                />
                                            </Show>
                                            <Button
                                                label=if state.is_desktop { "切换文件夹…" } else { "选择本地文件夹…" }
                                                icon=IconName::FOLDER_SEARCH
                                                size=Size::Sm
                                                on_click=Callback::new(move |_| state.pick_folder())
                                            />
                                        </Cluster>
                                    </Row>
                                </Card>

                                // 2. 外观与液态玻璃
                                <SectionTitle>"外观与液态玻璃"</SectionTitle>
                                <Card>
                                    <Row label="主题模式" hint="支持浅色、深色或自动跟随系统">
                                        <Segmented
                                            options=mode_options
                                            value=mode_val
                                            on_change=Callback::new(move |m: String| state.set_theme_mode(theme, &m))
                                        />
                                    </Row>
                                    <Divider />
                                    <Row label="强调色" hint="用于选中态、光标焦点环与交互高亮">
                                        <Swatches
                                            items=swatches
                                            selected=Signal::derive(move || state.accent_hex.get())
                                            on_select=Callback::new(move |hex: String| state.set_accent(theme, &hex))
                                        />
                                    </Row>
                                    <Divider />
                                    <Row
                                        label="玻璃效果"
                                        hint="低配设备可关掉折射与模糊；选择即时生效并记住，刷新后由库恢复"
                                    >
                                        // 不用库里的 GlassEffectsSetting：那是原生 <select>，
                                        // 下拉由 WebView2 自己画，深色主题下会变成白底 + 白字。
                                        // 这里换成本库的 Select（自绘浮层，跟随主题令牌）。
                                        {move || view! {
                                            <Select
                                                options=effects_options.clone()
                                                value=Signal::derive(move || effects_value.get())
                                                width=150
                                                label="玻璃效果"
                                                on_change=Callback::new(move |v: String| {
                                                    if let Some(mode) = GlassEffects::from_value(&v) {
                                                        set_effects(mode);
                                                        save_effects_preference(mode);
                                                        effects_value.set(v);
                                                    }
                                                })
                                            />
                                        }}
                                    </Row>
                                    <Divider />
                                    <Row label="玻璃浓淡 (--lg)" hint="0 = 通透折射，1 = 高对比饱和磨砂" wide=true>
                                        <Slider
                                            value=Signal::derive(move || state.glass_level.get() as f64)
                                            min=0.0
                                            max=1.0
                                            step=0.05
                                            label="玻璃浓淡"
                                            on_input=Callback::new(move |v: f64| theme.glass.set(v as f32))
                                            on_change=Callback::new(move |v: f64| state.set_glass_level(theme, v as f32))
                                        />
                                    </Row>
                                    <Divider />
                                    <Row label="文件树层级样式" hint="左侧目录树缩进引导方式">
                                        <Segmented
                                            options=indent_options
                                            value=Signal::derive(move || state.tree_indent.get())
                                            on_change=Callback::new(move |s: String| state.set_tree_indent(s))
                                        />
                                    </Row>
                                    <Divider />
                                    <Row label="彩虹文件夹" hint="同层不撞色，按层级轮转色板；右键目录可单独指定颜色">
                                        <Segmented
                                            options=folder_color_options
                                            value=Signal::derive(move || state.folder_color_mode.get())
                                            on_change=Callback::new(move |s: String| state.set_folder_color_mode(s))
                                        />
                                    </Row>
                                    <Divider />
                                    <Row label="着色范围" hint="只染图标最克制；染名称会让整棵树更抢眼，也更吵">
                                        <Segmented
                                            options=folder_style_options
                                            value=Signal::derive(move || state.folder_color_style.get())
                                            on_change=Callback::new(move |s: String| state.set_folder_color_style(s))
                                        />
                                    </Row>
                                    <Divider />
                                    // 这三行各说一件事：第一行是沉浸本身，后两行是它进入时的行为。
                                    // 以前「沉浸文件夹」这一行右边挂的是「顺带折叠其它目录」的开关，
                                    // 标签说的是功能、控件改的是子选项，谁也看不出这一行到底改了什么。
                                    <Row
                                        label="沉浸文件夹"
                                        hint="只让当前笔记所在目录可读，其余目录名盖成色条，专注当前这一片；纯视图状态，不改磁盘。文件树顶栏也有同一开关"
                                    >
                                        <Switch
                                            checked=Signal::derive(move || state.immersive.get())
                                            label="沉浸文件夹"
                                            on_change=Callback::new(move |on: bool| {
                                                if on {
                                                    state.enter_immersive(None);
                                                } else {
                                                    state.exit_immersive();
                                                }
                                            })
                                        />
                                    </Row>
                                    <Divider />
                                    <Row
                                        label="进入时收起其它目录"
                                        hint="进入沉浸时顺手把无关目录折叠掉；关掉则保持原来的展开状态，只把目录名盖住"
                                    >
                                        <Switch
                                            checked=Signal::derive(move || state.immersive_collapse.get())
                                            label="进入沉浸时收起其它目录"
                                            on_change=Callback::new(move |v: bool| state.set_immersive_collapse(v))
                                        />
                                    </Row>
                                    <Divider />
                                    <Row label="保留上级路径" hint="当前笔记的上级目录名在沉浸时仍可读，方便确认自己在哪一层">
                                        <Switch
                                            checked=Signal::derive(move || state.immersive_keep_ancestors.get())
                                            label="保留上级路径"
                                            on_change=Callback::new(move |v: bool| state.set_immersive_keep_ancestors(v))
                                        />
                                    </Row>
                                </Card>

                                // 3. 字体与排版
                                <SectionTitle>"字体与排版"</SectionTitle>
                                <Card>
                                    <Row label="Markdown 配色" hint="标题按强调色的浓度阶梯区分层级；链接用强调色、高亮用琥珀、已完成任务用绿色——颜色只编码这三类语义">
                                        <Segmented
                                            options=md_color_options
                                            value=Signal::derive(move || state.md_color.get())
                                            on_change=Callback::new(move |m: String| state.set_md_color(m))
                                        />
                                    </Row>
                                    <Divider />
                                    <Row label="界面字体" hint="应用于顶栏、侧边栏、标签页与控件">
                                        {move || view! {
                                            <Select
                                                options=font_options()
                                                value=Signal::derive(move || state.ui_font.get())
                                                width=230
                                                label="界面字体"
                                                on_change=Callback::new(move |f: String| state.set_font(theme, "ui", f))
                                            />
                                        }}
                                    </Row>
                                    <Divider />
                                    <Row label="Markdown 阅读字体" hint="应用于编辑区与预览正文排版">
                                        {move || view! {
                                            <Select
                                                options=font_options()
                                                value=Signal::derive(move || state.md_font.get())
                                                width=230
                                                label="Markdown 正文字体"
                                                on_change=Callback::new(move |f: String| state.set_font(theme, "markdown", f))
                                            />
                                        }}
                                    </Row>
                                    <Divider />
                                    <Row label="等宽 / 代码字体" hint="应用于代码块与行内代码">
                                        {move || view! {
                                            <Select
                                                options=font_options()
                                                value=Signal::derive(move || state.mono_font.get())
                                                width=230
                                                label="代码等宽字体"
                                                on_change=Callback::new(move |f: String| state.set_font(theme, "mono", f))
                                            />
                                        }}
                                    </Row>
                                    <Divider />
                                    <Row label="正文行宽" hint="控制长文档的最大阅读宽度">
                                        <Segmented
                                            options=width_options
                                            value=Signal::derive(move || state.line_width.get())
                                            on_change=Callback::new(move |w: String| state.set_line_width(theme, w))
                                        />
                                    </Row>
                                    <Divider />
                                    {move || view! {
                                        <Row
                                            label=format!("正文字号 ({}px)", state.editor_font_size.get().round() as i32)
                                            hint="也可随时用 Ctrl+± 缩放，Ctrl+0 复位"
                                            wide=true
                                        >
                                            <Slider
                                                value=Signal::derive(move || state.editor_font_size.get() as f64)
                                                min=11.0
                                                max=24.0
                                                step=0.5
                                                label="正文字号"
                                                on_change=Callback::new(move |v: f64| state.set_editor_font_size(v as f32))
                                            />
                                        </Row>
                                    }}
                                </Card>

                                // 4. 编辑器首选项
                                <SectionTitle>"编辑器首选项"</SectionTitle>
                                <Card>
                                    <Row label="默认视图模式" hint="笔记工作区的分栏展示方式">
                                        <Segmented
                                            options=view_options
                                            value=Signal::derive(move || state.view_mode.get())
                                            on_change=Callback::new(move |m: String| state.set_view_mode(m))
                                        />
                                    </Row>
                                    <Divider />
                                    <Row label="自动保存" hint="停顿 500ms 后自动安全写入磁盘（关闭后需手动 Ctrl+S）">
                                        <Switch
                                            checked=Signal::derive(move || state.auto_save.get())
                                            label="自动保存"
                                            on_change=Callback::new(move |on: bool| {
                                                state.auto_save.set(on);
                                                state.patch_settings(|s| s.editor.auto_save = on);
                                            })
                                        />
                                    </Row>
                                    <Divider />
                                    <Row label="回车自动续接列表" hint="在无序/有序/任务列表行尾按 Enter 自动续接下一项">
                                        <Switch
                                            checked=Signal::derive(move || state.auto_continue_lists.get())
                                            label="回车自动续接列表"
                                            on_change=Callback::new(move |on: bool| {
                                                state.auto_continue_lists.set(on);
                                                state.patch_settings(|s| s.editor.auto_continue_lists = on);
                                            })
                                        />
                                    </Row>
                                </Card>

                                // 5. 关于与更新
                                <SectionTitle>"关于 Inkdown"</SectionTitle>
                                <Card>
                                    // 版本与检查按钮：状态（检查中 / 已是最新 / 有新版本 / 失败）
                                    // 直接写在这一行的说明里，不用再弹窗告诉用户结果。
                                    {move || {
                                        let st = state.update_state.get();
                                        let busy = state.update_busy.get();
                                        let hint = match &busy {
                                            Some(b) => {
                                                let label = b.label();
                                                format!("{label}（下载完成后应用会退出并静默安装，装好自动重开）")
                                            }
                                            None => st.summary(),
                                        };
                                        let checking = st.is_checking();
                                        let locked = checking || busy.is_some();
                                        view! {
                                            <Row label=format!("版本 v{}", env!("CARGO_PKG_VERSION")) hint=hint>
                                                <Button
                                                    label=if checking { "检查中…" } else { "检查更新" }
                                                    icon=IconName::REFRESH_CW
                                                    size=Size::Sm
                                                    disabled=locked
                                                    on_click=Callback::new(move |_| state.check_update())
                                                />
                                            </Row>
                                        }
                                    }}
                                    // 只有真发现新版本时才展开这一段，平时不占版面。
                                    {move || match state.update_state.get() {
                                        CheckState::Available(info) => {
                                            let notes = if info.notes.trim().is_empty() {
                                                "这一版没有写更新说明".to_string()
                                            } else {
                                                info.notes.clone()
                                            };
                                            let page = info.page_url.clone();
                                            let hint = new_version_hint(&info);
                                            let installing = state.update_busy.get().is_some();
                                            view! {
                                                <Divider />
                                                <Row label=format!("新版本 v{}", info.latest) hint=hint>
                                                    <Cluster>
                                                        <Button
                                                            label=if installing { "更新中…" } else { "下载并安装" }
                                                            icon=IconName::DOWNLOAD
                                                            size=Size::Sm
                                                            variant=ButtonVariant::Primary
                                                            disabled=installing
                                                            on_click=Callback::new(move |_| state.install_update())
                                                        />
                                                        <Button
                                                            label="发行页"
                                                            icon=IconName::EXTERNAL_LINK
                                                            size=Size::Sm
                                                            on_click=Callback::new(move |_| {
                                                                let url = page.clone();
                                                                spawn_local(async move {
                                                                    crate::tauri_bridge::open_external_url(&url).await;
                                                                });
                                                            })
                                                        />
                                                    </Cluster>
                                                </Row>
                                                <div class="ink-update__notes">{notes}</div>
                                            }
                                                .into_any()
                                        }
                                        _ => ().into_any(),
                                    }}
                                    <Divider />
                                    <Row label="数据哲学" hint="纯明文 Markdown 与本地附件，卸载应用后知识库依然完整可读">
                                        <Chip label="本地优先" icon=IconName::SHIELD />
                                    </Row>
                                </Card>
                            </Stack>
                        </div>
                    </Pad>
                </PageInset>
            </ScrollArea>
        </Page>
    }
}

/// 「新版本」那一行的说明：发布时间与安装包大小，没有就不写。
fn new_version_hint(info: &ReleaseInfo) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(day) = info.published_at.get(..10) {
        parts.push(format!("发布于 {day}"));
    }
    if let Some(name) = &info.asset_name {
        let size = crate::update::human_size(info.asset_size);
        parts.push(if size.is_empty() {
            format!("安装包 {name}")
        } else {
            format!("安装包 {name} · {size}")
        });
    }
    if parts.is_empty() {
        "更新说明见下方".to_string()
    } else {
        parts.join(" · ")
    }
}
