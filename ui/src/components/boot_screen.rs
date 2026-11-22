//! 启动进度屏：桌面端整库扫描期间给出明确反馈，别让窗口看起来像死机。

use glass_design::prelude::*;
use leptos::prelude::*;

use crate::vault::BootStage;

#[component]
pub fn BootScreen(stage: BootStage) -> impl IntoView {
    let percent = stage.percent();
    let current = stage;

    view! {
        <div class="ink-boot" role="status" aria-live="polite">
            <div class="ink-boot__card">
                <div class="ink-boot__brand">
                    <span class="ink-boot__mark">
                        <Icon name=IconName::LAYERS size=18 />
                    </span>
                    <span class="ink-boot__name">"Inkdown"</span>
                </div>
                <div class="ink-boot__caption">"正在打开知识库"</div>

                <div
                    class="ink-boot__track"
                    role="progressbar"
                    aria-valuemin="0"
                    aria-valuemax="100"
                    aria-valuenow=percent
                    aria-label="启动进度"
                >
                    <div class="ink-boot__fill" style=format!("width: {percent}%")></div>
                </div>

                <div class="ink-boot__row">
                    <span class="ink-boot__stage">{stage.label()}</span>
                    <span class="ink-boot__pct">{format!("{percent}%")}</span>
                </div>

                <ul class="ink-boot__steps">
                    {BootStage::ALL
                        .iter()
                        .map(|s| {
                            let done = s.percent() <= current.percent();
                            let active = *s == current;
                            view! {
                                <li
                                    class="ink-boot__step"
                                    data-done=done.then_some("1")
                                    data-active=active.then_some("1")
                                >
                                    <span class="ink-boot__step-dot"></span>
                                    <span>{s.label()}</span>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>

                <p class="ink-boot__note">
                    "首次打开或仓库较大时，扫描会花一点时间；进度走完即可开始写作。"
                </p>
            </div>
        </div>
    }
}
