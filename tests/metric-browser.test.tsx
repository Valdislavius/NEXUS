import { afterEach, expect, test, vi } from 'vitest';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { HistoryChart } from '../src/HistoryChart';
import type { Component } from '../src/model';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }));
afterEach(() => { cleanup(); localStorage.clear(); vi.mocked(invoke).mockReset(); });
const at = new Date().toISOString();
const component: Component = { id: 'many-sensors', kind: 'cpu', name: 'Fixture CPU', subtitle: '', specs: [], metrics: Array.from({ length: 100 }, (_, i) => ({ id: `thread-${i}`, label: `Датчик ${i}`, value: i, unit: '%', source: 'fixture', primary: i === 0, at })) };

test('pagination reaches metrics beyond the old 48-row truncation and queries their real IDs', async () => {
 vi.mocked(invoke).mockResolvedValue([]);
 const view = render(<HistoryChart component={component} at={at} enabled />);
 expect(view.getAllByRole('checkbox').length).toBeLessThanOrEqual(25);
 fireEvent.click(view.getByRole('button', { name: 'Следующие датчики' }));
 fireEvent.click(view.getByRole('button', { name: 'Следующие датчики' }));
 fireEvent.click(view.getByRole('checkbox', { name: 'Датчик 55%' }));
 await waitFor(() => expect(invoke).toHaveBeenLastCalledWith('history_query', { query: expect.objectContaining({ metricIds: ['thread-0', 'thread-55'], maxPoints: 300 }) }));
 expect(view.getByText('Страница 3 из 5')).toBeVisible();
});

test('search reaches every sensor without mounting all metric controls', async () => {
 vi.mocked(invoke).mockResolvedValue([]);
 const view = render(<HistoryChart component={component} at={at} enabled />);
 fireEvent.change(view.getByRole('searchbox', { name: 'Поиск датчика' }), { target: { value: 'thread-99' } });
 fireEvent.click(view.getByRole('checkbox', { name: 'Датчик 99%' }));
 await waitFor(() => expect(invoke).toHaveBeenLastCalledWith('history_query', { query: expect.objectContaining({ metricIds: ['thread-0', 'thread-99'] }) }));
 expect(view.getAllByRole('checkbox')).toHaveLength(2);
 fireEvent.change(view.getByRole('searchbox', { name: 'Поиск датчика' }), { target: { value: 'no matching fixture' } });
 expect(view.getByText('По этому запросу датчики не найдены.')).toBeVisible();
});

test('the metric browser bounds selected series to eight', async () => {
 vi.mocked(invoke).mockResolvedValue([]);
 const view = render(<HistoryChart component={component} at={at} enabled />);
 for(let i = 1; i < 8; i++) fireEvent.click(view.getByRole('checkbox', { name: `Датчик ${i}%` }));
 expect(view.getByRole('checkbox', { name: 'Датчик 8%' })).toBeDisabled();
 await waitFor(() => expect(invoke).toHaveBeenLastCalledWith('history_query', { query: expect.objectContaining({ metricIds: Array.from({length:8},(_,i)=>`thread-${i}`) }) }));
});
