import { launch, goto, evaluate } from './cdp.mjs';

const h = await launch({ headed: true });
try {
  await goto(h.cdp, 'http://127.0.0.1:8899/index.html');
  const clip = await evaluate(h.cdp, `(() => {
    const el = document.querySelector('.c-drawer.ink-recent-drawer .c-drawer__grip');
    const b = el.getBoundingClientRect();
    return { x: b.left + b.width / 2, y: b.top + b.height / 2 };
  })()`);
  const before = await evaluate(h.cdp, `[...document.querySelectorAll('[data-glass]')].map(e => e.style.getPropertyValue('backdrop-filter'))`);
  await h.cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: clip.x, y: clip.y, button: 'left', buttons: 1, clickCount: 1 });
  await new Promise((r) => setTimeout(r, 150));
  const during = await evaluate(h.cdp, `({
    lite: document.documentElement.dataset.glassLite,
    filters: [...document.querySelectorAll('[data-glass]')].map(e => e.style.getPropertyValue('backdrop-filter')),
  })`);
  await h.cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: clip.x, y: clip.y, button: 'left', buttons: 0, clickCount: 1 });
  await new Promise((r) => setTimeout(r, 400));
  const after = await evaluate(h.cdp, `({
    lite: document.documentElement.dataset.glassLite || null,
    filters: [...document.querySelectorAll('[data-glass]')].map(e => e.style.getPropertyValue('backdrop-filter')),
    count: document.querySelectorAll('[data-glass]').length,
  })`);
  console.log(JSON.stringify({
    beforeCount: before.length,
    beforeUrl: before.filter((v) => v.includes('url(')).length,
    duringLite: during.lite,
    duringUrl: during.filters.filter((v) => v.includes('url(')).length,
    duringBlurSat: during.filters.filter((v) => /blur\(.+saturate\(/.test(v)).length,
    afterLite: after.lite,
    afterCount: after.count,
    afterUrl: after.filters.filter((v) => v.includes('url(')).length,
    afterSameAsBefore: JSON.stringify(after.filters) === JSON.stringify(before),
  }, null, 2));
} finally { await h.close(); }
