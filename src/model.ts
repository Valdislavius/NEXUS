import { z } from 'zod';
export const diagnosticStatusSchema = z.enum([
  'unsupported', 'permission_denied', 'dependency_missing', 'device_unresponsive',
  'driver_no_data', 'invalid_response', 'transient_error',
]);
export const diagnosticSchema = z.object({ status: diagnosticStatusSchema, code: z.string() });
const timestampSchema = z.iso.datetime({ offset: true });
export const metricSchema = z.object({
  id: z.string(), label: z.string(), value: z.number().finite().nonnegative().nullable(),
  unit: z.string(), source: z.string(), reason: z.string().optional(), at: timestampSchema, primary: z.boolean(),
  description: z.string().optional(), diagnostic: diagnosticSchema.optional(),
  status: z.enum(['available', 'unavailable', 'stale']).optional(), maxAgeMs: z.number().int().positive().optional(),
});
export const smartAttributeSchema = z.object({
  id: z.number().int().min(1).max(255), name: z.string(), flags: z.number().int().min(0).max(65535),
  current: z.number().int().min(0).max(255), worst: z.number().int().min(0).max(255),
  threshold: z.number().int().min(0).max(255).optional(), rawHex: z.string().regex(/^[0-9a-fA-F]{12}$/),
  rawValue: z.string().regex(/^\d+$/), interpretation: z.string().optional(), unit: z.string().optional(),
  value: z.number().finite().nonnegative().nullable().optional(), source: z.string(),
});
export const componentSchema = z.object({
  id: z.string(), kind: z.string(), name: z.string(), subtitle: z.string(),
  specs: z.array(z.object({ label: z.string(), value: z.string() })), metrics: z.array(metricSchema), note: z.string().optional(),
  smart: z.array(smartAttributeSchema).optional(),
});
export const snapshotSchema = z.object({
  version: z.literal(1), type: z.literal('snapshot'), at: timestampSchema, host: z.string(), platform: z.string(),
  ready: z.boolean(), components: z.array(componentSchema), warnings: z.array(z.string()),
});
export type Component = z.infer<typeof componentSchema>;
export type Metric = z.infer<typeof metricSchema>;
export type DiagnosticStatus = z.infer<typeof diagnosticStatusSchema>;
export type SMARTAttribute = z.infer<typeof smartAttributeSchema>;
export type Snapshot = z.infer<typeof snapshotSchema>;
const number = (v: number, digits = 1) => v.toLocaleString('ru-RU', { maximumFractionDigits: digits });
export function formatMetric(m: Pick<Metric, 'value' | 'unit'>): string {
  if (m.value === null) return 'Нет данных';
  const v = m.value;
  if (m.unit === 'bytes' || m.unit === 'B/s') {
    const units = m.unit === 'bytes' ? ['Б', 'КиБ', 'МиБ', 'ГиБ', 'ТиБ', 'ПиБ'] : ['Б/с', 'КиБ/с', 'МиБ/с', 'ГиБ/с', 'ТиБ/с', 'ПиБ/с'];
    const i = Math.min(5, Math.max(0, Math.floor(Math.log2(v || 1) / 10)));
    return `${number(v / 1024 ** i)} ${units[i]}`;
  }
  if (m.unit === 'bit/s') {
    const i = Math.min(3, Math.max(0, Math.floor(Math.log10(v || 1) / 3)));
    return `${number(v / 1000 ** i)} ${['бит/с', 'Кбит/с', 'Мбит/с', 'Гбит/с'][i]}`;
  }
  if (m.unit === 'seconds') {
    const minutes = Math.floor(v / 60);
    return `${minutes >= 1440 ? `${Math.floor(minutes / 1440)} д ` : ''}${Math.floor(minutes / 60) % 24} ч ${minutes % 60} мин`;
  }
  if (m.unit === '°C') return `${number(v, 0)} °C`;
  if (m.unit === 'hours') return `${number(v, 0)} ч`;
  if (m.unit === 'МГц' && v >= 1000) return `${number(v / 1000, 2)} ГГц`;
  return `${number(v)}${m.unit ? ` ${m.unit}` : ''}`;
}
// Never advertise sensors that have not produced a real reading. Once discovered,
// retain the row on a temporary failure, with null rather than an old value or zero.
export function reconcileSnapshot(previous: Snapshot | null, incoming: Snapshot): Snapshot {
  return {
    ...incoming,
    components: incoming.components.map(c => {
      const known = new Set(previous?.components.find(p => p.id === c.id)?.metrics.map(m => m.id));
      return { ...c, specs: c.specs.filter(s => s.value.trim() && !/недоступно|не реализован|^x\.x$|^To Be Filled By O.E.M.$|^Default string$/i.test(s.value)),
        metrics: c.metrics.filter(m => m.value !== null || known.has(m.id) || m.diagnostic || (c.kind === 'cpu' && m.id === 'temperature')) };
    }),
  };
}

// A fresh transport frame never makes an expired cached hardware reading current.
export function invalidateExpiredMetrics(snapshot: Snapshot, now = Date.now()): Snapshot {
  let changed = false;
  const components = snapshot.components.map(c => {
    let componentChanged = false;
    const metrics = c.metrics.map(m => {
      const age = now - Date.parse(m.at);
      const invalidTime = !Number.isFinite(age) || age < 0;
      const expired = invalidTime || m.status === 'stale' || m.status === 'unavailable' || (m.maxAgeMs != null && age > m.maxAgeMs);
      if (!expired || m.value === null) return m;
      changed = componentChanged = true;
      return { ...m, value: null, status: 'stale' as const, reason: invalidTime ? 'Время показания недостоверно' : 'Срок актуальности показания истёк', diagnostic: { status: invalidTime ? 'invalid_response' as const : 'transient_error' as const, code: invalidTime ? 'SENSOR_TIMESTAMP_INVALID' : 'SENSOR_READING_EXPIRED' } };
    });
    return componentChanged ? { ...c, metrics } : c;
  });
  return changed ? { ...snapshot, components } : snapshot;
}
