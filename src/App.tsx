import { useState } from 'react';
import { RefreshCw, Settings2, X, ShieldCheck, ArrowUpRight, CircleAlert } from 'lucide-react';
import { isTauri } from '@tauri-apps/api/core';
import { Card } from './Card';
import { useMonitor } from './useMonitor';
import { downloadDiagnostics } from './diagnostics';
import { kindNames, missingReason } from './presentation';
import type { Snapshot } from './model';
const filters = [ ['all', 'Все компоненты'], ['performance', 'Производительность'], ['storage', 'Накопители'], ['system', 'Система и сеть'] ] as const;
export function Dashboard({ snapshot }: { snapshot: Snapshot }) {
  const [filter, setFilter] = useState<string>('all');
  const visible = snapshot.components.filter(c => c.metrics.length || c.specs.length || c.smart?.length);
  const filtered = visible.filter(c => filter === 'all' || (filter === 'performance' && ['cpu', 'gpu', 'ram'].includes(c.kind)) || (filter === 'storage' && c.kind === 'storage') || (filter === 'system' && ['motherboard', 'system', 'network'].includes(c.kind)));
  return <><div className="section-toolbar"><nav aria-label="Группы компонентов">{filters.map(([id, label]) => <button key={id} className={filter === id ? 'active' : ''} aria-pressed={filter === id} onClick={() => setFilter(id)}>{label}</button>)}</nav><span className="component-count">{visible.length} компонентов</span></div><div className="component-grid">{filtered.map((c, index) => <Card key={c.id} component={c} position={index} />)}</div>{!filtered.length && <div className="empty-state">В этой группе пока нет обнаруженных устройств.</div>}</>;
}
export function AppView({ monitor }: { monitor: ReturnType<typeof useMonitor> }) {
  const { snapshot, error, interval, changeInterval, restart, restarting, stale } = monitor;
  const [settings, setSettings] = useState(false), [settingError, setSettingError] = useState(''), [saving, setSaving] = useState(false);
  const [diagnosticMode, setDiagnosticMode] = useState(false), [exporting, setExporting] = useState(false), [exportMessage, setExportMessage] = useState('');
  const waiting = !snapshot?.ready;
  const displaySnapshot = snapshot && (stale || error) ? { ...snapshot, components: snapshot.components.map(c => ({ ...c, smart: undefined, metrics: c.metrics.map(m => ({ ...m, value: null, diagnostic: { status: 'transient_error' as const, code: 'COLLECTOR_DATA_UNAVAILABLE' }, reason: 'Ожидаем свежие данные' })) })) } : snapshot;
  const unavailable = displaySnapshot?.components.flatMap(c => c.metrics.filter(m => m.value === null).map(m => ({ component: c, metric: m }))) || [];
  const availableCount = displaySnapshot?.components.reduce((count, c) => count + c.metrics.filter(m => m.value !== null).length, 0) || 0;
  const exportReport = async () => {
    setExporting(true); setExportMessage(''); setSettingError('');
    try { await downloadDiagnostics(); setExportMessage('Диагностический отчёт передан для сохранения.'); }
    catch { setSettingError('Не удалось сохранить диагностический отчёт. Повторите попытку.'); }
    finally { setExporting(false); }
  };
  return <div className="app-shell"><header className="app-header"><a className="brand" href="#main" aria-label="NEXUS — к оборудованию"><span className="brand-mark">N</span>NEXUS<span className="version">ALPHA 0.3.1</span></a><div className="header-right"><span className="host-name">{snapshot?.host || 'Этот компьютер'}</span><span className={`status ${error ? 'error' : stale ? 'warning' : waiting ? 'waiting' : ''}`}><i />{error ? 'Сбор прерван' : stale ? 'Данные задерживаются' : waiting ? 'Подключение' : 'Мониторинг активен'}</span><button className={`icon-button ${settings ? 'selected' : ''}`} aria-label="Настройки" aria-expanded={settings} onClick={() => setSettings(!settings)}><Settings2 size={19} /></button></div></header>
    <main id="main"><div className="page-heading"><div><div className="eyebrow">ОБЗОР УСТРОЙСТВА</div><h1>Ваш компьютер<span>.</span></h1><p>Характеристики и показатели в реальном времени.</p></div><button className="secondary-button" onClick={() => void restart()} disabled={restarting || (waiting && !error)}><RefreshCw size={15} className={restarting ? 'spinning' : ''} />{restarting ? 'Определяем…' : 'Обновить оборудование'}</button></div>
      {settings && <section className="settings-panel" aria-label="Настройки мониторинга"><div><h2>Мониторинг</h2><p>Изменения действуют до закрытия приложения. SMART и датчики дисков обновляются раз в 30 секунд.</p></div><label>Обновлять каждые<select value={interval} disabled={saving} onChange={async e => { setSaving(true); setSettingError(''); try { await changeInterval(Number(e.target.value)); } catch { setSettingError('Не удалось изменить интервал. Повторите попытку.'); } finally { setSaving(false); } }}><option value={2000}>2 секунды</option><option value={5000}>5 секунд</option><option value={10000}>10 секунд</option></select></label><button className="icon-button" aria-label="Закрыть настройки" onClick={() => setSettings(false)}><X size={18} /></button>
        <div className="sensor-diagnostics"><h2>Диагностика датчиков</h2><p>Отчёт содержит доступность источников и технические коды. Серийные номера, данные пользователя и IP-адреса не включаются.</p><button className="secondary-button" onClick={() => void exportReport()} disabled={exporting || !isTauri()}>{exporting ? 'Сохраняем…' : 'Экспорт диагностического отчёта'}</button>{!isTauri() && <p>Экспорт доступен в настольном приложении.</p>}<label><input type="checkbox" checked={diagnosticMode} onChange={e => setDiagnosticMode(e.target.checked)} />Безопасный диагностический режим</label>{diagnosticMode && <><p>Показывает текущие статусы и коды ошибок. Дополнительные команды устройствам не отправляются.</p>{!displaySnapshot?.ready ? <p>Ожидаем данные мониторинга.</p> : <><p>Доступных показаний: {availableCount}. Недоступных: {unavailable.length}.</p>{unavailable.length > 0 && <dl className="data-list">{unavailable.map(({ component: c, metric: m }) => <div key={`${c.id}-${m.id}`}><dt>{kindNames[c.kind] || 'Устройство'} · {m.label}</dt><dd>{missingReason(m)}{m.diagnostic && <small>Код: {m.diagnostic.code}</small>}</dd></div>)}</dl>}</>}</>}{exportMessage && <p role="status">{exportMessage}</p>}</div>
        {settingError && <p className="setting-error" role="alert">{settingError}</p>}</section>}
      {error && <div className="notice error" role="alert"><CircleAlert size={20} /><div><strong>Не удалось обновить показатели</strong><p>{error}</p></div><button onClick={() => void restart()} disabled={restarting}>Повторить <ArrowUpRight size={14} /></button></div>}
      {stale && !error && <div className="notice" role="status"><CircleAlert size={18} /><p>Показатели временно недоступны. Можно повторно определить оборудование.</p></div>}
      {waiting && !error ? <div className="loading-state" role="status"><span className="loading-dot" /><h2>Знакомимся с вашим компьютером</h2><p>Читаем характеристики устройств и запускаем мониторинг.</p><div className="skeleton-grid">{[0, 1, 2, 3].map(i => <div key={i} />)}</div></div> : displaySnapshot && <Dashboard snapshot={displaySnapshot} />}
      {!!snapshot?.warnings.length && <details className="diagnostics"><summary><CircleAlert size={14} />Некоторые источники не ответили · {snapshot.warnings.length}</summary>{snapshot.warnings.map((warning, i) => <p key={i}>{warning}</p>)}</details>}
      <footer><span><ShieldCheck size={14} />Данные остаются на этом компьютере</span><span>{snapshot?.ready ? `Последнее обновление ${new Date(snapshot.at).toLocaleTimeString('ru-RU')}` : 'NEXUS Desktop'}<span className="footer-dot">·</span>Сеанс без сохранения истории</span></footer>
    </main></div>;
}

export function App() { return <AppView monitor={useMonitor()} />; }
