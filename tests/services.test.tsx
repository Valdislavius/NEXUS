import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { App } from '../src/App';
import { defaultSettings } from '../src/desktop';
import { useMonitor } from '../src/useMonitor';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => vi.fn()) }));
vi.mock('../src/useMonitor', () => ({ useMonitor: vi.fn() }));
const at = new Date().toISOString();
const snapshot = { version: 1 as const, type: 'snapshot' as const, at, host: 'fixture', platform: 'windows', ready: true, warnings: [], components: [] };
const monitor = { snapshot, error: '', interval: 2000, changeInterval: vi.fn(), restart: vi.fn(), restarting: false, stale: false };
const normal = { atMs: Date.now(), state: 'normal', findings: [], availableSensors: 1, totalSensors: 1, fresh: true };

beforeEach(() => {
 vi.mocked(useMonitor).mockReturnValue(monitor);
 vi.mocked(invoke).mockImplementation(async command => command === 'desktop_settings' ? defaultSettings : command === 'diagnostics_current' ? normal : command === 'benchmark_status' ? null : []);
});
afterEach(() => { cleanup(); vi.resetAllMocks(); });

test('a timeline storage failure cannot discard fresh critical diagnostics', async () => {
 const finding = { id: 'hot', componentId: 'cpu', componentKind: 'cpu', severity: 'critical', title: 'Порог превышен', atMs: Date.now(), actual: { value: 95, unit: '°C', threshold: 90 }, reason: 'Синтетическая проверка', explanations: [], actions: [] };
 vi.mocked(invoke).mockImplementation(async command => {
  if(command === 'timeline_query') throw new Error('SQLite unavailable');
  if(command === 'diagnostics_current') return { ...normal, state: 'critical', findings: [finding] };
  return command === 'desktop_settings' ? defaultSettings : command === 'benchmark_status' ? null : [];
 });
 const view = render(<App />);
 await waitFor(() => expect(view.getByText('1 предупреждений')).toBeVisible());
 fireEvent.click(view.getByRole('button', { name: 'Диагностика и тесты', exact: true }));
 expect(view.getByRole('heading', { name: 'Критическое событие' })).toBeVisible();
 expect(view.getByText('Порог превышен')).toBeVisible();
});

test('a diagnostic request failure cannot leave a previous normal state marked fresh', async () => {
 const view = render(<App />);
 await view.findByText('Показатели в норме');
 vi.mocked(invoke).mockImplementation(async command => {
  if(command === 'diagnostics_current') throw new Error('Diagnostics unavailable');
  return command === 'desktop_settings' ? defaultSettings : [];
 });
 vi.mocked(useMonitor).mockReturnValue({ ...monitor, snapshot: { ...snapshot, at: new Date(Date.parse(at) + 1000).toISOString() } });
 view.rerender(<App />);
 await view.findByText('Недостаточно свежих данных');
 expect(view.queryByText('Показатели в норме')).toBeNull();
});
