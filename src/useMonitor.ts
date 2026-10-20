import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { reconcileSnapshot, snapshotSchema } from './model';
import type { Snapshot } from './model';
export function useMonitor() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [error, setError] = useState('');
  const [interval, setIntervalValue] = useState(2000);
  const [restarting, setRestarting] = useState(false);
  const [now, setNow] = useState(Date.now());
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    const cleanup: (() => void)[] = [];
    const accept = (raw: unknown) => {
      if (!mounted.current) return;
      const parsed = snapshotSchema.safeParse(raw);
      if (!parsed.success) { setError('Не удалось прочитать показания. Попробуйте обновить оборудование.'); return; }
      setSnapshot(old => old && Date.parse(old.at) > Date.parse(parsed.data.at) ? old : reconcileSnapshot(old, parsed.data));
      setError('');
      if (parsed.data.ready) setRestarting(false);
    };
    async function connect() {
      if (!isTauri()) { setError('Откройте настольное приложение NEXUS для мониторинга компьютера.'); return; }
      try {
        for (const [event, handler] of [
          ['nexus:snapshot', accept],
          ['nexus:error', (value: unknown) => { if (mounted.current) { setError(String(value)); setRestarting(false); } }],
        ] as const) {
          const unlisten = await listen(event, e => handler(e.payload));
          if (mounted.current) cleanup.push(unlisten); else unlisten();
        }
        const result = await invoke<{ snapshot: unknown; error: string | null }>('current_snapshot');
        if (result.snapshot) accept(result.snapshot);
        if (result.error && mounted.current) setError(result.error);
      } catch (e) { if (mounted.current) setError(String(e)); }
    }
    void connect();
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
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
  return { snapshot, error, interval, changeInterval, restart, restarting,
    stale: !!snapshot?.ready && now - Date.parse(snapshot.at) > Math.max(15000, interval * 3) };
}
