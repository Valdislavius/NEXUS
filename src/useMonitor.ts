import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { invalidateExpiredMetrics, reconcileSnapshot, snapshotSchema } from './model';
import type { Snapshot } from './model';
export function useMonitor() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [error, setError] = useState('');
  const [interval, setIntervalValue] = useState(2000);
  const [restarting, setRestarting] = useState(false);
  const [now, setNow] = useState(performance.now());
  const receivedAt = useRef<number | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    const cleanup: (() => void)[] = [];
    let liveSnapshotVersion = 0, liveIntervalVersion = 0;
    const acceptInterval = (value: unknown) => {
      if (mounted.current && typeof value === 'number' && [2000, 5000, 10000].includes(value)) setIntervalValue(value);
    };
    const accept = (raw: unknown) => {
      if (!mounted.current) return;
      const parsed = snapshotSchema.safeParse(raw);
      if (!parsed.success) { setError('Не удалось прочитать показания. Попробуйте обновить оборудование.'); return; }
      // The native backend already rejects obsolete Collector generations.
      // Wall-clock timestamps can move backwards after Windows corrects time.
      const wallAge = Date.now() - Date.parse(parsed.data.at);
      receivedAt.current = wallAge < 0 ? Number.NEGATIVE_INFINITY : performance.now() - wallAge;
      setSnapshot(old => reconcileSnapshot(old, invalidateExpiredMetrics(parsed.data)));
      setError('');
      if (parsed.data.ready) setRestarting(false);
    };
    async function connect() {
      if (!isTauri()) { setError('Откройте настольное приложение NEXUS для мониторинга компьютера.'); return; }
      try {
        for (const [event, handler] of [
          ['nexus:snapshot', (value: unknown) => { liveSnapshotVersion++; accept(value); }],
          ['nexus:error', (value: unknown) => { if (mounted.current) { setError(String(value)); setRestarting(false); } }],
          ['nexus:interval', (value: unknown) => { liveIntervalVersion++; acceptInterval(value); }],
        ] as const) {
          const unlisten = await listen(event, e => handler(e.payload));
          if (mounted.current) cleanup.push(unlisten); else unlisten();
        }
        const snapshotVersion = liveSnapshotVersion, intervalVersion = liveIntervalVersion;
        const result = await invoke<{ snapshot: unknown; error: string | null; intervalMs?: number }>('current_snapshot');
        if (liveIntervalVersion === intervalVersion) acceptInterval(result.intervalMs);
        if (liveSnapshotVersion === snapshotVersion) {
          if (result.snapshot) accept(result.snapshot);
          if (result.error && mounted.current) setError(result.error);
        }
      } catch (e) { if (mounted.current) setError(String(e)); }
    }
    void connect();
    const timer = window.setInterval(() => setNow(performance.now()), 5000);
    return () => { mounted.current = false; cleanup.forEach(f => f()); window.clearInterval(timer); };
  }, []);
  const changeInterval = useCallback(async (value: number) => {
    await invoke('set_interval', { milliseconds: value });
    setIntervalValue(value);
  }, []);
  const restart = useCallback(async () => {
    if (restarting) return;
    setRestarting(true); setError(''); setSnapshot(null);
    try { await invoke('restart_collector', { milliseconds: interval }); }
    catch (e) { if (mounted.current) { setError(String(e)); setRestarting(false); } }
  }, [interval, restarting]);
  return { snapshot: snapshot ? invalidateExpiredMetrics(snapshot) : null, error, interval, changeInterval, restart, restarting,
    stale: !!snapshot?.ready && receivedAt.current !== null && now - receivedAt.current > Math.max(15000, interval * 3) };
}

