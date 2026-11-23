<!-- deep-ptc:project-cards v1 next=R1,F10,Q11 -->
# Project cards

> Maintained by Deep PTC in DeepSeek Harness: edit in a Deep PTC conversation, view 特点卡片 / Project cards.
> User-approved settings guide this project. Features and requirements are records that can be checked and corrected.

## User requirements / 用户要求
<!-- card id=requirements kind=requirements rev=5 updated=2026-11-23T04:48:23.019Z -->

- **Q4** [done] 软件启动时不得弹出任何终端/控制台窗口（含瞬间闪现）；宿主不得起控制台子进程去查系统信息。
- **Q5** [done] 设置页每个开关行的标签必须与它实际改变的选项对应，不得把子选项的文字挤在控件旁被裁掉。
- **Q6** [done] 需要应用内更新系统：检查 GitHub Releases 最新版本、显示更新说明、下载安装包并能完成安装。
- **Q7** [done] 顶栏主题胶囊必须提供「跟随系统」档，与设置页的三档（浅色/深色/跟随系统）一致，不能只有深浅互换。
- **Q8** [done] 删除文件夹必须整棵子树移入软件内回收站，可恢复；不得直接永久删除。
- **Q9** [done] 打开的页签在多标签时宽度保持一致，便于连续关闭。
- **Q10** [done] 更新检查要在每天的第一次启动时静默进行：当天只联网一次、不挡启动、失败不打扰；发现有新版本时用角标与一句话提示，不用弹窗打断。


## Implemented features / 已实现功能
<!-- card id=features kind=features rev=5 updated=2026-11-23T04:48:23.063Z -->

- **F4** 启动不再闪终端窗口：系统字体列表改为进程内读注册表（winreg，HKLM+HKCU），不再起 reg.exe 子进程；explorer 调用统一加 CREATE_NO_WINDOW。入口 src-tauri/src/lib.rs 的 collect_windows_fonts / font_families_of。验证：font_tests 4 个解析单测 + windows_font_list_looks_sane（真读本机注册表）。
- **F5** 沉浸文件夹设置拆成三行：第一行 Switch 控制沉浸本身（state.immersive → enter/exit_immersive），第二行「进入时收起其它目录」（immersive_collapse），第三行「保留上级路径」（immersive_keep_ancestors）；删掉手拼的 .l-row + .c-caption 裸文本。入口 ui/src/components/settings_page.rs。
- **F6** 更新系统：设置页「关于 Inkdown」比对 GitHub Releases（src-tauri/src/update.rs，ureq + native-tls，显式 TlsProvider::NativeTls），按三段数字比较版本；每天第一次启动静默查一次（localStorage 记 inkdown.update.checkedOn，当天查过不再联网，首屏后 1.8s 触发，失败静默跳过），有新版时左侧「偏好设置」挂「新」角标并提示一次；「下载并安装」把安装包下到 %LOCALAPPDATA%\Inkdown\updates\ 并发 update://progress，然后退出、cmd 静默 /S 安装、装好自动重开。入口 src-tauri/src/update.rs、ui/src/update.rs、ui/src/state/mod.rs、ui/src/components/settings_page.rs、ui/src/components/nav_sidebar.rs。验证：update_tests 6 个单测 + fetches_live_release（真联网下载校验
- **F7** 顶栏主题胶囊是三档分段（浅色/深色/跟随系统），值来自 Theme.mode，与设置页共用同一套取值。入口 ui/src/components/toolbar.rs。验证：npm run check / npm run build:ui。
- **F8** 删除文件夹走软件内回收站：delete_folder 把整棵子树 rename 到 .trash/{毫秒}-{名}，TrashEntry.isDir 标记目录条目；恢复保留原名与目录配色，只有永久删除/清空才清掉配色与索引子树。入口 ui/src/state/mod.rs、ui/src/vault/mod.rs。单测 vault::plan_tests::trash_name_keeps_dots_for_folders、forget_path_for_trash_clears_subtree_but_keeps_color。
- **F9** 页签等宽：.ink-tabs-wrap 内页签 flex: 1 1 0，min-width 112px / max-width 190px，装不下就横向滚动；固定页签（--pinned）不参与均分。入口 ui/inkdown-app.css。
