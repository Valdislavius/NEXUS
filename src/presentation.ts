import type { Component, Metric, DiagnosticStatus } from './model';
import { formatMetric } from './model';
export const kindNames: Record<string, string> = { cpu: 'Процессор', gpu: 'Видеокарта', ram: 'Оперативная память', storage: 'Накопитель', motherboard: 'Материнская плата', system: 'Система', network: 'Сеть' };
export const metric = (c: Component, id: string) => c.metrics.find(m => m.id === id);
export const spec = (c: Component, label: string) => c.specs.find(s => s.label === label)?.value;
export const cleanName = (name: string) => name.replace(/\(R\)|\(TM\)/g, '').replace(/^12th Gen /, '').replace(/[.,]0 ГиБ RAM$/, ' ГиБ').trim();
export const diagnosticLabels: Record<DiagnosticStatus, string> = {
  unsupported: 'Датчик не поддерживается', permission_denied: 'Недостаточно прав',
  dependency_missing: 'Отсутствует необходимая зависимость', device_unresponsive: 'Устройство не отвечает',
  driver_no_data: 'Драйвер не предоставляет данные', invalid_response: 'Некорректный ответ',
  transient_error: 'Временная ошибка чтения',
};
export function missingReason(m: Metric): string {
  return m.diagnostic ? diagnosticLabels[m.diagnostic.status] : m.reason || 'Датчик не предоставляет данные';
}
export function metricHint(m: Metric): string {
  return [m.value === null ? missingReason(m) : '', m.description, m.source].filter(Boolean).join(' · ');
}
export function displaySpecs(c: Component) {
  return c.specs.filter(s => !(s.label === 'Сокет' && /^U3E1$/i.test(s.value.trim())))
    .map(s => ({ ...s, value: s.value.replace(/^GenuineIntel$/, 'Intel').replace(/^AuthenticAMD$/, 'AMD') }));
}
export function metricValue(m: Metric): string {
  if (m.value === null) return formatMetric(m);
  if (m.id === 'smart-failure') return m.value === 0 ? 'Сбой не прогнозируется' : 'Прогнозируется сбой';
  if (m.id === 'smart-warning') {
    if (m.value === 0) return 'Не обнаружены';
    const flags = ['Мало резервных блоков', 'Температура вне порога', 'Снижена надёжность', 'Только чтение', 'Сбой резервного питания', 'Память переведена в режим чтения'];
    const labels = flags.filter((_, i) => (m.value! & (1 << i)) !== 0);
    if ((m.value & ~63) !== 0) labels.push(`Дополнительный код: 0x${m.value.toString(16).toUpperCase()}`);
    return labels.join(' · ');
  }
  return formatMetric(m);
}
export function componentTitle(c: Component) {
  if (c.kind === 'motherboard') return spec(c, 'Модель') || c.name;
  if (c.kind === 'system') return c.name.replace(/^Майкрософт /, '');
  return cleanName(c.name);
}
export type Reading = { label: string; value: string; hint?: string; missing?: boolean };
export function readings(c: Component): Reading[] {
  const fromMetric = (m: Metric): Reading => ({ label: m.label.replace(' Windows', ''), value: formatMetric(m), hint: metricHint(m), missing: m.value === null });
  const result = c.metrics.filter(m => m.primary).map(fromMetric);
  const add = (label: string, value?: string) => { if (value) result.push({ label, value }); };
  if (c.kind === 'motherboard') { add('BIOS', spec(c, 'Версия BIOS')); add('Дата BIOS', spec(c, 'Дата BIOS')); add('Производитель', spec(c, 'Производитель')?.replace(' Technology Co., Ltd.', ''));  }
  if (c.kind === 'system') { add('Сборка', spec(c, 'Сборка')); add('Архитектура', spec(c, 'Архитектура')); }
  if (c.kind === 'cpu' && !c.metrics.some(m => m.id === 'temperature')) result.push({label:'Температура',value:'Нет данных',missing:true});
  if (c.kind === 'storage' && result.length < 3) { const free = metric(c, 'free'); if (free) result.push(fromMetric(free)); }
  if (!result.length) c.specs.slice(0, 3).forEach(s => add(s.label, s.value));
  return result.slice(0, 3);
}
export function diskHealth(c: Component): { label: string; warning: boolean } | null {
  const critical = metric(c, 'smart-warning'), predicted = metric(c, 'smart-failure');
  if (critical?.value != null) return { label: critical.value === 0 ? 'SMART: без предупреждений' : 'SMART: есть предупреждения', warning: critical.value !== 0 };
  if (predicted?.value != null) return { label: predicted.value === 0 ? 'SMART: сбой не прогнозируется' : 'SMART: прогнозируется сбой', warning: predicted.value !== 0 };
  return null;
}

export function componentSubtitle(c: Component) {
 if(c.kind==='gpu'){const total=metric(c,'memoryTotal');return total?.value!=null?formatMetric(total)+' видеопамяти':c.subtitle.replace(' · датчики NVML','');}
 if(c.kind==='motherboard')return spec(c,'Производитель')?.replace(' Technology Co., Ltd.','')||'Платформа компьютера';
 if(c.kind==='system')return 'Операционная система';
 return c.subtitle;
}
