# Inkdown

本地优先的 Markdown 笔记应用。**一个文件夹就是一个仓库**，笔记是普通的 `.md` 文件，配置只写在
仓库内的 `.inkdown/` 里 —— 把应用删掉，笔记照样能读。

Tauri 2 桌面端；同一套代码也能用任意静态服务器在浏览器里跑（窗口能力自动降级为网页实现）。

当前版本 **0.1.0**。版本号规则见 [版本策略](#版本策略)。

---

## ⚠️ 界面与控件来自 Golden Gate HTML Design

**这是硬约束，不是建议。** Inkdown 不自建视觉语言，它的全部界面资产都来自同级目录的设计系统：

```
Project/
├─ Golden Gate HTML Design/    ← 设计系统，唯一来源
└─ Inkdown/                    ← 本仓库，只负责消费
```

（`Golden Gate HTML Design` 是独立仓库，不在本仓库内。本地开发时两者保持同级。）

### 要改界面，先去这里找

| 你要改什么 | 去看 |
|---|---|
| 颜色、圆角、间距、字号、动效时长 | `Golden Gate HTML Design/styles/tokens.css` |
| 按钮、输入框、菜单、抽屉、开关等控件 | `Golden Gate HTML Design/styles/components.css` + `scripts/ui/kit.js` |
| 页面栅格、栏宽、顶栏留白 | `Golden Gate HTML Design/styles/layout.css` |
| 图标 | `Golden Gate HTML Design/scripts/core/icons.js` |
| 玻璃材质 / 折射 / 高光 | `Golden Gate HTML Design/scripts/core/glass-*.js`、`refraction.js` |
| 设计规则（17 条硬规则） | `Golden Gate HTML Design/docs/06-rules.md`、`README.md` |
| 组件参数与用法 | `Golden Gate HTML Design/docs/03-components.md` |
| 曾经踩过的坑 | `Golden Gate HTML Design/docs/08-regressions.md` |

### 三条禁令

1. **禁止自造颜色。** 不写 `#hex`、`rgb()`、`hsl()`，只用 `var(--*)`。缺令牌就去 GG 加令牌。
2. **禁止自造控件。** 不新拼 `div` 冒充按钮/菜单/抽屉，先用 `kit.js` 里的工厂；缺控件就去 GG 补。
3. **禁止自造圆角/阴影/间距。** 圆角只从 `--r-xs/sm/md/lg/xl/window/pill` 取，间距只从 `--s-*` 取。

### 缺东西时的正确顺序

```
GG 里没有 → 先在 GG 里加（连同文档与预览快照）→ 再同步到 web/ → 在 Inkdown 里使用
```

**不要反过来。** 在 `web/` 里临时加一套，等于让两份实现开始漂移 —— 这个项目已经为漂移付过代价
（见下面的「同步现状」）。

### 同步现状（0.1.0）

`web/styles/` 的 4 份 CSS、`web/scripts/ui/kit.js`、`web/scripts/core/*` 都是 GG 对应文件的副本，
差异只有三类：

| 差异 | 说明 |
|---|---|
| 文件头命名 | `Lumina ·` → `Inkdown ·`，纯注释 |
| 应用侧增补 | 少量 Inkdown 专用图标与页面样式 |
| **玻璃交互降级** | `glass-lens.js` 的 `beginGlassInteraction` / `endGlassInteraction` **只在 Inkdown 侧**，GG 尚未回同步 |

最后一条是已知欠账：拖拽期间摘掉 `url(#filter)` 只留 `blur()+saturate()`，把 122ms/帧降到 5.7ms。
它属于设计系统能力，应回同步到 GG，但目前只在本仓库生效。

---

## 版本策略

**功能更新只动第三位。**

```
0.1.0 → 0.1.1 → 0.1.2 → …
```

- **第二位（minor）和第一位（major）必须得到项目所有者的明确许可**，不要"觉得改动够大"就升。
- 版本号写在三处，**必须完全一致**：
  - `package.json` → `version`
  - `src-tauri/Cargo.toml` → `[package] version`
  - `src-tauri/tauri.conf.json` → `version`
- `npm run check` 会核对这三处；不一致直接 FAIL。

---

## 快速开始

前置：Node 18+、Rust 1.77+、Tauri 2 的系统依赖（Windows 需 WebView2 运行时）。

```bash
npm install          # 只装 @tauri-apps/cli，前端零依赖

npm run dev          # 浏览器预览 http://127.0.0.1:8899
npm run tauri dev    # 桌面端（热重载，玻璃/材质走真实窗口）
npx tauri build      # 打包：src-tauri/target/release/bundle/nsis/*.exe

npm run check        # 静态自检（语法 / CSS 括号与注释 / 版本一致 / 令牌兜底）
```

浏览器预览与桌面端的差别：没有文件监听、没有 DWM 亚克力、`ai_stream` 退化为一次性请求。
除此之外功能一致 —— 这是刻意的，方便不装 Rust 也能改界面。

---

## 能做什么

**笔记与仓库**

- 一个文件夹 = 一个仓库；首次启动自动初始化 `Documents/Inkdown Vault`，设置里可切换其它文件夹
- 文件树四档层级样式：连续引导线 / 彩虹色块 / 简约框 / 无（设置里可切换）
- 实时文件监听：在资源管理器里增删改，应用内即时同步；编辑器有未保存改动时**不覆盖**，挂提示条让你选
- 手动刷新（列表按钮 / `Ctrl+Alt+R` / 窗口重新获焦）兜底
- 收藏、回收站、重命名、拖拽移动、按更新时间/名称排序

**编辑与阅读**

- 编辑器 + 实时预览分栏，栏宽可拖
- 自研零依赖 Markdown 渲染器：标题、列表、表格、引用、围栏代码、任务列表、`[[双链]]`、`![[图片嵌入]]`
- 标签页、面包屑、历史前进后退、大纲/信息/关系三个右栏视图
- 查找替换（`Ctrl+F` / `Ctrl+H`）、快速打开（`Ctrl+P`）、缩放（`Ctrl+±/0`）
- 代码 / 图片 / PDF / Office / CSV 走各自的视图，不塞进 Markdown 渲染器

**外观**

- 浅色 / 深色、强调色、系统亚克力与自绘玻璃两条材质通道
- 液态玻璃：SVG 位移贴图折射 + 边缘高光；拖拽与缩放期间自动降级，松手恢复
- 侧边导航 / 文件列表 / 右侧栏三个区域都可收起，栏宽记忆到仓库配置

**插件**

- 拔插式插件：一个文件夹 + `manifest.json`，启用/禁用到注册项级别可逆
- 首个内置插件 **AI 助手**：右栏常驻对话，可带当前笔记与选区、`@` 引用其他笔记，
  回复可一键插入/替换，改写作整篇回退

---

## 结构

```
Inkdown/
├─ web/                         ← 全部前端产物，零构建，直接就是最终代码
│  ├─ index.html
│  ├─ styles/                   ← tokens → base → layout → components → inkdown → plugins
│  │  ├─ tokens.css base.css layout.css components.css   ← 来自 GG
│  │  ├─ inkdown.css            ← 应用外壳与页面（本仓库自有）
│  │  └─ plugins.css            ← 插件页样式（本仓库自有）
│  ├─ scripts/
│  │  ├─ app.js                 ← 外壳：导航 / 列表 / 编辑器 / 快捷键 / 命令
│  │  ├─ ui/kit.js              ← 控件工厂（来自 GG）
│  │  ├─ core/                  ← DOM、图标、玻璃透镜、折射、主题、平台、高亮
│  │  ├─ vault/                 ← 仓库模型、文件系统适配、双链解析、监听
│  │  ├─ views/                 ← ViewHost 与各类型视图（设置 / 插件 / PDF / 代码 / 媒体 / Office）
│  │  ├─ sidebar/               ← 右栏视图注册表与内置视图
│  │  ├─ tabs/                  ← 标签页管理
│  │  └─ plugins/               ← 插件宿主：registry / host / loader / permissions / events
│  ├─ builtin-plugins/          ← 随应用分发的插件源码（首次运行写入仓库）
│  └─ assets/
├─ src-tauri/
│  ├─ src/lib.rs                ← 24 个命令：仓库读写、窗口、材质、AI 流式、文件监听
│  ├─ Cargo.toml
│  └─ tauri.conf.json
├─ tools/
│  ├─ check.mjs                 ← npm run check
│  └─ perf/                     ← 帧时间测量脚本（产物在 out/，已 gitignore）
└─ package.json                 ← 只有 @tauri-apps/cli 一个依赖
```

**零构建**是硬约束：没有 Vite、没有打包器，`web/` 就是运行产物。加前端依赖会破坏这个前提。

---

## 插件

插件装在 `<仓库>/.inkdown/plugins/<id>/`：

```
manifest.json   { id, name, version, main, style?, permissions? }
main.js         export async function activate(ctx) { ... }
style.css       可选
其他 .js        允许被 main.js 相对导入（加载器会重写相对路径）
```

状态**不在插件目录里**，由宿主记在 `.inkdown/plugins.json`（`enabled` / `granted`）——
插件目录可以被整体替换，状态必须独立。

```js
ctx = {
  id, version,
  app:     { version },
  vault:   { read, write, exists, list, walk },        // 需 vault:read / vault:write
  editor:  { getSelection, replaceSelection, insertAtCursor, currentNote, setContent, onSelectionChange },
  ui:      { navItem, page, sidebarView, command, settingsSection, toast, dialog, markdown, openSidebar },
  storage: { get, set },                               // 插件目录下 data.json
  net:     { fetch },                                  // 需 net
  events:  { on },                                     // note:open / note:save / theme:change
}
```

五类注册（导航项 / 页面 / 右栏视图 / 命令 / 设置段）**每一项都返回 disposer**，禁用时逆序回收。
ESM 求值后无法真正卸载，所以「禁用」的确切语义是**撤销全部副作用**，模块代码仍留在内存里。

权限在 manifest 里声明，未声明的能力调用时**直接抛错**。首次启用弹一次确认，选择信任才记账。
这是知情同意，不是沙箱 —— 桌面端插件能跑任意 JS。

---

## 数据在哪

```
<仓库>/
├─ 你的笔记.md
├─ .inkdown/
│  ├─ settings.json     外观、编辑器偏好
│  ├─ workspace.json    展开状态、栏宽、最近打开
│  ├─ index.json        标题/摘要索引缓存
│  ├─ trash.json        回收站索引
│  ├─ plugins.json      插件启用与授权记录
│  └─ plugins/<id>/     插件本体与 data.json
└─ .trash/
```

全部是明文 JSON/Markdown。**笔记不加密**，AI 插件的 API Key 也以明文存在
`.inkdown/plugins/ai/data.json` —— 别把笔记仓库同步到公开位置。

---

## 自检

`npm run check` 做五件确定性的事，不需要浏览器：

1. 所有前端模块 `node --check`
2. 每份 CSS 的括号配对与注释闭合（曾经有个未闭合注释吞掉 68 行缩放热区）
3. 重复章节标题
4. `package.json` / `Cargo.toml` / `tauri.conf.json` 三处版本一致
5. 引用了**从未定义**的令牌、且兜底是颜色字面量 → 提醒（这种写法在浅色主题下会整块发黑）

没有单元测试，也没有 CI。改动仍需在本机跑一遍界面再提交。

---

## 已知边界

- **只在 Windows 实测。** macOS/Linux 的文件监听后端（FSEvents / inotify）未验证；网络盘可能漏事件，靠刷新按钮兜底。
- **插件不能 import 宿主模块**，只能用 `ctx`。`ctx.net.fetch` 依赖 `net_fetch` 命令，目前尚未实现，桌面端会退回原生 fetch（受 CORS 限制）。
- **AI 的真实 SSE 链路未在桌面端实测**，浏览器里验证的是降级路径。
- **大仓库全量 `walk`**：每次文件变化都重扫，几千篇以内可接受，再大需要按路径增量。
- **会话按路径隔离**：重命名笔记会被当成一个新会话。

---

## 许可

私有项目，未经作者许可不得分发。设计系统 `Golden Gate HTML Design` 为独立仓库，另行管理。
