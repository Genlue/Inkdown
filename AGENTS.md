# AGENTS.md

给在这个仓库里工作的 AI 代理与贡献者的说明。

## 开工前先读

| 文件 | 为什么 |
|---|---|
| `README.md` | 项目结构、运行方式、**版本策略** |
| `../GlassDesign/README.md` | GlassDesign 界面库与液态玻璃规范 |
| `../GlassDesign/docs/02-组件契约.md` | 104 个组件 Props、受控语义与边界 |
| `../GlassDesign/docs/05-接入手册.md` | 宿主壳层、壁纸接线与材质档位 |

## 硬规则

### 1. 界面资产全部来自 GlassDesign

- **禁止自造颜色**：`ui/inkdown-app.css` 里不写 `#hex` / `rgb()` / `hsl()`，只消费 `var(--*)` 令牌。
- **优先使用 `glass_design::prelude::*` 组件**：布局用 `AppShell` / `Toolbar` / `NavColumn` / `Main` / `ScrollArea` / `Page` / `Pad` / `Stack` / `Cluster` / `Center` / `SplitView`；浮层统一走 `Dialog` / `Menu` / `MenuButton` / `toast`。
- **不引入 AI 功能或外部网络请求**：保持纯正、轻快的本地 Markdown 知识库体验。

### 2. 版本号只动第三位

```
0.1.0 → 0.1.1 → 0.1.2 → …
```

**minor / major 必须得到项目所有者的明确许可。**

版本号写在四处，必须完全一致：`package.json`、`ui/Cargo.toml`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`。`npm run check` 会核对。

### 3. 仓库借用不能跨 `await`

`AppState.vault` 是 `Rc<RefCell<Option<Vault>>>`，而渲染随时会在 `.await` 期间读它。
只要有一个 `borrow_mut()` 活过 await 点，读仓库就是一次 `RefCell already mutably borrowed`
panic —— wasm 里 panic 等于 trap，界面从此停在加载页、点哪都不动。

异步操作一律拆三段：`vault_fs()` 拿句柄 → 借用之外做 I/O → `with_vault_mut()` 写回内存。
`Vault` 上只保留同步的内存操作，磁盘读写由调用方用 `VaultFs` 完成。

### 4. 其它

- 中文注释与界面文案，注释说明**为什么**，不复述代码在干什么。
- 改完必须跑通 `npm run check`（含 `cargo check` 与 `cargo test --lib`）以及 `npm run build:ui`。
- 浏览器预览（`npm run dev`）和桌面端（`npm run tauri dev`）能力边界不同（桌面端独有本地文件夹、文件监听、系统字体与窗口控制），重大改动需两边都验。
