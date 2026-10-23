import { afterEach, expect, test, vi } from 'vitest';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { Benchmarks } from '../src/Tools';
import type { BenchmarkResult } from '../src/desktop';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }));
afterEach(() => { cleanup(); vi.mocked(invoke).mockReset(); });
const running: BenchmarkResult = { id: 'test-cpu', kind: 'cpu_single', status: 'running', startedAtMs: 1, progress: 10, details: 'Bounded test' };
const completed: BenchmarkResult = { ...running, status: 'completed', finishedAtMs: 2, progress: 100, score: 42, unit: 'ops/s' };

test('a failed history query cannot hide a running test or its cancellation', async () => {
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'benchmark_status') return running;
    if (command === 'benchmark_history') throw new Error('SQLite unavailable');
    return true;
  });
  const view = render(<Benchmarks enabled />);
  const cancel = await view.findByRole('button', { name: 'Остановить', exact: true });
  expect(await view.findByRole('alert')).toHaveTextContent('История проверок временно недоступна.');
  fireEvent.click(cancel);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('benchmark_cancel', undefined));
});

test('a failed current status is reported even when the history remains readable', async () => {
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'benchmark_status') throw new Error('Status unavailable');
    return [completed];
  });
  const view = render(<Benchmarks enabled />);
  expect(await view.findByRole('alert')).toHaveTextContent('Не удалось получить состояние теста.');
  expect(await view.findByText('История проверок · 1')).toBeVisible();
});

test('history failures after completion are handled without an unhandled rejection', async () => {
  let finished = false, statusCalls = 0;
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'benchmark_status') { finished = ++statusCalls > 1; return finished ? completed : running; }
    if (command === 'benchmark_history') { if (finished) throw new Error('SQLite busy'); return []; }
    return null;
  });
  const view = render(<Benchmarks enabled />);
  await view.findByRole('button', { name: 'Остановить', exact: true });
  await waitFor(() => expect(view.queryByRole('button', { name: 'Остановить', exact: true })).toBeNull());
  expect(await view.findByRole('alert')).toHaveTextContent('История проверок временно недоступна.');
});

test('completion refreshes history even though the active polling effect stops', async () => {
  let finished = false, statusCalls = 0;
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'benchmark_status') { finished = ++statusCalls > 1; return finished ? completed : running; }
    if (command === 'benchmark_history') return finished ? [completed] : [];
    return null;
  });
  const view = render(<Benchmarks enabled />);
  await view.findByRole('button', { name: 'Остановить', exact: true });
  expect(await view.findByText('История проверок · 1')).toBeVisible();
  expect(view.queryByRole('button', { name: 'Остановить', exact: true })).toBeNull();
});
