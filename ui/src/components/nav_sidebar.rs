//! 左侧导航栏（使用 GlassDesign NavColumn + NavInfo + NavGroup + NavItem）。

use glass_design::prelude::*;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::state::{AppState, ContextMenuKind, ContextMenuState, NavPage};

#[component]
pub fn AppNavSidebar() -> impl IntoView {
    let state = AppState::get();

    let vault_subtitle = move || {
        if let Some(root) = state.vault_root_path() {
            root
        } else {
            "浏览器沙盒仓库".to_string()
        }
    };

    let open_vault_menu = move |ev: web_sys::MouseEvent| {
        if let Some(target) = ev
            .current_target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        {
            let rect = target.get_bounding_client_rect();
            state.context_menu.set(Some(ContextMenuState {
                x: rect.left(),
                y: rect.bottom() + 4.0,
                kind: ContextMenuKind::VaultSwitcher,
            }));
        }
    };

    view! {
        <NavColumn>
            <div on:click=open_vault_menu>
                {move || view! {
                    <NavInfo
                        avatar="./assets/inkdown.png"
                        app_icon=true
                        title=state.vault_name()
                        subtitle=vault_subtitle()
                        on_click=Callback::new(move |_| {})
                    />
                }}
            </div>

            <NavGroup>
                <NavSection label="知识库" />
                {move || {
                    let counts = state.counts();
                    view! {
                        <NavItem
                            icon=IconName::FOLDER_TREE
                            label="文件"
                            count=counts.all as u32
                            selected=Signal::derive(move || matches!(state.nav_page.get(), NavPage::Files))
                            on_click=Callback::new(move |_| {
                                state.nav_page.set(NavPage::Files);
                                if state.list_hidden.get_untracked() {
                                    state.toggle_list();
                                }
                            })
                        />
                        <NavItem
                            icon=IconName::STAR
                            label="收藏"
                            count=counts.favorites as u32
                            selected=Signal::derive(move || matches!(state.nav_page.get(), NavPage::Favorites))
                            on_click=Callback::new(move |_| {
                                state.nav_page.set(NavPage::Favorites);
                                if state.list_hidden.get_untracked() {
                                    state.toggle_list();
                                }
                            })
                        />
                        <NavItem
                            icon=IconName::TRASH
                            label="回收站"
                            count=counts.trash as u32
                            selected=Signal::derive(move || matches!(state.nav_page.get(), NavPage::Trash))
                            on_click=Callback::new(move |_| {
                                state.nav_page.set(NavPage::Trash);
                                if state.list_hidden.get_untracked() {
                                    state.toggle_list();
                                }
                            })
                        />
                    }
                }}

                <NavSection label="系统" />
                <NavItem
                    icon=IconName::SLIDERS
                    label="偏好设置"
                    selected=Signal::derive(move || matches!(state.nav_page.get(), NavPage::Settings))
                    on_click=Callback::new(move |_| state.nav_page.set(NavPage::Settings))
                />
                <NavItem
                    icon=IconName::KEYBOARD
                    label="快捷键速查"
                    on_click=Callback::new(move |_| state.shortcuts_open.set(true))
                />
            </NavGroup>

            <div class="l-nav__foot">
                <div class="ink-foot">
                    <span class="ink-foot__dot" aria-hidden="true"></span>
                    <span>{if state.is_desktop { "本地文件 · 自动保存" } else { "浏览器沙盒 · 自动保存" }}</span>
                </div>
            </div>
        </NavColumn>
    }
}
