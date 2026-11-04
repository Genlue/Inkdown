// 性能基准：node tools/perf/measure.mjs <标签> [headed] [DPR]
// 注意：glassPerf 字段需要临时在 glass-lens.js 里埋 perfMark 才有数，
// 交付版本已移除埋点，帧时间/长任务/重建次数仍然有效。
import { writeFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { launch, goto, evaluate } from './cdp.mjs';

const LABEL = process.argv[2] || 'run';
const HEADED = process.argv[3] === 'headed';
const SCALE = Number(process.argv[4] || 1);
const OUT = fileURLToPath(new URL('./out/', import.meta.url));
mkdirSync(OUT, { recursive: true });

const BASE = process.env.PERF_URL || 'http://127.0.0.1:8899/index.html';

const HELPERS = `(() => {
  window.__glassPerf = {
    counts: {}, ms: {},
    reset() { this.counts = {}; this.ms = {}; },
    record(n, ms) {
      this.counts[n] = (this.counts[n] || 0) + 1;
      this.ms[n] = (this.ms[n] || 0) + ms;
    },
    dump() {
      const out = {};
      for (const k of Object.keys(this.counts)) out[k] = { n: this.counts[k], ms: +this.ms[k].toFixed(2), avg: +(this.ms[k] / this.counts[k]).toFixed(3) };
      return out;
    },
  };

  window.__probe = {
    state: null,
    reset() {
      this.state = { frames: [], tail: [], longtasks: [], attrs: 0, childAdds: 0, liteFrames: 0 };
      window.__glassPerf.reset();
      this.mo?.disconnect();
      this.lo?.disconnect();
      const st = this.state;
      this.mo = new MutationObserver((recs) => {
        for (const r of recs) {
          if (r.type === 'attributes' && r.attributeName === 'href') st.attrs++;
          if (r.type === 'childList' && r.target?.nodeName === 'filter') st.childAdds += r.addedNodes.length;
        }
      });
      this.mo.observe(document.body, { subtree: true, attributes: true, attributeFilter: ['href'], childList: true });
      this.lo = new PerformanceObserver((list) => {
        for (const e of list.getEntries()) st.longtasks.push(+e.duration.toFixed(1));
      });
      this.lo.observe({ entryTypes: ['longtask'] });
    },
    _loop(n, tick) {
      return new Promise((resolve) => {
        const st = this.state;
        let prev = 0, i = 0;
        const step = (now) => {
          if (prev) st.frames.push(+(now - prev).toFixed(2));
          prev = now;
          if (i >= n) { resolve(); return; }
          tick(i, n);
          if (document.documentElement.dataset.glassLite === '1') st.liteFrames++;
          i++;
          requestAnimationFrame(step);
        };
        requestAnimationFrame(step);
      });
    },
    async tail() {
      const st = this.state;
      const arr = [];
      let prev = 0, i = 0;
      await new Promise((resolve) => {
        const step = (now) => {
          if (prev) arr.push(+(now - prev).toFixed(2));
          prev = now;
          if (++i > 5) { resolve(); return; }
          requestAnimationFrame(step);
        };
        requestAnimationFrame(step);
      });
      st.tail = arr;
    },

    async drain() {
      await new Promise((r) => setTimeout(r, 250));
      this.mo?.disconnect();
      this.lo?.disconnect();
      return this.state;
    },

    async drawer(n = 60) {
      const el = document.querySelector('.c-drawer.ink-recent-drawer');
      el.style.setProperty('--dw-p', '0');
      // 预热：让材质和合成层先就位，不要把冷启动算进拖动
      await this._loop(20, (i, m) => el.style.setProperty('--dw-p', String((i / m) * 0.3)));
      el.style.setProperty('--dw-p', '0');
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      this.reset();
      await this._loop(n, (i, m) => el.style.setProperty('--dw-p', String(i / (m - 1))));
    },

    async drawerResize(n = 60) {
      const drawer = document.querySelector('.c-drawer.ink-recent-drawer');
      const el = drawer.querySelector('.c-drawer__sheet');
      drawer.style.setProperty('--dw-p', '1');
      await new Promise((r) => setTimeout(r, 400));
      const from = el.getBoundingClientRect().height;
      await this._loop(8, () => {});
      this.reset();
      await this._loop(n, (i, m) => drawer.style.setProperty('--dw-size', (from - 120 * (i / (m - 1))).toFixed(1) + 'px'));
    },

    async drawerWide(n = 60) {
      document.documentElement.style.setProperty('--ink-list-w', '520px');
      await new Promise((r) => setTimeout(r, 400));
      await this.drawer(n);
    },

    async idle(n = 60) {
      await this._loop(10, () => {});
      this.reset();
      await this._loop(n, () => {});
    },

    async width(n = 60) {
      const pane = document.getElementById('listPane');
      const from = pane.getBoundingClientRect().width;
      const to = from + 136;
      await this._loop(10, () => {});
      this.reset();
      await this._loop(n, (i, m) => document.documentElement.style.setProperty('--ink-list-w', (from + (to - from) * (i / (m - 1))).toFixed(2) + 'px'));
      document.documentElement.style.setProperty('--ink-list-w', to.toFixed(2) + 'px');
    },

    async menu(n = 120) {
      const btn = [...document.querySelectorAll('button')].find((b) => b.getAttribute('aria-label') === '更多操作');
      const open = () => btn.click();
      const close = () => document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      await this._loop(10, () => {});
      this.reset();
      await this._loop(n, (i, m) => {
        if (i === 0) open();
        if (i === Math.floor(m * 0.4)) close();
        if (i === Math.floor(m * 0.8)) open();
      });
      close();
    },
  };
})()`;

function stats(frames) {
  if (!frames.length) return {};
  const s = [...frames].sort((a, b) => a - b);
  const q = (p) => s[Math.min(s.length - 1, Math.floor(p * s.length))];
  const sum = s.reduce((a, b) => a + b, 0);
  return {
    n: s.length,
    p50: +q(0.5).toFixed(2),
    p95: +q(0.95).toFixed(2),
    max: +s[s.length - 1].toFixed(2),
    mean: +(sum / s.length).toFixed(2),
    dropped: s.filter((v) => v > 33.4).length,
  };
}

function aggregateProfile(profile) {
  const byId = new Map(profile.nodes.map((n) => [n.id, n]));
  const self = new Map();
  for (let i = 0; i < profile.samples.length; i++) {
    const id = profile.samples[i];
    const dt = profile.timeDeltas[i] || 0;
    self.set(id, (self.get(id) || 0) + dt);
  }
  const rows = [];
  for (const [id, us] of self) {
    const n = byId.get(id);
    if (!n) continue;
    const cf = n.callFrame;
    rows.push({
      fn: cf.functionName || '(anon)',
      url: (cf.url || '').split('/').pop(),
      line: cf.lineNumber + 1,
      ms: +(us / 1000).toFixed(2),
    });
  }
  const merged = new Map();
  for (const r of rows) {
    const k = `${r.fn} @ ${r.url}:${r.line}`;
    const m = merged.get(k) || { ...r, ms: 0 };
    m.ms += r.ms;
    merged.set(k, m);
  }
  return [...merged.values()].sort((a, b) => b.ms - a.ms).slice(0, 14);
}

async function pressAt(h, selector) {
  const r = await evaluate(h.cdp, `(() => {
    const el = document.querySelector(${JSON.stringify(selector)});
    if (!el) return null;
    const b = el.getBoundingClientRect();
    return { x: b.left + b.width / 2, y: b.top + b.height / 2 };
  })()`);
  if (!r) throw new Error('找不到交互元素 ' + selector);
  await h.cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: r.x, y: r.y, button: 'left', buttons: 1, clickCount: 1 });
  await new Promise((res) => setTimeout(res, 60));
  return r;
}

