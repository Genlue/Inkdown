import { launch, goto, evaluate } from './cdp.mjs';

const h = await launch();
try {
  await goto(h.cdp, 'http://127.0.0.1:8899/index.html');
  const out = await evaluate(h.cdp, `(async () => {
    const optics = await import('./scripts/core/glass-optics.js');
    const shapeMod = await import('./scripts/core/glass-shape.js');
    const schema = await import('./scripts/core/glass-schema.js');
    const p = schema.normalizeGlassOptics(schema.glassSurfaceDefaults('sheet'));
    const time = (fn) => { const t = performance.now(); fn(); return +(performance.now() - t).toFixed(1); };
    const run = (w, h, radii) => {
      const uniform = radii.every((r) => Math.abs(r - radii[0]) < 0.5);
      const shape = {
        w, h, radius: radii[0],
        polygon: uniform ? null : shapeMod.roundedRectPolygon(w, h, radii),
      };
      const [dw, dh] = optics.glassMapSize(shape, 512);
      const [hw, hh] = optics.glassMapSize(shape, 320);
      return {
        css: [w, h], radii, verts: shape.polygon ? shape.polygon.length : 0, maps: [[dw, dh], [hw, hh]],
        disp: [time(() => optics.glassDisplacementField(shape, p, dw, dh)), time(() => optics.glassDisplacementField(shape, p, dw, dh))],
        hl: [time(() => optics.glassHighlightField(shape, p, hw, hh)), time(() => optics.glassHighlightField(shape, p, hw, hh))],
      };
    };
    const uniform = (w, h) => ({ w, h, radius: 16 });
    const t0 = performance.now();
    optics.glassDisplacementField(uniform(284, 486), p, ...optics.glassMapSize(uniform(284, 486), 512));
    const cold = +(performance.now() - t0).toFixed(1);
    return {
      cold,
      drawer: run(284, 486, [16, 16, 0, 0]),
      drawerWide: run(520, 486, [16, 16, 0, 0]),
      drawerUniformForCompare: run(284, 486, [16, 16, 16, 16]),
    };
  })()`);
  console.log(JSON.stringify(out, null, 2));
} finally { await h.close(); }
