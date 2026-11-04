// 临时性能探针：无头 Chrome + CDP，不依赖 puppeteer
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const CHROME = 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const PORT = 9333;

export async function launch({ headed = false, scale = 1 } = {}) {
  const profile = mkdtempSync(join(tmpdir(), 'inkdown-perf-'));
  const args = [
    headed ? '--new-window' : '--headless=new',
    `--remote-debugging-port=${PORT}`,
    `--user-data-dir=${profile}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-extensions',
    '--hide-scrollbars',
    '--window-size=1440,900',
    '--window-position=60,60',
    `--force-device-scale-factor=${scale}`,
    '--use-angle=d3d11',
    '--enable-unsafe-swiftshader',
    '--disable-background-timer-throttling',
    '--disable-renderer-backgrounding',
    '--disable-backgrounding-occluded-windows',
    'about:blank',
  ];
  const proc = spawn(CHROME, args, { stdio: 'ignore' });

  let wsUrl = null;
  for (let i = 0; i < 100; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${PORT}/json/version`);
      wsUrl = (await r.json()).webSocketDebuggerUrl;
      break;
    } catch {
      await new Promise((r) => setTimeout(r, 150));
    }
  }
  if (!wsUrl) throw new Error('Chrome 没起来');

  const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
  const page = list.find((t) => t.type === 'page');
  const cdp = await connect(page.webSocketDebuggerUrl);

  const browser = await connect(wsUrl);
  return {
    cdp, browser,
    async close() {
      try { browser.send('Browser.close'); } catch {}
      await new Promise((r) => setTimeout(r, 300));
      try { proc.kill(); } catch {}
      try { rmSync(profile, { recursive: true, force: true }); } catch {}
    },
  };
}

export function connect(url) {
  const ws = new WebSocket(url);
  let id = 0;
  const pending = new Map();
  const listeners = new Set();

  ws.addEventListener('message', (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(JSON.stringify(msg.error)));
      else resolve(msg.result);
    } else if (msg.method) {
      for (const fn of listeners) fn(msg);
    }
  });

  const ready = new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', reject, { once: true });
  });

  const api = {
    ws,
    ready,
    send(method, params = {}) {
      const mid = ++id;
      return ready.then(() => new Promise((resolve, reject) => {
        pending.set(mid, { resolve, reject });
        ws.send(JSON.stringify({ id: mid, method, params }));
      }));
    },
    on(fn) { listeners.add(fn); return () => listeners.delete(fn); },
    waitFor(method, timeout = 30000) {
      return new Promise((resolve, reject) => {
        const t = setTimeout(() => { off(); reject(new Error(`等 ${method} 超时`)); }, timeout);
        const off = api.on((msg) => {
          if (msg.method === method) { clearTimeout(t); off(); resolve(msg.params); }
        });
      });
    },
    close() { try { ws.close(); } catch {} },
  };
  return api;
}

export async function evaluate(cdp, expression, { awaitPromise = true } = {}) {
  const r = await cdp.send('Runtime.evaluate', {
    expression,
    awaitPromise,
    returnByValue: true,
    userGesture: true,
  });
  if (r.exceptionDetails) {
    throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
  }
  return r.result.value;
}

export async function goto(cdp, url) {
  const loaded = cdp.waitFor('Page.loadEventFired');
  await cdp.send('Page.enable');
  await cdp.send('Page.navigate', { url });
  await loaded;
  // 等应用把玻璃挂完
  await new Promise((r) => setTimeout(r, 1200));
}
