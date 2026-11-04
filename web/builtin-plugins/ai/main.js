/* ==========================================================================
   AI 助手 · 入口
   一次激活注册五个东西：
   - 右侧栏对话视图（主入口：打开就能聊，不需要先选文字）
   - 导航项「AI」→ 展开右栏并切到对话，不再跳整页
   - 整页「AI 设置与历史」→ 只放连接设置与可回退的写入记录
   - 命令：打开对话 / 新建对话 / 回退上一次改写
   ========================================================================== */

import { createChatPanel } from './chat.js';
import { createSettingsPage } from './ui.js';
import { restoreLatest } from './history.js';
import { clearSession } from './sessions.js';

export async function activate(ctx) {
  const page = ctx.ui.page({
    id: 'ai',
    title: 'AI 设置与历史',
    icon: 'sparkles',
    mount: (host) => createSettingsPage(host, ctx),
  });

  const chat = ctx.ui.sidebarView({
    id: 'ai.chat',
    label: 'AI',
    icon: 'sparkles',
    mount: (host) => createChatPanel(host, ctx, { openSettings: () => page.open() }),
  });

  const nav = ctx.ui.navItem({
    id: 'ai',
    label: 'AI',
    icon: 'sparkles',
    order: 10,
    onClick: () => ctx.ui.openSidebar('ai.chat'),
  });

  const cmds = [
    ctx.ui.command({
      id: 'ai.open-chat',
      title: 'AI: 打开对话',
      icon: 'sparkles',
      scope: 'global',
      run: () => ctx.ui.openSidebar('ai.chat'),
    }),
    ctx.ui.command({
      id: 'ai.new-chat',
      title: 'AI: 新建对话',
      icon: 'plus',
      scope: 'global',
      run: () => {
        const note = ctx.editor.currentNote();
        if (!note) {
          ctx.ui.toast('先打开一篇笔记', { icon: 'info' });
          return;
        }
        clearSession(ctx, note.path);
        ctx.ui.openSidebar('ai.chat');
        ctx.ui.toast('已新建对话', { icon: 'sparkles', duration: 1600 });
      },
    }),
    ctx.ui.command({
      id: 'ai.undo-rewrite',
      title: 'AI: 回退上一次改写',
      icon: 'rotate-ccw',
      scope: 'global',
      run: () => restoreLatest(ctx),
    }),
  ];

  return () => {
    cmds.forEach((off) => off());
    nav();
    chat();
    page();
  };
}
