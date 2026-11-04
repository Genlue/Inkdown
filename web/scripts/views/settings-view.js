/* ==========================================================================
   Inkdown · SettingsView
   macOS 原生卡片式设置视图（多区域字体自选 + 外观 + 编辑器首选项）
   ========================================================================== */

import { h } from '../core/dom.js';
import { theme } from '../core/theme.js';
import { setSidebarBackdrop, currentBackdrop } from '../core/platform.js';
import { CardGroup, Row, Segmented, Swatches, Select, Button, toast } from '../ui/kit.js';

const SYSTEM_DEFAULT_FONTS = [
  '系统默认 (System Default)',
  'Microsoft YaHei',
  'PingFang SC',
  'Segoe UI',
  'Segoe UI Variable Text',
  'Inter',
  'SF Pro Text',
  'Arial',
  'Helvetica Neue',
  'Cascadia Code',
  'Cascadia Mono',
  'Consolas',
  'Fira Code',
  'JetBrains Mono',
  'SF Mono',
  'Courier New',
  'SimSun',
  'KaiTi',
  'FangSong',
  'Georgia',
  'Times New Roman',
];

export function createSettingsView({ vault, onFontChange, onThemeChange, onPickFolder, onOpenInExplorer }) {
  const container = h('div', { class: 'ink-settings-view' });

  const scrollBody = h('div', { class: 'ink-settings-view__body l-pad l-stack l-stack--7', style: { maxWidth: '640px', margin: '0 auto', width: '100%', padding: '24px 20px 60px' } });
  container.append(scrollBody);

    // 仓库与存储卡片
  const isRealFolder = Boolean(vault.isFolder && vault.rootPath);
  const vaultPathText = isRealFolder
    ? vault.rootPath
    : '浏览器虚拟存储 (localStorage)';

  const vaultActions = h('div', { class: 'l-cluster l-cluster--2', style: { marginTop: '8px', flexWrap: 'wrap', gap: '8px' } });

  if (isRealFolder) {
    vaultActions.append(
      Button({
        label: '在资源管理器中打开',
        icon: 'folder-open',
        variant: 'primary',
        size: 'sm',
        onClick: () => {
          if (onOpenInExplorer) {
            onOpenInExplorer('');
          } else {
            toast('已请求在文件资源管理器中打开', { icon: 'folder-open' });
          }
        },
      }),
      Button({
        label: '复制路径',
        icon: 'copy',
        variant: 'subtle',
        size: 'sm',
        onClick: async () => {
          await navigator.clipboard.writeText(vault.rootPath);
          toast('仓库绝对路径已复制到剪贴板', { icon: 'check' });
        },
      }),
      Button({
        label: '切换其它文件夹…',
        icon: 'folder-symlink',
        variant: 'subtle',
        size: 'sm',
        onClick: () => onPickFolder?.(),
      })
    );
  } else {
    vaultActions.append(
      Button({
        label: '选择本地真实文件夹…',
        icon: 'folder-open',
        variant: 'primary',
        size: 'sm',
        onClick: () => onPickFolder?.(),
      })
    );
  }

  const vaultCard = CardGroup({
    title: '当前仓库与存储',
    children: [
      Row({
        label: '仓库名称',
        hint: isRealFolder ? '基于纯本地文件夹构建的知识库' : '当前为内存沙盒演示仓库',
        control: h('span', { class: 'c-subtitle', style: { fontWeight: '600' } }, vault.meta.name || '我的仓库'),
      }),
      Row({
        label: '存储位置 (绝对路径)',
        hint: isRealFolder
          ? '文件直接保存在该目录下，纯透明文本与附件，无厂商锁定'
          : '笔记保存在浏览器本地缓存中。推荐连接本地目录',
        stack: true,
        control: h(
          'div',
          { style: { width: '100%' } },
          h('div', {
            style: {
              fontFamily: 'var(--ink-font-mono)',
              fontSize: '12px',
              padding: '6px 10px',
              userSelect: 'all',
              wordBreak: 'break-all',
              whiteSpace: 'normal',
              lineHeight: '1.4',
              borderRadius: '6px',
              backgroundColor: 'var(--surface-sunken)',
              border: '1px solid var(--line-subtle)',
              marginBottom: '6px',
            },
          }, vaultPathText),
          vaultActions
        ),
      }),
    ],
  });

  const currentSettings = vault.settings || {};
  const currentFonts = currentSettings.fonts || {
    ui: '',
    markdown: '',
    mono: '',
  };

  // 外观卡片
  const appearanceSeg = Segmented({
    value: theme.mode,
    options: [
      { value: 'light', label: '浅色' },
      { value: 'dark', label: '深色' },
      { value: 'system', label: '自动' },
    ],
    onChange: (v) => {
      theme.setMode(v);
      vault.patchSettings({ appearance: { mode: v, accent: theme.accent } });
      onThemeChange?.();
    },
  });

  const swatchRow = Swatches({
    value: theme.accent,
    onChange: (hex) => {
      theme.setAccent(hex);
      vault.patchSettings({ appearance: { ...vault.settings.appearance, mode: theme.mode, accent: hex } });
      onThemeChange?.();
    },
  });

  const currentTreeIndent = currentSettings.appearance?.treeIndent || 'lines';
  const treeIndentSeg = Segmented({
    value: currentTreeIndent,
    options: [
      { value: 'lines', label: '连续引导线' },
      { value: 'rainbow', label: '彩虹色块框' },
      { value: 'boxes', label: '简约框' },
      { value: 'none', label: '无' },
    ],
    onChange: (v) => {
      vault.patchSettings({ appearance: { ...vault.settings.appearance, treeIndent: v } });
      const tree = document.querySelector('.ink-tree');
      if (tree) tree.dataset.indent = v;
      toast(`已切换文件树层级样式: ${v === 'lines' ? '连续引导竖线' : v === 'rainbow' ? '彩虹色块框' : v === 'boxes' ? '简约框' : '无'}`);
    },
  });

  const currentBackdropVal = currentSettings.appearance?.backdrop || currentBackdrop() || 'system';
  const backdropSeg = Segmented({
    value: currentBackdropVal,
    options: [
      { value: 'system', label: '系统 Acrylic (DWM)' },
      { value: 'css', label: '自绘 CSS 磨砂' },
    ],
    onChange: async (v) => {
      await setSidebarBackdrop(v);
      vault.patchSettings({ appearance: { ...vault.settings.appearance, backdrop: v } });
      toast(`已切换侧边栏材质: ${v === 'system' ? '系统 Acrylic (Windows DWM)' : '自绘 CSS 磨砂'}`);
    },
  });

  const appearanceCard = CardGroup({
    title: '外观与主题',
    children: [
      Row({ label: '外观模式', hint: '选择界面色彩风格', control: appearanceSeg }),
      Row({ label: '强调色', hint: 'macOS 系统主色盘', stack: true, control: swatchRow }),
      Row({
        label: '侧边栏材质',
        hint: '系统 Acrylic 提供原生 Windows DWM 真实背景模糊；自绘 CSS 磨砂为跨平台自绘玻璃',
        control: backdropSeg,
      }),
      Row({ label: '文件树层级样式', hint: '同级节点引导线（Obsidian 风格）或彩虹色块框', control: treeIndentSeg }),
    ],
  });

  // 字体配置区域
  const typographyCardContainer = h('div');

  const buildTypographyCard = (fontList) => {
    // 制作下拉选项
    const fontOptions = [
      { value: '', label: '系统默认 (System Default)' },
      ...fontList
        .filter((f) => f && f !== '系统默认 (System Default)')
        .map((f) => ({ value: f, label: f })),
    ];

    const uiFontSelect = Select({
      options: fontOptions,
      value: currentFonts.ui || '',
      width: '220px',
      label: '界面字体',
      onChange: (v) => {
        currentFonts.ui = v;
        vault.patchSettings({ fonts: { ...currentFonts, ui: v } });
        onFontChange?.('ui', v);
        toast(`界面字体已设置为: ${v || '系统默认'}`);
      },
    });

    const mdFontSelect = Select({
      options: fontOptions,
      value: currentFonts.markdown || '',
      width: '220px',
      label: 'Markdown 正文字体',
      onChange: (v) => {
        currentFonts.markdown = v;
        vault.patchSettings({ fonts: { ...currentFonts, markdown: v } });
        onFontChange?.('markdown', v);
        toast(`Markdown 正文字体已设置为: ${v || '系统默认'}`);
      },
    });

    const monoFontSelect = Select({
      options: fontOptions,
      value: currentFonts.mono || '',
      width: '220px',
      label: '代码与等宽字体',
      onChange: (v) => {
        currentFonts.mono = v;
        vault.patchSettings({ fonts: { ...currentFonts, mono: v } });
        onFontChange?.('mono', v);
        toast(`代码字体已设置为: ${v || '系统默认'}`);
      },
    });

    const editorSizeSeg = Segmented({
      value: vault.settings.editor?.size || 'md',
      options: [
        { value: 'sm', label: '小 (12.5px)' },
        { value: 'md', label: '中 (13.5px)' },
        { value: 'lg', label: '大 (15px)' },
      ],
      onChange: (v) => {
        vault.patchSettings({ editor: { size: v } });
        document.documentElement.style.setProperty(
          '--ink-editor-fs',
          v === 'sm' ? '12.5px' : v === 'lg' ? '15px' : '13.5px'
        );
      },
    });

    const card = CardGroup({
      title: '字体与排版',
      children: [
        Row({
          label: '界面字体 (UI Font)',
          hint: '用于顶栏、侧边栏、标签页、按钮等系统控件',
          control: uiFontSelect,
        }),
        Row({
          label: 'Markdown 正文字体',
          hint: '用于 Markdown 笔记内容区及阅读排版',
          control: mdFontSelect,
        }),
        Row({
          label: '代码等宽字体 (Code / Mono)',
          hint: '用于代码块、行内代码、行号及 Frontmatter 属性',
          control: monoFontSelect,
        }),
        Row({
          label: '编辑器字号',
          hint: '调整主编辑器源码阅读字号',
          control: editorSizeSeg,
        }),
      ],
    });

    typographyCardContainer.replaceChildren(card);
  };

  // 初始化使用预设字体，随后异步加载完整系统字体
  buildTypographyCard(SYSTEM_DEFAULT_FONTS);

  // 异步获取本机系统字体
  (async () => {
    try {
      let sysFonts = [];
      if (vault.fs.systemFonts) {
        sysFonts = await vault.fs.systemFonts();
      } else if (window.queryLocalFonts) {
        const available = await window.queryLocalFonts();
        sysFonts = Array.from(new Set(available.map((f) => f.family))).sort();
      }

      if (sysFonts && sysFonts.length > 0) {
        const combined = Array.from(new Set([...sysFonts, ...SYSTEM_DEFAULT_FONTS])).sort((a, b) =>
          a.localeCompare(b, 'zh-Hans-CN')
        );
        buildTypographyCard(combined);
      }
    } catch (e) {
      console.warn('[settings] 获取本机字体列表降级使用预设', e);
    }
  })();

  // 编辑器卡片
  const modeSeg = Segmented({
    value: vault.workspace.mode || 'edit',
    options: [
      { value: 'edit', label: '纯编辑' },
      { value: 'split', label: '双栏分屏' },
      { value: 'preview', label: '仅预览' },
    ],
    onChange: (v) => {
      vault.patchWorkspace({ mode: v });
      const split = document.querySelector('.ink-split');
      if (split) split.dataset.mode = v;
      const docPane = document.getElementById('docPane');
      if (docPane) docPane.dataset.mode = v;
    },
  });

  const editorCard = CardGroup({
    title: '编辑器首选项',
    children: [
      Row({ label: '默认视图模式', hint: '打开笔记时的初始视图状态', control: modeSeg }),
    ],
  });

  // 关于卡片
  const aboutCard = CardGroup({
    title: '关于 Inkdown',
    children: [
      Row({
        label: '版本',
        hint: 'Tauri v2 + Golden Gate macOS 液态玻璃设计系统',
        control: h('span', { class: 'c-subtitle' }, 'v0.1.0'),
      }),
      Row({
        label: '存储模式',
        hint: '纯本地文件夹仓库 (File-over-app)，无厂商锁定',
        control: h('span', { class: 'c-subtitle' }, '本地优先'),
      }),
    ],
  });

  scrollBody.append(vaultCard, appearanceCard, typographyCardContainer, editorCard, aboutCard);

  return container;
}