async function releaseAt(h, p, dx = 0) {
  // 先补一个真实 pointermove，让控件自己算出最终位移；
  // 否则 onUp 会按 delta=0 把刚拖出来的宽度写回去。
  if (dx) {
    await h.cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: p.x + dx, y: p.y, button: 'none', buttons: 1 });
  }
  await h.cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: p.x + dx, y: p.y, button: 'left', buttons: 0, clickCount: 1 });
}

async function scenario(h, name, { url = BASE, action, press = null, prepare = null, profile = true } = {}) {
  await goto(h.cdp, url);
  await evaluate(h.cdp, HELPERS);
  if (prepare) await evaluate(h.cdp, prepare);
  let pt = null;
  if (press) pt = await pressAt(h, press);
  await evaluate(h.cdp, `window.__probe.reset()`);
  if (profile) {
    await h.cdp.send('Profiler.enable');
    await h.cdp.send('Profiler.setSamplingInterval', { interval: 200 });
    await h.cdp.send('Profiler.start');
  }
  await evaluate(h.cdp, `window.__probe.${action}()`);
  if (pt) await releaseAt(h, pt, action === 'width' ? 136 : 0);
  await evaluate(h.cdp, `window.__probe.tail()`);
  let prof = null;
  if (profile) {
    prof = await h.cdp.send('Profiler.stop');
    await h.cdp.send('Profiler.disable');
  }
  const state = await evaluate(h.cdp, `window.__probe.drain()`);
  const gp = await evaluate(h.cdp, `window.__glassPerf.dump()`);

  const result = {
    name,
    frames: stats(state.frames),
    longTasks: { count: state.longtasks.length, totalMs: +state.longtasks.reduce((a, b) => a + b, 0).toFixed(1), maxMs: state.longtasks.length ? Math.max(...state.longtasks) : 0 },
    tail: state.tail,
    liteFrames: state.liteFrames,
    rebuildAttrs: state.attrs,
    filterNodesAdded: state.childAdds,
    glassPerf: gp,
    top: prof ? aggregateProfile(prof.profile) : [],
  };
  console.log(`\n=== ${name} ===`);
  console.log(JSON.stringify(result.frames), 'tail', JSON.stringify(result.tail), 'liteFrames', result.liteFrames, 'longtasks', JSON.stringify(result.longTasks), 'attrs', result.rebuildAttrs, 'glassPerf', JSON.stringify(gp));
  return result;
}

