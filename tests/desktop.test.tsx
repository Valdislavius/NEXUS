import { afterEach, expect, test, vi } from 'vitest';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { boundedPoints, stats, HistoryChart } from '../src/HistoryChart';
import { Reports, Benchmarks } from '../src/Tools';
import { AppView } from '../src/App';
import { invalidateExpiredMetrics } from '../src/model';
import type { Component, Snapshot } from '../src/model';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
afterEach(() => { cleanup(); vi.mocked(invoke).mockReset(); vi.mocked(isTauri).mockReturnValue(true); });
const at = new Date().toISOString();
const component: Component = { id: 'cpu', kind: 'cpu', name: 'Real CPU', subtitle: '', specs: [], metrics: [{ id: 'load', label: 'Загрузка', value: 20, unit: '%', source: 'test-provider', primary: true, at }, { id: 'temperature', label: 'Температура', value: null, unit: '°C', source: 'test-provider', primary: true, at }] };
const snapshot: Snapshot = { version: 1, type: 'snapshot', at, host: 'local', platform: 'windows', ready: true, warnings: [], components: [component] };
test('chart bounds arbitrarily large history while preserving endpoints', () => { const points = Array.from({ length: 100000 }, (_, i) => i); const result = boundedPoints(points); expect(result).toHaveLength(300); expect(result[0]).toBe(0); expect(result.at(-1)).toBe(99999); expect(boundedPoints([1, 2])).toEqual([1, 2]); });
test('statistics exclude missing data and weight aggregated samples', () => { expect(stats([{ atMs: 0, value: null, min: null, max: null, count: 0 }])).toEqual({ min: null, max: null, avg: null }); expect(stats([{ atMs: 1, value: 10, min: 8, max: 12, count: 3 }, { atMs: 2, value: 30, min: 20, max: 40, count: 1 }])).toEqual({ min: 8, max: 40, avg: 15 }); });
test('expired cached readings are cleared even inside a fresh snapshot', () => { const now = Date.now(); const input = { ...snapshot, components: [{ ...component, metrics: [{ ...component.metrics[0], at: new Date(now - 70000).toISOString(), maxAgeMs: 60000 }] }] }; const result = invalidateExpiredMetrics(input, now); expect(result.components[0].metrics[0].value).toBeNull(); expect(result.components[0].metrics[0].diagnostic?.code).toBe('SENSOR_READING_EXPIRED'); expect(input.components[0].metrics[0].value).toBe(20); });
test('all ten desktop pages navigate without extra hardware requests', () => { const view = render(<AppView monitor={{ snapshot, error: '', interval: 2000, changeInterval: vi.fn(), restart: vi.fn(), restarting: false, stale: false }} />); const nav = view.getByRole('navigation', { name: 'Основная навигация' }); expect(nav.querySelectorAll('button')).toHaveLength(10); fireEvent.click(view.getByRole('button', { name: 'Процессор', exact: true })); expect(view.getByRole('navigation', { name: 'Разделы компонента' })).toBeVisible(); expect(view.getByText('Недоступно', { selector: 'strong' })).toBeVisible(); fireEvent.click(view.getByRole('button', { name: 'Графики и история' })); expect(view.getByRole('region', { name: 'Графики и история' })).toBeVisible(); expect(invoke).not.toHaveBeenCalled(); });
test('chart requests a bounded selected time range only', async () => { vi.mocked(invoke).mockResolvedValue([]); render(<HistoryChart component={component} at={at} enabled />); await waitFor(() => expect(invoke).toHaveBeenCalledWith('history_query', { query: expect.objectContaining({ componentId: 'cpu', metricIds: ['load'], maxPoints: 300 }) })); });
test('report defaults to anonymized export and explicit component selection', async () => { vi.mocked(invoke).mockResolvedValue({ path: 'NEXUS/report.xlsx', rows: 5 }); const view = render(<Reports components={[component]} enabled />); fireEvent.click(view.getByRole('button', { name: 'Создать отчёт' })); await waitFor(() => expect(invoke).toHaveBeenCalledWith('report_generate', { request: expect.objectContaining({ format: 'xlsx', components: ['cpu'], anonymized: true }) })); expect(await view.findByText('Отчёт сохранён · 5 строк')).toBeVisible(); });
test('benchmark never starts on mount or first confirmation click', async () => { vi.mocked(invoke).mockImplementation(async command => command === 'benchmark_history' ? [] : null); const view = render(<Benchmarks componentKind="cpu" enabled />); await waitFor(() => expect(invoke).toHaveBeenCalledWith('benchmark_status', undefined)); expect(vi.mocked(invoke).mock.calls.some(([name]) => name === 'benchmark_start')).toBe(false); fireEvent.click(view.getAllByRole('button', { name: 'Начать проверку' })[0]); expect(view.getByRole('button', { name: 'Запустить на 5 секунд' })).toBeVisible(); expect(vi.mocked(invoke).mock.calls.some(([name]) => name === 'benchmark_start')).toBe(false); });

