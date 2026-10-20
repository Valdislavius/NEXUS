import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';
import { diagnosticStatusSchema } from './model';

const sensorReportSchema = z.object({
  component: z.string(), sensor: z.string(), source: z.string(),
  status: z.union([z.literal('available'), diagnosticStatusSchema]), code: z.string().optional(),
});
// Reconstruct the allowlisted report before writing. Snapshots, names, specs,
// addresses and arbitrary backend error strings never enter this download.
export const diagnosticReportSchema = z.object({
  version: z.literal(1), type: z.literal('diagnostics'), appVersion: z.literal('0.3.1'), at: z.string(),
  privacy: z.object({ serialNumbers: z.literal(false), userIdentifiers: z.literal(false), ipAddresses: z.literal(false) }),
  collector: z.object({ running: z.boolean(), snapshotReady: z.boolean() }),
  sensors: z.array(sensorReportSchema),
  log: z.array(sensorReportSchema.extend({ at: z.string() })).max(128).optional(),
});
export type DiagnosticReport = z.infer<typeof diagnosticReportSchema>;

export async function downloadDiagnostics(): Promise<void> {
  const report = diagnosticReportSchema.parse(await invoke<DiagnosticReport>('export_diagnostics'));
  const blob = new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `NEXUS-diagnostics-${new Date().toISOString().slice(0, 10)}.json`;
  document.body.appendChild(anchor);
  try { anchor.click(); }
  finally { anchor.remove(); window.setTimeout(() => URL.revokeObjectURL(url), 1000); }
}
