# AGENTS.md

给在这个仓库里工作的 AI 代理的说明书。人类也适用。

## 开工前先读

| 文件 | 为什么 |
|---|---|
| `README.md` | 项目是什么、怎么跑、**版本策略**、已知边界 |
| `../Golden Gate HTML Design/README.md` | 设计系统的 17 条硬规则 |
| `../Golden Gate HTML Design/docs/06-rules.md` | 每条规则的原因与判据 |
| `../Golden Gate HTML Design/docs/03-components.md` | 控件参数与用法 |
| `../Golden Gate HTML Design/docs/08-regressions.md` | 踩过的坑，动手前扫一遍 |

## 硬规则

### 1. 界面资产只能来自 `../Golden Gate HTML Design`

这个项目**不自建视觉语言**。颜色、间距、圆角、控件、图标、玻璃材质，全部从设计系统取。

- 只用 `var(--*)`，不写 `#hex` / `rgb()` / `hsl()`；圆角只从 `--r-xs/sm/md/lg/xl/window/pill` 取。
- 控件只从 `web/scripts/ui/kit.js` 的工厂拿，不要把 `div` 拼成按钮、菜单、抽屉。
- 设计系统里**没有**的：先在 GG 里加（连同文档与 `index.html` 预览快照），再同步到 `web/`。
  顺序不能反过来 —— 在 `web/` 里临时加一套，两份实现就开始漂移。
- **找不到 GG 文件夹时停下来问。** 只 clone 了本仓库的情况下，不要照记忆或想象复刻一套样式，
  也不要"先随便写个能看的"。

### 2. 版本号只动第三位

```
0.1.0 → 0.1.1 → 0.1.2 → …
```

**minor / major 必须得到项目所有者的明确许可。** 不要因为"改动挺大"就升到 0.2.0。

版本写在三处，必须一致：`package.json`、`src-tauri/Cargo.toml`（`[package]`）、
`src-tauri/tauri.conf.json`。`npm run check` 会核对。

### 3. 零构建

没有 Vite / 打包器，`web/` 就是最终产物。**不加前端依赖**。加一个依赖就让"改完刷新即可"失效。

### 4. 其它

- 中文注释与界面文案，注释说明**为什么**，不复述代码在干什么。
- 改完必须 `npm run check` PASS；动 Rust 还要 `cargo check` 与 `cargo test --lib`。
- 不要用"以后再说"的 TODO 交差。做不到就在报告里写清楚做不到。

## 结构提示

- `web/scripts/app.js` —— 外壳（导航、列表、编辑器、快捷键）。**已经很大，新功能优先放新模块。**
- `web/scripts/ui/kit.js` —— 控件工厂，来自 GG，不要在这里发明 API。
- `web/scripts/plugins/` —— 插件宿主；插件本体在 `web/builtin-plugins/<id>/`。
- `src-tauri/src/lib.rs` —— 24 个命令。所有路径参数都是**仓库内相对路径**，逐段拒绝 `..`。
- `web/styles/inkdown.css`（本仓库自有）与 `components.css`（来自 GG）职责不同，别往后者塞页面样式。

## 四个反复踩过的坑

1. **令牌兜底写死深色**：`var(--surface-float, rgba(30,30,36,.92))` 里令牌根本没定义，浅色主题下整块发黑。
   `npm run check` 会提醒这类写法。
2. **CSS 注释没闭合**：会静默吞掉后面整段规则（曾吞掉 8 个窗口缩放热区）。
3. **两套文件系统适配器返回形状不一致**：`TauriFS` 必须和 `LocalFS` 一样带 `name`，
   否则只在桌面端炸、浏览器里完全正常。
4. **插件不能 `import` 宿主模块**：加载器只重写插件目录内的相对导入。宿主能力一律经 `ctx`。

## 验证习惯

这里**没有单元测试，也没有 CI**。界面改动必须真的跑起来看：

```bash
npm run dev        # 浏览器，http://127.0.0.1:8899
npm run tauri dev  # 桌面端，玻璃/材质/文件监听/流式请求只有这里才是真的
```

浏览器预览会降级掉一些能力（DWM 亚克力、文件监听、`ai_stream` 流式），
**只测浏览器就宣称"完成"，等于没测。**
