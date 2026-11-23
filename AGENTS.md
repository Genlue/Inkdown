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
- **自造磨砂面必须跟随性能档**：库只管自己的 `.c-*` 材质；若将来给侧栏、自定义顶栏等写 `backdrop-filter`，要同时写 `:root[data-gd-effects="solid"]` 下的不透明降级，否则「性能优先」只生效一半。
- **不引入 AI 功能或外部网络请求**：保持纯正、轻快的本地 Markdown 知识库体验。唯一例外是更新检查——每天第一次启动时静默查一次 GitHub Releases（当天已查过就不再联网），以及用户点「检查更新」时再查；只取版本号与更新说明，不上报任何数据。再要加联网功能之前必须先问过项目所有者。

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

### 5. 文件树相关的不变量

- **目录颜色是数据，不是样式**：色板在 `vault::FOLDER_PALETTE`，由节点算出十六进制值后写进行内 `--folder-color`；`ui/inkdown-app.css` 仍然只消费 `var(--*)`。别把十六进制搬进 CSS。
- **快照写在 `.inkdown/history/`**：这些路径与索引、工作区一样是隐藏目录，桌面端的 `vault://fs-change` 在 UI 层已经过滤掉隐藏路径——去掉那段过滤会让每次保存都触发整仓 `walk`。
- **多选与沉浸都不能改磁盘**：选择集、沉浸范围都是纯视图状态；只有批量移动/删除才落盘，且必须复用 `plan_move` / 回收站流程。
- **Markdown 配色是一套语义系统，不是装饰**：标题只允许用 `--accent` 与正文色做浓度混合（层级越深越淡），语义色只给「可点击 / 需标记 / 已完成」三类元素。新增样式时别引入第四种含义，也别绕过 `.md-body[data-md-color]` 的变量直接写颜色——否则「关闭」档会漏色。
- **需要跟随主题的浮层不能交给原生控件**：`<select>` 的展开列表由 WebView2 自己画，深色主题下会出现白底 + 白字（库里的 `GlassEffectsSetting` 就是原生 select，设置页已改用库内 `Select`）。同样别在大纲、菜单这类地方自造原生 popup。

### 6. 启动与输入热路径

- **链接索引不能堵首屏**：`rebuild_link_index()` 要读全库正文，启动流程里已经用 900ms 定时器推到首屏之后，并靠 `indexing` 信号防止和保存后的重建叠着跑。改启动流程时别把它挪回同步路径。
- **预览渲染必须防抖**：`editor_content` 每次击键都变，直接 `render_markdown()` + 换 `innerHTML` 在长文档里是每键 60–90ms 的长任务。当前 90ms 防抖，换文档立即渲染、「仅编辑」模式不渲染。分屏锚点镜像同理只在换文档 / 切视图 / 改字号时预建，内容变化只标脏。
- **被取消的定时器闭包会泄漏**：`Closure::once_into_js` 只有回调真正执行时才释放 Rust 侧数据。凡是会 `clearTimeout` 的防抖任务，必须把 timer 句柄和 `Closure` 一起存进 `StoredValue`，取消时 `set_value(None)` 让闭包和它捕获的整篇正文一起 drop。

### 7. 启动链路

- **慢活不许留在 Tauri 同步 command**：`vault_walk` / `system_fonts` / `vault_open` 里的 watcher 注册已经分别走 `spawn_blocking` 与后台线程。Tauri 的同步 command 跑在窗口主线程，整库递归一堵，WebView2 的消息循环就停——鼠标变等待、进度动画也画不出来，这才是「打开卡几秒」的根因。
- **`walk()` 跳过点开头目录**：与前端 `is_hidden()` 规则一致，这些路径本来不进文件树；递归进去只会让 `.git` / `.inkdown/history` / `.trash` 白拖慢启动。改扫描逻辑时别把它们加回来。
- **启动只 walk 一次**：`open_folder()` 用 `load()` 之后的 `vault.entries` 判断有没有笔记，别为了这个判断再单独扫一遍整库。
- **阶段上报与绘制绑定**：`report_stage()` 上报后会 `yield_paint()` 让出一帧（只在启动期，非启动期零开销），否则连续几个同步阶段会被压成一帧、进度条看着像跳变。`index.html` 的静态过渡屏只负责 WASM 实例化前的白板时间，`start()` 挂载完必须移除。
