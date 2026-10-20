import { invoke, isTauri } from '@tauri-apps/api/core';
export type HistoryPoint = { atMs: number; value: number | null; min: number | null; max: number | null; count: number };
export type HistorySeries = { componentId: string; metricId: string; unit: string; source: string; points: HistoryPoint[]; min: number | null; max: number | null; avg: number | null };
export type TimelineEvent = { id: string; atMs: number; source: string; componentId: string; severity: 'info' | 'warning' | 'critical'; title: string; details: string; data: Record<string, unknown> };
export type Finding = { id: string; componentId: string; componentKind: string; severity: 'warning' | 'critical'; title: string; atMs: number; actual: string | { value: number | null; unit: string; threshold?: number; previous?: number }; reason: string; explanations: string[]; actions: string[] };
export type Diagnostics = { atMs: number; state: 'normal' | 'warning' | 'critical' | 'insufficient_data'; findings: Finding[]; availableSensors: number; totalSensors: number; fresh: boolean; storageError?: string | null };
export type BenchmarkKind = 'cpu_single' | 'cpu_multi' | 'memory' | 'storage_read' | 'network_dns' | 'motherboard';
export type BenchmarkResult = { id: string; kind: BenchmarkKind; status: 'running' | 'completed' | 'cancelled' | 'failed' | 'stopped'; startedAtMs: number; finishedAtMs?: number; progress: number; score?: number; unit?: string; details: string; error?: string };
export type DesktopSettings = { profile: 'economy' | 'balanced' | 'detailed'; backgroundMode: boolean; autostart: boolean; closeBehavior: 'tray' | 'exit'; retentionDays: number; overlay: { mode: 'mini' | 'advanced'; x: number; y: number; scale: number; opacity: number; metricIds: string[] }; hotkeys: { overlay: string; main: string }; thresholds: { cpuTemperature: number; gpuTemperature: number; storageTemperature: number; memoryLoad: number } };
export const defaultSettings: DesktopSettings = { profile: 'balanced', backgroundMode: true, autostart: false, closeBehavior: 'tray', retentionDays: 7, overlay: { mode: 'mini', x: 24, y: 24, scale: 1, opacity: .92, metricIds: ['cpu.load', 'cpu.temperature', 'gpu.load', 'gpu.temperature', 'ram.load'] }, hotkeys: { overlay: 'Ctrl+Shift+F10', main: 'Ctrl+Shift+F9' }, thresholds: { cpuTemperature: 90, gpuTemperature: 85, storageTemperature: 60, memoryLoad: 90 } };
export function desktopCall<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) return Promise.reject(new Error('Эта функция доступна в настольном приложении.'));
  return invoke<T>(command, args);
}
export const timeLabel = (at: number) => new Date(at).toLocaleString('ru-RU', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit' });
export const stateLabels = { normal: 'Норма', warning: 'Предупреждение', critical: 'Критическое событие', insufficient_data: 'Недостаточно данных' };