test('structured diagnostic observations render actual units and thresholds safely', async () => {
 const { Findings } = await import('../src/Tools');
 const view = render(<Findings findings={[{ id: 'temperature', componentId: 'cpu', componentKind: 'cpu', severity: 'warning', title: 'Повышенная температура', atMs: Date.now(), actual: { value: 91, unit: '°C', threshold: 90 }, reason: 'Порог превышен', explanations: ['Высокая нагрузка'], actions: ['Проверить охлаждение'] }]} />);
 fireEvent.click(view.getByText('Повышенная температура')); expect(view.getByText(/91 °C/)).toBeVisible(); expect(view.getByText(/Порог 90 °C/)).toBeVisible();
});

test('chart zero-point budget never returns the full history', () => { expect(boundedPoints([1, 2, 3], 0)).toEqual([]); expect(boundedPoints([1, 2, 3], 1)).toEqual([3]); });

test('stale transport cannot keep diagnostics presented as a fresh normal state', () => {
 const view = render(<AppView monitor={{ snapshot, error: '', interval: 2000, changeInterval: vi.fn(), restart: vi.fn(), restarting: false, stale: true }} diagnostics={{ atMs: Date.now(), state: 'normal', findings: [], availableSensors: 1, totalSensors: 2, fresh: true }} />);
 expect(view.getByText('Недостаточно свежих данных')).toBeVisible(); fireEvent.click(view.getByRole('button', { name: 'Диагностика и тесты', exact: true })); expect(view.getByRole('heading', { name: 'Недостаточно данных', exact: true })).toBeVisible(); expect(view.queryByText('СВЕЖИЕ ДАННЫЕ')).toBeNull();
});

test('historical provider unit changes render on separate charts', async () => {
 const tempComponent = { ...component, metrics: [component.metrics[1]] };
 vi.mocked(invoke).mockResolvedValue([{ componentId: 'cpu', metricId: 'temperature', unit: '°C', source: 'provider-A', points: [], min: null, max: null, avg: null }, { componentId: 'cpu', metricId: 'temperature', unit: 'K', source: 'provider-B', points: [], min: null, max: null, avg: null }]);
 const view = render(<HistoryChart component={tempComponent} at={at} enabled />);
 expect(await view.findByRole('img', { name: 'История, °C' })).toBeVisible(); expect(view.getByRole('img', { name: 'История, K' })).toBeVisible(); expect(view.getByText('· provider-A')).toBeVisible(); expect(view.getByText('· provider-B')).toBeVisible();
});

test('collapsed overview cards do not mount detailed sensor grids', async () => {
 const { Card } = await import('../src/Card');
 const view = render(<Card component={{ ...component, metrics: [...component.metrics, { ...component.metrics[0], id: 'thread-0', label: 'Поток 00', primary: false }] }} />);
 expect(view.queryByText('Загрузка потоков')).toBeNull(); fireEvent.click(view.getByRole('button', { expanded: false })); expect(view.getByRole('heading', { name: /Загрузка потоков/ })).toBeVisible();
});
