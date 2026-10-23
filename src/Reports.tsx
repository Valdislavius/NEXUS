import { useEffect, useRef, useState } from 'react';
import { Check, FileDown } from 'lucide-react';
import { desktopCall } from './desktop';
import type { Component } from './model';

export type ReportSections = { summary: boolean; metrics: boolean; diagnostics: boolean; benchmarks: boolean };
const sectionOptions: { id: keyof ReportSections; label: string; detail: string }[] = [
  { id: 'summary', label: 'Сводка и агрегаты', detail: 'Период, доступные ряды и статистика min/max/avg.' },
  { id: 'metrics', label: 'История метрик', detail: 'Показания с временными отметками, единицами и источниками.' },
  { id: 'diagnostics', label: 'Диагностические события', detail: 'Предупреждения и другие зарегистрированные события.' },
  { id: 'benchmarks', label: 'Результаты тестов', detail: 'Результаты и состояния ручных проверок.' },
];

export function Reports({ components, componentId, enabled = false }: { components: Component[]; componentId?: string; enabled?: boolean }) {
  const visibleIds = components.map(c => c.id);
  const componentKey = JSON.stringify(visibleIds);
  const initialSelection = componentId ? visibleIds.filter(id => id === componentId) : visibleIds;
  const initializedSelection = useRef(initialSelection.length > 0);
  const [format, setFormat] = useState('xlsx'), [hours, setHours] = useState(24);
  const [selected, setSelected] = useState(initialSelection), [anonymized, setAnonymized] = useState(true);
  const [sections, setSections] = useState<ReportSections>({ summary: true, metrics: true, diagnostics: true, benchmarks: true });
  const [busy, setBusy] = useState(false), [result, setResult] = useState<{ path: string; rows: number } | null>(null), [error, setError] = useState('');
  useEffect(() => {
    if (!initializedSelection.current && visibleIds.length && (!componentId || visibleIds.includes(componentId))) {
      initializedSelection.current = true;
      setSelected(componentId ? [componentId] : visibleIds);
      return;
    }
    setSelected(previous => {
      const remaining = previous.filter(id => visibleIds.includes(id));
      return remaining.length === previous.length ? previous : remaining;
    });
  }, [componentKey, componentId]);
  const currentSelection = selected.filter(id => visibleIds.includes(id));
  const hasSections = Object.values(sections).some(Boolean);
  function selectComponent(id: string, checked: boolean) {
    initializedSelection.current = true;
    setSelected(previous => {
      const current = previous.filter(value => visibleIds.includes(value));
      return checked ? [...new Set([...current, id])] : current.filter(value => value !== id);
    });
  }
  async function generate() {
    if (!enabled || busy || !currentSelection.length || !hasSections) return;
    setBusy(true); setResult(null); setError('');
    try {
      const toMs = Date.now();
      setResult(await desktopCall('report_generate', { request: { format, fromMs: toMs - hours * 3600000, toMs, components: currentSelection, anonymized, sections } }));
    } catch (e) { setError(String(e)); } finally { setBusy(false); }
  }
  return <section className="panel report-panel">
    <div className="panel-title"><div><span className="eyebrow">ЛОКАЛЬНЫЙ ЭКСПОРТ</span><h2>Диагностический отчёт</h2></div><FileDown size={23} /></div>
    <p className="panel-description">Выберите сводку, историю метрик, диагностические события и результаты тестов. Пропущенные значения сохраняются пустыми.</p>
    <div className="form-group"><span>Формат</span><div className="segmented">{['xlsx', 'csv', 'pdf'].map(f => <button key={f} className={format === f ? 'active' : ''} onClick={() => setFormat(f)}>{f === 'xlsx' ? 'Excel .xlsx' : f.toUpperCase()}</button>)}</div></div>
    {format === 'csv' && <p className="small muted">CSV объединяет выбранные разделы в одной таблице.</p>}
    <div className="form-group"><span>Период</span><div className="segmented">{[1, 6, 24, 168].map(h => <button key={h} className={hours === h ? 'active' : ''} onClick={() => setHours(h)}>{h === 168 ? '7 дней' : `${h} ч`}</button>)}</div></div>
    <fieldset className="checkbox-grid"><legend>Содержание отчёта</legend>{sectionOptions.map(({ id, label, detail }) => <label key={id}><input type="checkbox" checked={sections[id]} onChange={e => setSections(previous => ({ ...previous, [id]: e.target.checked }))} /><span>{label}<small>{detail}</small></span></label>)}</fieldset>
    {!hasSections && <p className="small warning">Выберите хотя бы один раздел отчёта.</p>}
    <fieldset className="checkbox-grid"><legend>Компоненты</legend>{components.map(c => <label key={c.id}><input type="checkbox" checked={currentSelection.includes(c.id)} onChange={e => selectComponent(c.id, e.target.checked)} />{c.name}</label>)}</fieldset>
    {!components.length && <p className="small muted">Ожидаем список компонентов от сборщика.</p>}
    <label className="privacy-option"><input type="checkbox" checked={anonymized} onChange={e => setAnonymized(e.target.checked)} /><span>Обезличенный отчёт<small>Скрывать серийные номера, адреса сети и имена компьютеров.</small></span></label>
    {!anonymized && <p className="warning small">Отчёт может содержать личные сведения об устройствах и сети. Проверьте его перед передачей.</p>}
    <button className="primary-button" disabled={!enabled || busy || !currentSelection.length || !hasSections} onClick={() => void generate()}><FileDown size={16} />{busy ? 'Формируем отчёт…' : 'Создать отчёт'}</button>
    {result && <div className="report-result" role="status"><Check size={18} /><div><strong>Отчёт сохранён · {result.rows} строк</strong><p>{result.path}</p></div></div>}
    {error && <p role="alert" className="setting-error">{error}</p>}
    {!enabled && <p className="small muted">Формирование файлов доступно в настольном приложении.</p>}
  </section>;
}
