import { useEffect, useMemo, useRef, useState } from 'react';
import { ChevronLeft, ChevronRight, Download, Minus, Plus, Radio } from 'lucide-react';
import { desktopCall, timeLabel } from './desktop';
import type { HistoryPoint, HistorySeries, TimelineEvent } from './desktop';
import type { Component } from './model';
import { formatMetric } from './model';

type ChartPrefs = { duration: number; ids: string[] };
export function boundedPoints<T>(points: T[], max = 300): T[] {
  if (max <= 0) return [];
  if (points.length <= max) return points;
  if (max < 2) return points.slice(-Math.max(0, max));
  return Array.from({ length: max }, (_, i) => points[Math.round(i * (points.length - 1) / (max - 1))]);
}
export function stats(points: HistoryPoint[]) {
  const values = points.filter(p => p.value !== null);
  const count = values.reduce((sum, p) => sum + Math.max(1, p.count), 0);
  return { min: values.length ? Math.min(...values.map(p => p.min ?? p.value!)) : null, max: values.length ? Math.max(...values.map(p => p.max ?? p.value!)) : null, avg: count ? values.reduce((sum, p) => sum + p.value! * Math.max(1, p.count), 0) / count : null };
}
const palette = ['#8ce5a3', '#8aafff', '#e9bc71', '#c397ef'];
function loadPrefs(key: string): ChartPrefs | null { try { const value = JSON.parse(localStorage.getItem(key) || 'null'); return value && Array.isArray(value.ids) && value.ids.every((id: unknown) => typeof id === 'string') && Number.isFinite(value.duration) ? value : null; } catch { return null; } }
export function HistoryChart({ component, at, events = [], enabled = false }: { component: Component; at: string; events?: TimelineEvent[]; enabled?: boolean }) {
  const storageKey = `nexus.chart.${component.id}`;
  const prefs = useMemo(() => loadPrefs(storageKey), [storageKey]);
  const metrics = component.metrics.filter(m => !m.id.startsWith('volume-total-'));
  const [ids, setIds] = useState<string[]>(prefs?.ids?.filter(id => metrics.some(m => m.id === id)).slice(0, 4) || []);
  const chosen = ids.length ? ids : metrics.filter(m => m.primary).slice(0, 1).map(m => m.id).concat(metrics.length ? [metrics[0].id] : []).slice(0, 1);
  const [duration, setDuration] = useState(prefs?.duration && prefs.duration >= 60000 && prefs.duration <= 86400000 ? prefs.duration : 300000);
  const [live, setLive] = useState(true), [end, setEnd] = useState(Date.now()), [compare, setCompare] = useState(false);
  const [series, setSeries] = useState<HistorySeries[]>([]), [previous, setPrevious] = useState<HistorySeries[]>([]), [message, setMessage] = useState('');
  const [hover, setHover] = useState<number | null>(null);
  const generation = useRef(0), canvas = useRef<SVGSVGElement>(null);
  const to = live ? Math.max(Date.parse(at) || Date.now(), Date.now()) : end;
  const from = to - duration, idKey = chosen.join('|');
  useEffect(() => { try { localStorage.setItem(storageKey, JSON.stringify({ duration, ids: chosen })); } catch { /* Storage may be unavailable in a restricted WebView. */ } }, [storageKey, duration, idKey]);
  useEffect(() => {
    const current = ++generation.current;
    const query = { componentId: component.id, metricIds: chosen, fromMs: from, toMs: to, maxPoints: 300 };
    let gone = false;
    async function update() {
      try {
        if (!enabled) throw new Error('preview');
        const result = await desktopCall<HistorySeries[]>('history_query', { query });
        if (gone || current !== generation.current) return;
        const rows = result.map(s => ({ ...s, points: boundedPoints(s.points) }));
        for (const metricId of chosen) if (!rows.some(s => s.metricId === metricId)) { const m = component.metrics.find(m => m.id === metricId)!; rows.push({ componentId: component.id, metricId, unit: m.unit, source: m.source, points: [], min: null, max: null, avg: null }); }
        setSeries(rows); setMessage('');
        if (compare) {
          const older = await desktopCall<HistorySeries[]>('history_query', { query: { ...query, fromMs: from - duration, toMs: from } });
          if (!gone && current === generation.current) setPrevious(older.map(s => ({ ...s, points: boundedPoints(s.points.map(p => ({ ...p, atMs: p.atMs + duration }))) })));
        } else setPrevious([]);
      } catch (error) {
        if (gone) return;
        // Browser preview uses actual received samples only; no synthetic curve.
        setSeries(old => chosen.map(metricId => {
          const m = component.metrics.find(m => m.id === metricId)!;
          const existing = old.find(s => s.metricId === metricId && s.componentId === component.id)?.points || [];
          const pointAt = Date.parse(m.at || at);
          const points = existing.filter(p => p.atMs >= from && p.atMs <= to && p.atMs !== pointAt);
          if (live && Number.isFinite(pointAt) && pointAt >= from && pointAt <= to) points.push({ atMs: pointAt, value: m.value, min: m.value, max: m.value, count: m.value === null ? 0 : 1 });
          return { componentId: component.id, metricId, unit: m.unit, source: m.source, points: boundedPoints(points.sort((a, b) => a.atMs - b.atMs)), ...stats(points) };
        }));
        setPrevious([]); setMessage(enabled ? `История временно недоступна: ${String(error)}. Показаны только полученные за этот сеанс данные.` : 'Показаны только полученные за этот сеанс данные. История доступна в Desktop.');
      }
    }
    if (chosen.length) void update();
    return () => { gone = true; };
  }, [component.id, at, idKey, duration, live, end, compare, enabled]);
  const units = [...new Set(series.map(s => s.unit))];
  const exportCsv = () => {
    const lines = ['timestamp,component,metric,value,unit,source'];
    const cell = (v: unknown) => `"${String(v ?? '').replace(/^[=+@-]/, "'$&").replace(/"/g, '""')}"`;
    for (const s of series) for (const p of s.points) lines.push([new Date(p.atMs).toISOString(), s.componentId, s.metricId, p.value, s.unit, s.source].map(cell).join(','));
    const url = URL.createObjectURL(new Blob(['\uFEFF' + lines.join('\r\n')], { type: 'text/csv;charset=utf-8' }));
    const a = document.createElement('a'); a.href = url; a.download = 'NEXUS-chart.csv'; a.click(); window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  };
  function pan(direction: number) { setEnd(to + direction * duration / 2); setLive(false); }
  const eventList = events.filter(e => e.atMs >= from && e.atMs <= to && (!e.componentId || e.componentId === component.id));
  return <section className="panel history-panel" aria-label="Графики и история"><div className="panel-title"><div><span className="eyebrow">ИСТОРИЯ ПОКАЗАТЕЛЕЙ</span><h2>Активность во времени</h2></div><button className={`live-button ${live ? 'active' : ''}`} aria-pressed={live} onClick={() => { if (live) setEnd(to); setLive(!live); }}><Radio size={14} />Live</button></div>
    <div className="chart-toolbar"><div className="segmented">{[[300000, '5 мин'], [1800000, '30 мин'], [21600000, '6 ч'], [86400000, '24 ч']].map(([value, label]) => <button key={value} className={duration === value ? 'active' : ''} onClick={() => setDuration(Number(value))}>{label}</button>)}</div><div className="chart-tools"><button className="icon-button" aria-label="Предыдущий период" onClick={() => pan(-1)}><ChevronLeft size={16} /></button><button className="icon-button" aria-label="Следующий период" disabled={live || to >= Date.now()} onClick={() => pan(1)}><ChevronRight size={16} /></button><button className="icon-button" aria-label="Увеличить масштаб" onClick={() => setDuration(Math.max(60000, duration / 2))}><Plus size={16} /></button><button className="icon-button" aria-label="Уменьшить масштаб" onClick={() => setDuration(Math.min(86400000, duration * 2))}><Minus size={16} /></button><button className="icon-button" aria-label="Экспорт агрегированных точек графика CSV" onClick={exportCsv} disabled={!series.some(s => s.points.length)}><Download size={16} /></button></div></div>
    <div className="metric-selectors">{metrics.slice(0, 48).map(m => <label key={m.id} className={chosen.includes(m.id) ? 'checked' : ''}><input type="checkbox" checked={chosen.includes(m.id)} disabled={!chosen.includes(m.id) && chosen.length >= 4} onChange={e => setIds(e.target.checked ? [...chosen, m.id] : chosen.filter(id => id !== m.id))} />{m.label}<small>{m.unit}</small></label>)}</div>
    {!metrics.length && <p className="empty-inline">Нет доступных рядов. Датчики этого компонента не предоставляют числовые показатели.</p>}
    {units.map(unit => {
      const group = series.filter(s => s.unit === unit), all = group.flatMap(s => s.points.flatMap(p => p.value === null ? [] : [p.max ?? p.value]));
      const ymax = Math.max(unit === '%' ? 100 : 1, ...all) * (unit === '%' ? 1 : 1.08);
      const x = (v: number) => 45 + (v - from) / duration * 835, y = (v: number) => 168 - v / ymax * 140;
      return <div className="chart-unit" key={unit}><span className="axis-unit">{unit || 'Значение'}</span><svg ref={canvas} viewBox="0 0 920 200" role="img" aria-label={`История, ${unit || 'значения'}`} onPointerLeave={() => setHover(null)} onPointerMove={e => { const rect = e.currentTarget.getBoundingClientRect(); setHover(from + Math.min(1, Math.max(0, ((e.clientX - rect.left) / rect.width * 920 - 45) / 835)) * duration); }}>
        {[0, .5, 1].map(f => <g key={f}><line x1="45" y1={y(ymax * f)} x2="880" y2={y(ymax * f)} className="grid-line" /><text x="35" y={y(ymax * f) + 4} textAnchor="end">{formatMetric({ value: ymax * f, unit: unit === '%' ? '' : unit })}</text></g>)}
        {group.map(s => { let d = ''; let gap = true; for (const p of s.points) { if (p.value === null) { gap = true; continue; } d += `${gap ? 'M' : 'L'}${x(p.atMs).toFixed(2)} ${y(p.value).toFixed(2)} `; gap = false; } return <g key={s.metricId}><path d={d} stroke={palette[series.indexOf(s) % 4]} className="chart-line" />{s.points.filter(p => p.value !== null).length === 1 && s.points.filter(p => p.value !== null).map(p => <circle key={p.atMs} cx={x(p.atMs)} cy={y(p.value!)} r="3" fill={palette[series.indexOf(s) % 4]} />)}</g>; })}
        {previous.filter(s => s.unit === unit).map(s => <path key={s.metricId} d={s.points.reduce((state, p) => p.value === null ? { d: state.d, gap: true } : { d: `${state.d}${state.gap ? 'M' : 'L'}${x(p.atMs)} ${y(p.value)} `, gap: false }, { d: '', gap: true }).d} fill="none" stroke="#a5b7ac" strokeWidth="1.5" strokeDasharray="4 4" opacity=".55" />)}
        {eventList.map(e => <line key={e.id} x1={x(e.atMs)} x2={x(e.atMs)} y1="23" y2="168" className={`event-line ${e.severity}`}><title>{e.title}</title></line>)}
        {hover !== null && <line x1={x(hover)} x2={x(hover)} y1="23" y2="168" className="hover-line" />}
        {[0, .5, 1].map(f => <text key={f} x={x(from + duration * f)} y="192" textAnchor={f === 0 ? 'start' : f === 1 ? 'end' : 'middle'}>{new Date(from + duration * f).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })}</text>)}
      </svg>{!all.length && <p className="chart-empty">Ожидаем данные за выбранный период</p>}</div>;
    })}
    {hover !== null && <div className="chart-tooltip"><span>{timeLabel(hover)}</span>{series.map(s => { const p = s.points.reduce<HistoryPoint | null>((nearest, p) => !nearest || Math.abs(p.atMs - hover) < Math.abs(nearest.atMs - hover) ? p : nearest, null); return <strong key={s.metricId}>{component.metrics.find(m => m.id === s.metricId)?.label}: {p ? `${formatMetric({ value: p.value, unit: s.unit })} · ${timeLabel(p.atMs)}` : 'Нет данных'}</strong>; })}</div>}
    <div className="chart-stats">{series.map((s, i) => <div key={s.metricId}><i style={{ background: palette[i % 4] }} /><span>{component.metrics.find(m => m.id === s.metricId)?.label}</span><span>Мин. <b>{formatMetric({ value: s.min, unit: s.unit })}</b></span><span>Сред. <b>{formatMetric({ value: s.avg, unit: s.unit })}</b></span><span>Макс. <b>{formatMetric({ value: s.max, unit: s.unit })}</b></span></div>)}</div>
    <div className="chart-foot"><label><input type="checkbox" checked={compare} onChange={e => setCompare(e.target.checked)} />Сравнить с предыдущим периодом</label><span>Агрегированные точки · до 300 на ряд · {timeLabel(from)} — {timeLabel(to)}</span></div>{message && <p className="small muted">{message}</p>}
  </section>;
}





