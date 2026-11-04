/* ==========================================================================
   Inkdown · 插件页
   整页视图（ViewHost 的 page 分支）：顶部是插件目录，中间是已启用 / 未启用两张卡，
   底部重新扫描。数据全来自插件宿主的快照，页面自己不改 plugins.json。

   开关是这里唯一的写入口：拨一下 → host.enable / disable → 宿主 notify → 重画。
   重画会换掉整段 DOM，所以拨动期间必须锁住开关，否则连点会打进两个 enable。
   ========================================================================== */

import { h, fill } from '../core/dom.js';
import { icon } from '../core/icons.js';
import { Badge, Button, CardGroup, Empty, Row, Switch, toast } from '../ui/kit.js';
import { describePermission } from '../plugins/permissions.js';

const PLUGIN_DIR_TITLE = '插件目录';

export function createPluginsView({ vault, host, onOpenInExplorer, container = null }) {
  const root = h('div', { class: 'c-plugin-page' });
  const body = h('div', { class: 'c-plugin-page__body l-stack l-stack--7' });
  root.append(body);
  container?.append(root);

  /** 展开详情的插件 id：重画后保持展开状态 */
  const expanded = new Set();
  let busyId = '';
  let scanning = false;
  let started = false;

  const unsubscribe = host.subscribe(() => render());

  async function rescan() {
    if (scanning) return;
    scanning = true;
    render();
    try {
      await host.scan();
    } catch (err) {
      toast(`扫描失败：${err?.message || err}`, { icon: 'x-circle', tone: 'danger' });
    } finally {
      scanning = false;
      render();
    }
  }

  async function setEnabled(record, next, sw) {
    if (busyId) {
      sw.set(record.enabled);
      return;
    }
    busyId = record.id;
    sw.dataset.busy = '1';
    sw.disabled = true;

    try {
      if (next) {
        const ok = await host.enable(record.id);
        // 只有真的出错才报：用户在授权弹窗里点「取消」是正常操作，不该弹红条
        const live = host.get(record.id);
        if (!ok && live?.error) {
          toast(`「${record.name}」未能启用：${live.error}`, {
            icon: 'x-circle',
            tone: 'danger',
            duration: 3200,
          });
        }
      } else {
        await host.disable(record.id);
      }
    } catch (err) {
      toast(String(err?.message || err), { icon: 'x-circle', tone: 'danger' });
    } finally {
      busyId = '';
      render();
    }
  }

  /* ---------------- 局部节点 ---------------- */

  function dirCard() {
    const isReal = Boolean(vault.isFolder && vault.rootPath);
    const dirText = isReal ? `${vault.rootPath}/.inkdown/plugins` : '.inkdown/plugins';

    const pathBox = h(
      'div',
      { class: 'c-plugin-path' },
      h('span', { class: 'c-plugin-path__text' }, dirText),
      isReal && onOpenInExplorer
        ? Button({
            label: '在资源管理器中打开',
            icon: 'folder-open',
            size: 'sm',
            onClick: () => onOpenInExplorer('.inkdown/plugins'),
          })
        : h('span', { class: 'c-plugin-path__hint' }, isReal ? '' : '虚拟仓库：插件存在浏览器本地')
    );

    return CardGroup({
      title: PLUGIN_DIR_TITLE,
      children: [
        Row({
          label: '存放位置',
          hint: '每个插件一个文件夹，内含 manifest.json 与 main.js；状态记在 .inkdown/plugins.json',
          stack: true,
          control: pathBox,
        }),
      ],
    });
  }

  function detailNode(record, detailHost, chev) {
    const open = expanded.has(record.id);
    detailHost.hidden = !open;
    chev.dataset.open = open ? '1' : '0';

    const permText = record.permissions.length
      ? record.permissions.map((p) => describePermission(p).label).join(' · ')
      : '不需要任何权限';

    fill(
      detailHost,
      h(
        'div',
        { class: 'c-plugin-detail__row' },
        h('span', { class: 'c-plugin-detail__key' }, '作者'),
        h('span', { class: 'c-plugin-detail__value' }, record.author || '未署名')
      ),
      h(
        'div',
        { class: 'c-plugin-detail__row' },
        h('span', { class: 'c-plugin-detail__key' }, '权限'),
        h('span', { class: 'c-plugin-detail__value' }, permText)
      ),
      h(
        'div',
        { class: 'c-plugin-detail__row' },
        h('span', { class: 'c-plugin-detail__key' }, '文件夹'),
        h('span', { class: 'c-plugin-detail__value c-plugin-detail__value--mono' }, record.dir)
      ),
      record.error ? h('div', { class: 'c-plugin-error' }, record.error) : null
    );
  }

  function itemNode(record) {
    const chev = h('span', { class: 'c-plugin-row__chev', html: icon('chevron-down', 12) });
    const detail = h('div', { class: 'c-plugin-detail' });
    detailNode(record, detail, chev);

    const toggle = () => {
      if (expanded.has(record.id)) expanded.delete(record.id);
      else expanded.add(record.id);
      detailNode(record, detail, chev);
    };

    const sw = Switch({
      checked: record.enabled,
      label: `${record.name} 启用开关`,
      onChange: (next) => setEnabled(record, next, sw),
    });
    if (busyId === record.id) {
      sw.dataset.busy = '1';
      sw.disabled = true;
    }

    return h(
      'div',
      { class: `c-plugin-item${record.enabled ? '' : ' is-off'}` },
      h(
        'div',
        { class: 'c-plugin-row' },
        h(
          'div',
          {
            class: 'c-plugin-row__main',
            role: 'button',
            tabindex: '0',
            'aria-expanded': String(expanded.has(record.id)),
            onclick: toggle,
            onkeydown: (e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                toggle();
              }
            },
          },
          h('span', { class: 'c-plugin-row__icon', html: icon('puzzle', 16) }),
          h(
            'div',
            { class: 'c-plugin-row__text' },
            h(
              'div',
              { class: 'c-plugin-row__title' },
              h('span', { class: 'c-plugin-row__name' }, record.name),
              record.version ? h('span', { class: 'c-plugin-row__ver' }, `v${record.version}`) : null,
              record.error ? Badge({ label: '加载失败', variant: 'danger' }) : null
            ),
            h(
              'div',
              { class: `c-plugin-row__desc${record.error ? ' is-error' : ''}` },
              record.error
                ? `加载失败：${record.error}`
                : record.description || '没有填写描述'
            )
          )
        ),
        h('div', { class: 'c-plugin-row__aside' }, sw, chev)
      ),
      detail
    );
  }

  function footNode(total) {
    return h(
      'div',
      { class: 'c-plugin-foot' },
      Button({
        label: scanning ? '正在扫描…' : '重新扫描',
        icon: 'refresh-cw',
        size: 'sm',
        disabled: scanning,
        onClick: () => rescan(),
      }),
      h(
        'span',
        { class: 'c-plugin-foot__hint' },
        total ? '新增或删除插件文件夹后，点这里让宿主重新读一遍' : '把插件文件夹放进 .inkdown/plugins/ 后点重新扫描'
      )
    );
  }

  /* ---------------- 整页 ---------------- */

  function render() {
    const scroll = root.scrollTop;
    const list = host.list();
    const enabled = list.filter((p) => p.enabled);
    const disabled = list.filter((p) => !p.enabled);

    const sections = [
      dirCard(),
      enabled.length
        ? CardGroup({ title: `已启用 · ${enabled.length}`, children: enabled.map(itemNode) })
        : null,
      disabled.length
        ? CardGroup({ title: `未启用 · ${disabled.length}`, children: disabled.map(itemNode) })
        : null,
      !list.length
        ? Empty({
            icon: 'puzzle',
            title: '还没有插件',
            desc: '把插件文件夹放进 .inkdown/plugins/ 后点重新扫描',
          })
        : null,
      footNode(list.length),
    ];

    fill(body, sections);
    root.scrollTop = scroll;
  }

  /** 首帧先出骨架，再让宿主读盘；宿主 notify 会自己触发重画 */
  function start() {
    if (started) return;
    started = true;
    render();
    rescan();
  }

  start();

  return {
    refresh: () => rescan(),
    destroy: () => unsubscribe(),
  };
}
