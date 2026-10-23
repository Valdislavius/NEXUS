import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useMonitor } from '../src/useMonitor';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
const listeners = new Map<string, (event: { event: string; id: number; payload: unknown }) => void>();
const snapshot = { version: 1, type: 'snapshot', at: new Date().toISOString(), host: 'fixture', platform: 'windows', ready: true, warnings: [], components: [] };

beforeEach(() => {
  vi.mocked(listen).mockImplementation(async (event, handler) => {
    listeners.set(event, handler as (event: { event: string; id: number; payload: unknown }) => void);
    return () => { listeners.delete(event); };
  });
  vi.mocked(invoke).mockImplementation(async command => command === 'current_snapshot' ? { snapshot, error: null, intervalMs: 5000 } : undefined);
});
afterEach(() => { cleanup(); listeners.clear(); vi.useRealTimers(); vi.restoreAllMocks(); vi.resetAllMocks(); });

test('a restored economy interval is displayed and preserved on collector restart', async () => {
  const { result } = renderHook(() => useMonitor());
  await waitFor(() => expect(result.current.interval).toBe(5000));
  await act(() => result.current.restart());
  expect(invoke).toHaveBeenCalledWith('restart_collector', { milliseconds: 5000 });
});

test('profile interval events update the next restart without another hardware request', async () => {
  const { result } = renderHook(() => useMonitor());
  await waitFor(() => expect(result.current.interval).toBe(5000));
  act(() => listeners.get('nexus:interval')?.({ event: 'nexus:interval', id: 1, payload: 2000 }));
  expect(result.current.interval).toBe(2000);
  await act(() => result.current.restart());
  expect(invoke).toHaveBeenCalledWith('restart_collector', { milliseconds: 2000 });
  expect(vi.mocked(invoke).mock.calls.filter(([name]) => name === 'current_snapshot')).toHaveLength(1);
});

test('invalid interval events cannot change the polling cadence', async () => {
  const { result } = renderHook(() => useMonitor());
  await waitFor(() => expect(result.current.interval).toBe(5000));
  for (const payload of [1000, 15000, '2000', null, NaN]) act(() => listeners.get('nexus:interval')?.({ event: 'nexus:interval', id: 1, payload }));
  expect(result.current.interval).toBe(5000);
});

test('closing the frontend releases every collector event subscription', async () => {
  const { result, unmount } = renderHook(() => useMonitor());
  await waitFor(() => expect(result.current.interval).toBe(5000));
  expect(listeners.size).toBe(3);
  unmount();
  expect(listeners.size).toBe(0);
});

test('live frames remain current after a clock rollback and still expire by monotonic receipt time', async () => {
 vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
 const monotonic = vi.spyOn(performance, 'now').mockReturnValue(100);
 const wallTime = vi.spyOn(Date, 'now').mockReturnValue(Date.parse(snapshot.at));
 const { result } = renderHook(() => useMonitor());
 await act(async () => { await Promise.resolve(); });
 expect(result.current.interval).toBe(5000);
 const earlier = new Date(Date.parse(snapshot.at) - 3600000).toISOString();
 wallTime.mockReturnValue(Date.parse(earlier));
 act(() => listeners.get('nexus:snapshot')?.({ event: 'nexus:snapshot', id: 1, payload: { ...snapshot, at: earlier } }));
 expect(result.current.snapshot?.at).toBe(earlier);
 expect(result.current.stale).toBe(false);
 monotonic.mockReturnValue(20100);
 act(() => vi.advanceTimersByTime(20000));
 expect(result.current.stale).toBe(true);
});

test('an initial snapshot response cannot overwrite newer live data or interval events', async () => {
 let resolveInitial!: (value: unknown) => void;
 vi.mocked(invoke).mockImplementationOnce(() => new Promise(resolve => { resolveInitial = resolve; }));
 const { result } = renderHook(() => useMonitor());
 await waitFor(() => expect(invoke).toHaveBeenCalledWith('current_snapshot'));
 const live = { ...snapshot, at: new Date(Date.parse(snapshot.at) + 1000).toISOString() };
 act(() => {
  listeners.get('nexus:snapshot')?.({ event: 'nexus:snapshot', id: 1, payload: live });
  listeners.get('nexus:interval')?.({ event: 'nexus:interval', id: 2, payload: 10000 });
 });
 await act(async () => resolveInitial({ snapshot, error: 'Obsolete cached error', intervalMs: 2000 }));
 expect(result.current.snapshot?.at).toBe(live.at);
 expect(result.current.interval).toBe(10000);
 expect(result.current.error).toBe('');
});

test('individual sensor TTL expires while transport is still inside its grace interval', async () => {
 vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
 const monotonic = vi.spyOn(performance, 'now').mockReturnValue(100);
 const wallTime = vi.spyOn(Date, 'now').mockReturnValue(Date.parse(snapshot.at));
 const payload = { ...snapshot, components: [{ id: 'cpu', kind: 'cpu', name: 'Fixture CPU', subtitle: '', specs: [], metrics: [{ id: 'load', label: 'Load', value: 42, unit: '%', source: 'fixture', primary: true, at: snapshot.at, maxAgeMs: 6000 }] }] };
 vi.mocked(invoke).mockResolvedValue({ snapshot: payload, error: null, intervalMs: 2000 });
 const { result } = renderHook(() => useMonitor());
 await act(async () => { await Promise.resolve(); });
 expect(result.current.snapshot?.components[0].metrics[0].value).toBe(42);
 monotonic.mockReturnValue(7100); wallTime.mockReturnValue(Date.parse(snapshot.at) + 7000);
 act(() => vi.advanceTimersByTime(5000));
 expect(result.current.stale).toBe(false);
 expect(result.current.snapshot?.components[0].metrics[0].value).toBeNull();
});
