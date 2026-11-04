/* ==========================================================================
   Inkdown · 插件权限表
   一份清单，三个地方共用：manifest 校验、授权弹窗、插件页详情。
   新增权限只改这里 —— 校验和文案不会再各写一遍。
   ========================================================================== */

export const PERMISSIONS = {
  'vault:read': {
    label: '读取仓库',
    desc: '读取笔记、附件和目录结构',
  },
  'vault:write': {
    label: '写入仓库',
    desc: '创建或修改仓库里的文件',
  },
  net: {
    label: '访问网络',
    desc: '向外部地址发起请求（可能把笔记内容发出去）',
  },
  clipboard: {
    label: '剪贴板',
    desc: '读取或写入系统剪贴板',
  },
};

export const PERMISSION_NAMES = Object.keys(PERMISSIONS);

/** 中文名 + 说明；未知权限原样返回，避免界面出现空白行 */
export function describePermission(name) {
  return PERMISSIONS[name] || { label: name, desc: '未知权限' };
}
