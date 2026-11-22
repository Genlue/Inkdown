//! 新建仓库时写入的示例笔记与目录。

pub struct SeedNote {
    pub path: &'static str,
    pub content: String,
    pub favorite: bool,
    pub created_at: u64,
    pub mtime: u64,
}

pub const SEED_FOLDERS: &[&str] = &["Notes", "Notes/设计", "Projects", "assets"];

const WELCOME: &str = r#"# 欢迎使用 Inkdown

标签：#入门 #产品/Inkdown

Inkdown 是一个本地优先的 Markdown 笔记应用：一个文件夹就是一个知识库，笔记就是磁盘上的 `.md` 文件。

## 从这三件事开始

1. 点击左栏的 **新建笔记**，或者按 `Ctrl+N`
2. 在编辑区写下第一行字，右侧实时预览同步排版
3. 按 `Ctrl+S` 立即保存 —— 平时改动也会自动安全落盘

> 笔记是纯粹的 Markdown 文本与本地附件。没有数据库，没有厂商锁定。

## 常用快捷键

| 快捷键 | 作用 |
| --- | --- |
| `Ctrl+P` / `Ctrl+O` | 快速打开笔记 / 命令面板 |
| `Ctrl+N` | 新建笔记（`Ctrl+Shift+N` 新建文件夹） |
| `Ctrl+S` | 立即保存 |
| `Ctrl+F` / `Ctrl+H` | 单篇查找 / 替换 |
| `Ctrl+K` | 聚焦文件搜索框 |
| `Ctrl+\` | 收起 / 展开导航栏 |
| `Ctrl+Shift+B` | 收起 / 展开文件树 |
| `Ctrl+Shift+E` | 收起 / 展开右侧检查器 |
| `Alt+←` / `Alt+→` | 浏览历史后退 / 前进 |
| `Ctrl+W` | 关闭当前文档标签页 |

## Markdown 语法与双向链接

- **加粗**、*斜体*、~~删除线~~、==高亮==、`行内代码`
- [外部链接](https://github.com/Genlue/GlassDesign)
- 任务清单（预览区可直接点击勾选）：
  - [x] 安装并启动 Inkdown
  - [x] 采用 GlassDesign 液态玻璃界面体系
  - [ ] 写下你的第一篇知识笔记

```rust
fn main() {
    println!("你好，Inkdown × GlassDesign!");
}
```

## 双向链接与本地图片

- 双向链接：跳转到 [[Markdown 语法速查]] 或别名 [[阅读清单|我的近期书单]]
- 键入 `[[` 会自动弹出笔记补全列表
- 支持粘贴或拖拽图片直接保存到 `attachments/` 目录
"#;

const CHEATSHEET: &str = r#"# Markdown 语法速查

标签：#语法 #Markdown

> 相关：[[欢迎使用 Inkdown]]

## 标题

```markdown
# 一级标题
## 二级标题
### 三级标题
```

## 强调与行内格式

- `**加粗**` → **加粗**
- `*斜体*` → *斜体*
- `~~删除线~~` → ~~删除线~~
- `==高亮==` → ==高亮==
- `` `行内代码` `` → `行内代码`

## 列表与任务

- 无序列表用 `-`、`*` 或 `+`
- 缩进两个空格即可嵌套子项
  - 像这样
  - 再深一层也可以

1. 有序列表用 `1.`
2. 回车会自动续接下一行列表

- [x] 已完成事项
- [ ] 待办事项

## 引用与代码块

> 引用用来放旁注、摘录，或者一句想强调的话。

```ts
const greet = (name: string) => `Hello, ${name}`;
console.log(greet("Inkdown"));
```

## 表格

| 左对齐 | 居中 | 右对齐 |
| :--- | :---: | ---: |
| 文本 | 文本 | 12.00 |
| 更长的文本 | 也可以 | 340.50 |
"#;

const MEETING: &str = r#"# 周会纪要 · 产品与体验评审

标签：#产品/会议

**参会**：产品、设计、客户端

## 本周结论

1. 界面全面采用 GlassDesign 液态玻璃组件库（Rust + Leptos）重构
2. 专注于纯正本地笔记体验，移除冗余功能，保持轻快专注
3. 强化双链、标签聚合、大纲联动与多标签页协作

## 待办

- [x] 确定技术栈：Rust + Tauri 2 + GlassDesign
- [x] 实现本地仓库与实时文件监听
- [x] 完善 Markdown 查找替换与双链补全
- [ ] 开始整理个人知识库

> 下次评审：聚焦长文档滚动与键盘无障碍操作。
"#;

const READING: &str = r#"# 阅读清单

标签：#阅读 #设计

## 在读

- 《设计中的设计》—— 原研哉
- 《The Design of Everyday Things》

## 想读

1. 《A Philosophy of Software Design》
2. 《Refactoring UI》
3. 《The Humane Interface》

## 摘录

> 设计不是把东西做得更漂亮，而是让人与物之间的关系变得更清楚。

读到好的段落就贴在这里，标上页码，回头找起来方便。
"#;

const GLASS: &str = r#"# 关于「液态玻璃」的三条笔记

标签：#设计 #GlassDesign

## 1. 材质是分层的

界面上的每一块玻璃都要能回答一个问题：它压在谁上面。
侧栏贴着壁纸长期占位，用实心窗格；浮在内容上的胶囊与浮层才用折射玻璃。

## 2. 上层必须更厚

两层半透明叠在一起，前后关系会读不出来。所以层级越高，染色越重：

| 层级 | 用在哪 | 浓度 |
| --- | --- | --- |
| L1 | 顶栏 | 0.80 |
| L2 | 面板 | 0.88 |
| L3 | 菜单 | 0.94 |
| L4 | 模态 | 0.96 |

## 3. 动效只动 transform

导航栏开合时不要动画化列宽 —— 那是布局属性，每帧重排，和合成器上的 `transform` 不在同一条时间线上，中途就会看到两条边裂开。
"#;

pub fn seed_notes(now: u64) -> Vec<SeedNote> {
    let min = 60 * 1000;
    let hour = 60 * min;
    let day = 24 * hour;

    vec![
        SeedNote {
            path: "Notes/欢迎使用 Inkdown.md",
            content: WELCOME.to_string(),
            favorite: true,
            created_at: now.saturating_sub(3 * day),
            mtime: now.saturating_sub(2 * min),
        },
        SeedNote {
            path: "Notes/Markdown 语法速查.md",
            content: CHEATSHEET.to_string(),
            favorite: true,
            created_at: now.saturating_sub(6 * day),
            mtime: now.saturating_sub(hour),
        },
        SeedNote {
            path: "Projects/周会纪要 · 产品与体验评审.md",
            content: MEETING.to_string(),
            favorite: false,
            created_at: now.saturating_sub(5 * hour),
            mtime: now.saturating_sub(3 * hour),
        },
        SeedNote {
            path: "Notes/阅读清单.md",
            content: READING.to_string(),
            favorite: false,
            created_at: now.saturating_sub(9 * day),
            mtime: now.saturating_sub(day),
        },
        SeedNote {
            path: "Notes/设计/关于「液态玻璃」的三条笔记.md",
            content: GLASS.to_string(),
            favorite: false,
            created_at: now.saturating_sub(12 * day),
            mtime: now.saturating_sub(3 * day),
        },
        SeedNote {
            path: "Projects/hello.rs",
            content: "// Rust 示例文件\nfn main() {\n    println!(\"Hello from Inkdown!\");\n}\n"
                .to_string(),
            favorite: false,
            created_at: now.saturating_sub(2 * day),
            mtime: now.saturating_sub(day),
        },
        SeedNote {
            path: "Projects/quarterly-stats.csv",
            content: "季度,营收 (万元),活跃用户,环比增长\n2026 Q1,328.5,14200,+18.2%\n2026 Q2,410.2,18900,+24.8%\n2026 Q3,495.0,23500,+20.6%\n2026 Q4,580.4,28400,+17.2%\n".to_string(),
            favorite: false,
            created_at: now.saturating_sub(day),
            mtime: now.saturating_sub(4 * hour),
        },
    ]
}
