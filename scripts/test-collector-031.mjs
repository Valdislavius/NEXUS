import { spawn } from 'node:child_process';
import { writeFileSync, mkdirSync } from 'node:fs';
import { snapshotSchema } from '../src/model.ts';

mkdirSync('artifacts/alpha031', { recursive: true });
const binary = 'src-tauri/binaries/nexus-collector-x86_64-pc-windows-msvc.exe';
// The artifact contains selected sensor evidence only, never the full snapshot,
// host/network specifications, serials or verbatim provider errors.
async function run(mode) {
  const child = spawn(binary, [], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  let buffer = '', samples = 0, stderrLines = 0, evidence = [], finished = false;
  const timer = setTimeout(() => child.kill(), 30000);
  return await new Promise((resolve, reject) => {
    child.on('error', reject);
    child.stderr.on('data', data => { stderrLines += data.toString().split('\n').filter(Boolean).length; });
    child.stdout.on('data', data => {
      buffer += data; let end;
      while ((end = buffer.indexOf('\n')) >= 0) {
        const snapshot = snapshotSchema.parse(JSON.parse(buffer.slice(0, end))); buffer = buffer.slice(end + 1);
        if (!snapshot.ready) continue;
        samples++;
        evidence = snapshot.components.filter(c => ['cpu', 'gpu', 'storage'].includes(c.kind)).map(c => ({
          kind: c.kind, model: c.name, smartAttributes: c.smart?.length || 0,
          metrics: c.metrics.filter(m => ['temperature', 'power-hours', 'power-cycles', 'wear-leveling', 'lifetime-written', 'sata-crc-errors', 'wear', 'media-errors', 'smart-data'].includes(m.id))
            .map(m => ({ id: m.id, value: m.value, unit: m.unit, source: m.source, ...(m.diagnostic ? { diagnostic: m.diagnostic } : {}) })),
        }));
        if (samples >= 2 && !finished) {
          finished = true;
          if (mode === 'stop') child.stdin.write('{"command":"stop"}\n'); else child.stdin.end();
        }
      }
    });
    child.on('exit', code => {
      clearTimeout(timer);
      if (code !== 0 || !finished) { reject(new Error(`Collector ${mode} failed: ${code}`)); return; }
      resolve({ mode, passed: true, exitCode: code, readySnapshots: samples, stderrLines, evidence });
    });
  });
}
const results = [await run('eof'), await run('stop')];
writeFileSync('artifacts/alpha031/collector-lifecycle-and-sensors.json', JSON.stringify(results, null, 2));
console.log(JSON.stringify(results, null, 2));
