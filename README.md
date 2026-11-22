# Inkdown

本地优先的 Markdown 笔记应用，基于 **Tauri 2 + Rust + Leptos (CSR) + [GlassDesign](https://github.com/Genlue/GlassDesign)** 构建。

**一个文件夹就是一个知识库**，笔记是磁盘上普通的 `.md` 文件，配置只写在仓库内的 `.inkdown/` 目录里 —— 卸载应用后，你的全部笔记与附件依然完整可读。

当前版本 **1.0.3**。版本号规则见 [版本策略](#版本策略)。

---

## 界面与组件系统

Inkdown 的全部视觉语言、设计令牌（`var(--*)`）、液态玻璃光学折射材质与 100+ 桌面控件均来自 **GlassDesign (`v0.10.1`)**：

- **零字面色值**：应用层样式 `ui/inkdown-app.css` 只消费 GlassDesign 令牌，确保浅色 / 深色 / 自动主题与高对比度模式完全一致。
- **纯净本地笔记体验**：专注写作、双链、大纲联动与多标签页协作，无冗余 AI 模块或外部网络依赖。
- **玻璃效果三档**：外观设置里的「玻璃效果」可选完整效果 / 仅模糊 / 性能优先，档位写在 `<html data-gd-effects>` 并由 `localStorage` 记住，低配设备不必迁就折射与后景模糊。

---

## 版本策略

**功能更新只动第三位：**

```
0.1.0 → 0.1.1 → 0.1.2 → …
```

- **第二位（minor）和第一位（major）必须得到项目所有者的明确许可。**
- 版本号写在四处，**必须完全一致**：
  - `package.json` → `version`
  - `ui/Cargo.toml` → `[package] version`
  - `src-tauri/Cargo.toml` → `[package] version`
  - `src-tauri/tauri.conf.json` → `version`
- `npm run check` 会自动核对这四处；不一致直接 FAIL。

---

## 快速开始

前置：Node 18+、Rust 1.88+、`wasm32-unknown-unknown` target、`wasm-bindgen-cli 0.2.130`，以及 Tauri 2 系统依赖（Windows 需 WebView2）。

```bash
npm install          # 安装 @tauri-apps/cli
npm run build:ui     # 编译 Leptos WASM 前端到 ui/dist/
npm run dev          # 浏览器预览 http://127.0.0.1:8899
npm run tauri dev    # 启动桌面端（真实窗口、文件监听、系统字体、资源管理器联动）
npx tauri build      # 打包安装包：target/release/bundle/nsis/*.exe
npm run check        # 静态与编译自检（版本一致 / CSS 令牌 / 无 AI 残留 / cargo check & test）
```

---

## 核心能力

- **仓库与文件树**：一个文件夹就是一个仓库；首次启动自动初始化 `Documents/Inkdown Vault`；支持三种缩进引导线样式、拖拽移动、右键菜单、排序（最近编辑 / 最近创建 / 名称）与最近打开折叠条。
- **实时文件监听**：外部修改磁盘笔记时自动同步；若编辑器有未保存改动，挂出提示条让用户选择保留或加载新版。
- **多标签页与历史**：拖拽重排标签页、未保存小圆点指示、中键关闭、右键菜单、`Alt+←/→` 历史前进后退。
- **Markdown 编辑与实时预览**：
  - 纯编辑 / 双栏分屏（可拖比例）/ 仅预览三态切换
  - 回车自动续接列表、Tab 缩进、格式工具栏、单篇查找与替换（`Ctrl+F` / `Ctrl+H`）
  - `[[` 双链自动补全、预览区交互式任务清单勾选、代码块语法高亮与一键复制
  - 粘贴或拖入图片自动保存到 `attachments/` 并插入引用
  - 快速打开与命令面板（`Ctrl+P` / `Ctrl+O`）
- **右侧检查器**：大纲（滚动高亮与点击跳转）、关联（反向链接 / 出链 / 全库待建链接）、信息（字数 / 阅读时长 / 时间 / 路径）。
- **多格式文件视图**：代码语法高亮与行号、图片（适应/100%）、音视频、CSV 可排序数据表、PDF 预览、Office 文档卡片。
- **外观与排版**：浅色 / 深色 / 跟随系统、强调色盘、液态玻璃浓淡 `--lg` 滑块、玻璃效果三档（完整 / 仅模糊 / 性能优先）、UI / Markdown / 代码三区域系统字体自选、正文行宽与字号调节。

---

## 目录结构

```text
Inkdown/
├─ Cargo.toml              ← Rust 工作区（ui + src-tauri）
├─ ui/                     ← inkdown-ui（Leptos CSR + GlassDesign）
│  ├─ index.html
│  ├─ wallpaper.js
│  ├─ inkdown-app.css      ← 应用专属样式（仅消费 var(--*) 令牌）
│  └─ src/
│     ├─ lib.rs            ← 入口、全局快捷键与右键菜单
│     ├─ state/            ← 全局响应式状态与动作编排
│     ├─ vault/            ← 仓库模型、FS 适配（TauriFS / LocalFS）、双链与大纲索引
│     ├─ markdown/         ← 零依赖 Markdown 渲染器与离线代码高亮
│     └─ components/       ← 顶栏、导航、文件树、标签页、编辑器、检查器、设置页
├─ src-tauri/              ← Tauri 2 桌面宿主（glass-design-host + 仓库与窗口命令）
└─ tools/
   ├─ build-ui.mjs         ← WASM 编译与打包
   ├─ dev.mjs              ← 浏览器预览服务器
   └─ check.mjs            ← 自检脚本
```