const h = await launch({ headed: HEADED, scale: SCALE });
const results = {};
try {
  // 抽屉用真实默认状态；fallback 版本用来标定「没有 SVG 滤镜」的底噪
  const GRIP = '.c-drawer.ink-recent-drawer .c-drawer__grip';
  const LIST_HANDLE = '.ink-resizer--list';
  results.drawer = await scenario(h, '抽屉拖动', { action: 'drawer', press: GRIP });
  results.drawerWide = await scenario(h, '抽屉拖动(列表520宽)', { action: 'drawerWide', press: GRIP });
  results.drawerResize = await scenario(h, '抽屉改高度(已展开)', { action: 'drawerResize', press: GRIP });
  results.width = await scenario(h, '栏宽拖动', { action: 'width', press: LIST_HANDLE });
  results.menu = await scenario(h, '菜单开合', { action: 'menu' });
  results.drawerNoFilter = await scenario(h, '抽屉(无SVG滤镜,?fallback=1)', { action: 'drawer', url: `${BASE}?fallback=1` });
  results.idle = await scenario(h, '静止对照', { action: 'idle' });
} finally {
  await h.close();
}

writeFileSync(`${OUT}${LABEL}.json`, JSON.stringify(results, null, 2));
console.log(`\n已写入 tools/perf/out/${LABEL}.json`);
