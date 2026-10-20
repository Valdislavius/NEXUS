import { useState } from 'react';
import type { CSSProperties } from 'react';
import { ChevronDown, Cpu, Monitor, MemoryStick, HardDrive, CircuitBoard, Laptop, Network } from 'lucide-react';
import { formatMetric } from './model';
import type { Component, Metric } from './model';
import { componentTitle, componentSubtitle, kindNames, readings, metric, diskHealth, metricValue, displaySpecs, metricHint, missingReason } from './presentation';
const icons: Record<string, typeof Cpu> = { cpu: Cpu, gpu: Monitor, ram: MemoryStick, storage: HardDrive, motherboard: CircuitBoard, system: Laptop, network: Network };
function Meter({ value, label }: { value: number | null; label: string }) {
  return <div className="meter" role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={value ?? undefined}><span style={{ width: `${Math.min(100, Math.max(0, value ?? 0))}%` }} /></div>;
}
function MetricRows({ metrics }: { metrics: Metric[] }) {
  return <dl className="data-list">{metrics.map(m => <div key={m.id}><dt title={m.description || m.source}>{m.label}</dt><dd title={metricHint(m)} className={m.value === null ? 'muted' : ''}>{metricValue(m)}{m.value === null && <small>{missingReason(m)}</small>}</dd></div>)}</dl>;
}
function UnavailableSensors({ component: c }: { component: Component }) {
  const missing = c.metrics.filter(m => m.primary && m.value === null);
  if (!missing.length) return null;
  return <section className="detail-section"><h3>Доступность датчиков</h3><dl className="data-list">{missing.map(m => <div key={m.id}><dt title={m.description || m.source}>{m.label}</dt><dd title={m.source}>{missingReason(m)}</dd></div>)}</dl></section>;
}
function SATAAttributes({ component: c }: { component: Component }) {
  if (!c.smart?.length) return null;
  return <section className="detail-section"><h3>Атрибуты ATA SMART</h3><p className="smart-explanation">Норм. — значение по шкале производителя. RAW — исходные данные; единицы зависят от модели.</p><div className="smart-table-scroll"><table className="smart-table"><thead><tr><th scope="col">Атрибут</th><th scope="col" title="Нормализованное текущее значение">Норм.</th><th scope="col" title="Худшее нормализованное значение">Худшее</th><th scope="col" title="Порог производителя, если прочитан">Порог</th><th scope="col">RAW</th></tr></thead><tbody>{c.smart.map(a => <tr key={a.id}><th scope="row" title={`${a.source} · Флаги 0x${a.flags.toString(16).toUpperCase()}`}><span>{a.id} · {a.name}</span>{a.interpretation && <small>{a.interpretation}{a.value != null && `: ${formatMetric({ value: a.value, unit: a.unit || '' })}`}</small>}</th><td>{a.current}</td><td>{a.worst}</td><td>{a.threshold ?? '—'}</td><td>{a.rawValue}<small title="Шесть исходных байтов SMART">HEX {a.rawHex.toUpperCase()}</small></td></tr>)}</tbody></table></div></section>;
}
function CpuThreads({ component: c }: { component: Component }) {
  const threads = c.metrics.filter(m => m.id.startsWith('thread-'));
  if (!threads.length) return null;
  return <section className="detail-section"><h3>Загрузка потоков <span>{threads.length} потоков</span></h3><div className="thread-grid">{threads.map(m => <div className="thread" key={m.id}><div><span>{m.label}</span><strong>{formatMetric(m)}</strong></div><Meter value={m.value} label={m.label} /></div>)}</div></section>;
}
function Volumes({ component: c }: { component: Component }) {
  const totals = c.metrics.filter(m => m.id.startsWith('volume-total-'));
  if (!totals.length) return null;
  return <section className="detail-section"><h3>Тома и свободное место</h3>{totals.map(total => {
    const letter = total.id.slice('volume-total-'.length), used = metric(c, `volume-${letter}`), free = metric(c, `volume-free-${letter}`);
    const percent = total.value && used?.value != null ? used.value / total.value * 100 : null;
    return <div className="volume" key={letter}><div className="volume-title"><strong>{letter}</strong><span>{formatMetric(total)}</span></div><Meter value={percent} label={`Заполнение ${letter}`} /><p>{free ? `${formatMetric(free)} свободно` : 'Обновление данных тома'}{used && ` · ${formatMetric(used)} занято`}</p></div>;
  })}</section>;
}
function NetworkDetails({ component: c }: { component: Component }) {
 return <section className="detail-section full"><h3>Сетевые адаптеры</h3><div className="adapter-grid">{c.specs.map(s => {
   const prefix=s.label+' · ', metrics=c.metrics.filter(m=>m.label.startsWith(prefix)).map(m=>({...m,label:m.label.slice(prefix.length)}));
   return <details className="adapter" key={s.label}><summary><strong>{s.label}</strong><span>{s.value.split(' · ').slice(0,2).join(' · ')}</span><ChevronDown size={15}/></summary><div className="adapter-content"><p>{s.value.split(' · ').slice(2).join(' · ')}</p><MetricRows metrics={metrics}/></div></details>;
 })}</div></section>;
}
function Details({ component: c }: { component: Component }) {
  const extra = c.metrics.filter(m => !m.primary && !/^(thread-|volume-|clock-)/.test(m.id) && !['load', 'capacity', 'volumeCapacity', 'free'].includes(m.id));
  const specs = displaySpecs(c);
  const characteristics = specs.length > 0 && <section className="detail-section"><h3>{c.kind === 'ram' ? 'Модули памяти' : 'Характеристики'}</h3>{c.kind === 'ram'
    ? <div className="module-grid">{specs.map((s, i) => <div className="module" key={i}><strong>{s.label.replace(/^DDR4-/, 'Слот ')}</strong>{s.value.split(' · ').map((part, j) => <p key={j}>{part}</p>)}</div>)}</div>
    : <dl className="data-list">{specs.map((s, i) => <div key={i}><dt>{s.label}</dt><dd>{/^Раздел \d+$/.test(s.label) && s.value.includes(" · ") ? <>{s.value.split(" · ")[0]}<small>{s.value.split(" · ").slice(1).join(" · ")}</small></> : s.value}</dd></div>)}</dl>}</section>;
  const sensors = extra.length > 0 && <section className="detail-section"><h3>{c.kind === 'storage' ? 'Активность и SMART' : 'Дополнительные показатели'}</h3><MetricRows metrics={extra}/>{c.kind === 'storage' && diskHealth(c) && <p className="detail-note">SMART сообщает об известных накопителю предупреждениях. Их отсутствие не гарантирует исправность или сохранность данных.</p>}</section>;
  const missing = c.metrics.some(m => m.primary && m.value === null);
  const smart = c.kind === 'storage' && !!c.smart?.length;
  const single = ['gpu', 'ram', 'network', 'motherboard', 'system'].includes(c.kind) || (!extra.length && c.kind !== 'cpu' && c.kind !== 'storage');
  return <div className="card-details"><div className={`detail-grid ${single ? 'single' : ''}`}>
    {c.kind === 'network' ? <NetworkDetails component={c}/> : <>
      <div className="detail-lane">{characteristics}{c.kind === 'storage' && <Volumes component={c}/>}</div>
      <div className="detail-lane" hidden={!sensors && !missing && !smart && c.kind !== 'cpu'}>{sensors}<UnavailableSensors component={c}/>{smart && <SATAAttributes component={c}/>} {c.kind === 'cpu' && <CpuThreads component={c}/>}</div>
    </>}
  </div>{c.note && <p className="detail-note">{c.note}</p>}</div>;
}
export function Card({ component: c, position = 0 }: { component: Component; position?: number }) {
  const [open, setOpen] = useState(false), Icon = icons[c.kind] || Cpu;
  const load = metric(c, 'load'), health = diskHealth(c);
  const rows = readings(c);
  return <article className={`card ${open ? 'is-open' : ''}`} data-kind={c.kind} style={{'--summary-order': Math.floor(position / 2) * 4 + position % 2, '--detail-order': Math.floor(position / 2) * 4 + 2 + position % 2, '--mobile-summary-order': position * 2, '--mobile-detail-order': position * 2 + 1} as CSSProperties}>
    <button className="card-summary" onClick={() => setOpen(!open)} aria-expanded={open} aria-controls={`detail-${c.id}`}>
      <div className="card-top"><span className="component-kind"><Icon size={17} strokeWidth={1.6} />{kindNames[c.kind] || c.kind}</span><ChevronDown className="chevron" size={17} /></div>
      <div className="card-identity"><h2 title={c.name}>{componentTitle(c)}</h2><p>{componentSubtitle(c)}</p></div>
      <div className="readings">{rows.map((r, i) => <div className="reading" key={i} title={r.hint}><span>{r.label}</span><strong className={r.missing ? 'muted' : r.value.length > 11 ? 'long-value' : ''}>{r.value}</strong></div>)}</div>
      <div className="card-bottom">{load && c.kind !== 'network' ? <Meter value={load.value} label={`${kindNames[c.kind]}: ${load.label}`} /> : <span className="quiet-line" />}
        <span className={`card-caption ${health?.warning ? 'warning' : ''}`}>{health ? health.label : c.kind === 'storage' ? 'Физический накопитель' : c.kind === 'motherboard' || c.kind === 'system' ? 'Сведения об устройстве' : 'Текущие показатели'}<span>{open ? 'Свернуть' : 'Подробнее'}</span></span>
      </div>
    </button>
    <div className="card-expansion" id={`detail-${c.id}`} hidden={!open}>{open && <><div className="detail-heading"><div><span>{kindNames[c.kind]}</span><h2>{componentTitle(c)}</h2></div><button onClick={() => setOpen(false)} aria-label={`Свернуть ${componentTitle(c)}`}>Свернуть <ChevronDown size={15}/></button></div><Details component={c} /></>}</div>
  </article>;
}

