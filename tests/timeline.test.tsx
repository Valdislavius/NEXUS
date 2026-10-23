import { afterEach, expect, test, vi } from 'vitest';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { AppView } from '../src/App';
import { HistoryChart } from '../src/HistoryChart';
import type { Component, Snapshot } from '../src/model';
import type { TimelineEvent } from '../src/desktop';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => vi.fn()) }));
afterEach(() => { cleanup(); localStorage.clear(); vi.mocked(invoke).mockReset(); });
const at = new Date().toISOString(), eventAt = Date.now() - 3600000;
const disk: Component = { id: 'disk-b', kind: 'storage', name: 'Disk B fixture', subtitle: '', specs: [], metrics: [{ id: 'load', label: 'Активность', value: 10, unit: '%', source: 'fixture', primary: true, at }, { id: 'smart-warning', label: 'SMART', value: 1, unit: '', source: 'fixture', primary: false, at }] };
const event: TimelineEvent = { id: `diag:health:disk-b.smart-warning:${eventAt}`, atMs: eventAt, source: 'diagnostics', componentId: 'disk-b', severity: 'warning', title: 'Событие диска fixture', details: 'Synthetic event', data: {} };
const snapshot: Snapshot = { version: 1, type: 'snapshot', at, host: 'fixture', platform: 'windows', ready: true, warnings: [], components: [{ ...disk, id: 'disk-a', name: 'Disk A fixture' }, disk] };
const monitor = { snapshot, error: '', interval: 2000, changeInterval: vi.fn(), restart: vi.fn(), restarting: false, stale: false };

test('a Timeline link opens the exact device, triggering metric and bounded event period', async () => {
 vi.mocked(invoke).mockResolvedValue([]);
 const view = render(<AppView monitor={monitor} servicesEnabled events={[event]} />);
 fireEvent.click(view.getByRole('button', { name: 'Открыть данные: ' + event.title }));
 expect(view.getByRole('button', { name: 'Накопители', exact: true })).toHaveAttribute('aria-current', 'page');
 expect(view.getByRole('button', { name: 'Disk B fixture', exact: true })).toHaveClass('active');
 expect(view.getByRole('button', { name: 'Графики и история', exact: true })).toHaveClass('active');
 await waitFor(() => expect(invoke).toHaveBeenLastCalledWith('history_query', { query: expect.objectContaining({ componentId: 'disk-b', metricIds: ['smart-warning'], fromMs: eventAt - 150000, toMs: eventAt + 150000, maxPoints: 300 }) }));
 expect(view.getByRole('button', { name: 'Live', exact: true })).toHaveAttribute('aria-pressed', 'false');
 expect(view.getByRole('status')).toHaveTextContent(event.title);
 expect(view.container.querySelector('.event-line.selected')).not.toBeNull();
});

test('ordinary page navigation clears a previous event focus', async () => {
 vi.mocked(invoke).mockResolvedValue([]);
 const view = render(<AppView monitor={monitor} servicesEnabled events={[event]} />);
 fireEvent.click(view.getByRole('button', { name: 'Открыть данные: ' + event.title }));
 await view.findByRole('region', { name: 'Графики и история' });
 fireEvent.click(view.getByRole('button', { name: 'Настройки', exact: true }));
 fireEvent.click(view.getByRole('button', { name: 'Накопители', exact: true }));
 expect(view.getByRole('button', { name: 'Показатели и характеристики' })).toHaveClass('active');
 expect(view.queryByRole('region', { name: 'Графики и история' })).toBeNull();
});

test('events for missing or unsupported components never route to another device', () => {
 const missing = { ...event, id: 'removed', componentId: 'removed-disk' }, unsupported = { ...event, id: 'system', componentId: 'system' };
 const view = render(<AppView monitor={{ ...monitor, snapshot: { ...snapshot, components: [...snapshot.components, { ...disk, id: 'system', kind: 'system' }] } }} events={[missing, unsupported]} />);
 expect(view.queryByRole('button', { name: /^Открыть данные:/ })).toBeNull();
});

test('an event chart can return to live mode and respond to a new focus request', async () => {
 vi.mocked(invoke).mockResolvedValue([]);
 const focus = { request: 1, eventId: event.id, atMs: eventAt, title: event.title, metricId: 'smart-warning' };
 const view = render(<HistoryChart component={disk} at={at} events={[event]} enabled focus={focus} />);
 await waitFor(() => expect(invoke).toHaveBeenLastCalledWith('history_query', { query: expect.objectContaining({ toMs: eventAt + 150000 }) }));
 fireEvent.click(view.getByRole('button', { name: 'Live', exact: true }));
 await waitFor(() => expect(view.getByRole('button', { name: 'Live', exact: true })).toHaveAttribute('aria-pressed', 'true'));
 expect(view.queryByRole('status')).toBeNull();
 const earlier = eventAt - 3600000;
 view.rerender(<HistoryChart component={disk} at={at} enabled focus={{ ...focus, request: 2, atMs: earlier }} />);
 await waitFor(() => expect(invoke).toHaveBeenLastCalledWith('history_query', { query: expect.objectContaining({ fromMs: earlier - 150000, toMs: earlier + 150000 }) }));
});
