// Manual native integration runner. Requires an explicitly launched local debug
// WebView on 127.0.0.1:9222; never starts hardware tests automatically.
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
const root = path.resolve('artifacts/native-ui');
await mkdir(root, { recursive: true });
const targets = await (await fetch('http://127.0.0.1:9222/json/list')).json();
const target = targets.find(t => t.type === 'page' && /127\.0\.0\.1:1420|localhost:1420|tauri\.localhost/.test(t.url));
if (!target) throw new Error('Local NEXUS debug WebView not found.');
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
const pending = new Map(); let next = 0;
ws.onmessage = event => { const response = JSON.parse(String(event.data)); if (response.id) { const entry = pending.get(response.id); if (entry) { pending.delete(response.id); clearTimeout(entry.timer); response.error ? entry.reject(new Error(response.error.message)) : entry.resolve(response.result); } } };
function command(method, params = {}) { return new Promise((resolve, reject) => { const id = ++next; const timer = setTimeout(() => { pending.delete(id); reject(new Error(`${method}: timeout`)); }, 20000); pending.set(id, { resolve, reject, timer }); ws.send(JSON.stringify({ id, method, params })); }); }
async function evaluate(expression) { const result = await command('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }); if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text); return result.result.value; }
async function invoke(name, args) { return evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)},${JSON.stringify(args || {})})`); }
async function capture(name) { await command('Page.enable'); const result = await command('Page.captureScreenshot', { format: 'png', fromSurface: true }); await writeFile(path.join(root, `${name}.png`), Buffer.from(result.data, 'base64')); }
const delay = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
const summary = { at: new Date().toISOString(), kind: 'real-native-ui-integration', tests: [], screenshots: [] };
try {
 if (target.url === 'http://localhost:1420/') { await command('Page.navigate', { url: 'http://127.0.0.1:1420/' }); await delay(1000); }
 let current;
 for (let n = 0; n < 30; n++) { current = await invoke('current_snapshot'); if (current.snapshot?.ready) break; await delay(500); }
 if (!current?.snapshot?.ready) throw new Error('Collector did not provide a ready hardware snapshot.');
 summary.tests.push({ name: 'real-collector-snapshot', passed: true, components: current.snapshot.components.map(c => ({ kind: c.kind, metrics: c.metrics.length, available: c.metrics.filter(m => m.value !== null).length })) });
 const settings = await invoke('desktop_settings');
 summary.tests.push({ name: 'persistent-settings', passed: ['economy', 'balanced', 'detailed'].includes(settings.profile), profile: settings.profile });
 const diagnostics = await invoke('diagnostics_current');
 summary.tests.push({ name: 'diagnostics', passed: Array.isArray(diagnostics.findings), state: diagnostics.state, findings: diagnostics.findings.length, fresh: diagnostics.fresh });
 const now = Date.now();
 const cpu = current.snapshot.components.find(c => c.kind === 'cpu');
 const series = await invoke('history_query', { query: { componentId: cpu?.id, metricIds: ['load'], fromMs: now - 3600000, toMs: now, maxPoints: 300 } });
 summary.tests.push({ name: 'bounded-history-query', passed: Array.isArray(series) && series.every(s => s.points.length <= 300), series: series.length, points: series.reduce((sum, s) => sum + s.points.length, 0) });
 const timeline = await invoke('timeline_query', { query: { limit: 64 } });
 summary.tests.push({ name: 'timeline-query', passed: Array.isArray(timeline) && timeline.length <= 64, events: timeline.length });
 const benchmarks = await invoke('benchmark_history');
 summary.tests.push({ name: 'benchmark-history', passed: Array.isArray(benchmarks), results: benchmarks.length });
 const selectors = ['home', 'cpu', 'gpu', 'ram', 'storage', 'motherboard', 'network', 'diagnostics', 'reports', 'settings'];
 for (let i = 0; i < selectors.length; i++) {
   await evaluate(`document.querySelectorAll('nav[aria-label="Основная навигация"] button')[${i}].click()`);
   await delay(120);
   const state = await evaluate(`({heading: document.querySelector('main h1')?.textContent, horizontalOverflow: document.documentElement.scrollWidth > innerWidth, error: document.querySelector('vite-error-overlay') !== null, controls: document.querySelectorAll('main button').length})`);
   summary.tests.push({ name: `page-${selectors[i]}`, passed: !!state.heading && !state.horizontalOverflow && !state.error, ...state });
   if (['home', 'cpu', 'storage', 'diagnostics', 'reports'].includes(selectors[i])) { await capture(selectors[i]); summary.screenshots.push(`${selectors[i]}.png`); }
 }
 // Leave the app on overview, without changing persisted preferences or running tests.
 await evaluate(`document.querySelectorAll('nav[aria-label="Основная навигация"] button')[0].click()`);
 summary.passed = summary.tests.every(t => t.passed);
 await writeFile(path.join(root, 'integration.json'), JSON.stringify(summary, null, 2));
 console.log(JSON.stringify(summary, null, 2));
 if (!summary.passed) process.exitCode = 1;
} finally { ws.close(); }


