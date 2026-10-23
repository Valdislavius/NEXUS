import { describe, it, expect } from 'vitest';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { normalizeStaticExport } from '../scripts/normalize-static-export.mjs';
describe('Экспорт сегментов Next.js на Windows', () => {
  it('исправляет вложенное имя, сохраняет данные и повторяется безопасно', async () => {
    const prefix = join(tmpdir(), 'nexus-export-test-');
    const directory = await mkdtemp(prefix);
    try {
      await mkdir(join(directory, 'docs', '__next.docs'), { recursive: true });
      await writeFile(join(directory, 'docs', '__next.docs', '__PAGE__.txt'), 'verified-rsc-data');
      await writeFile(join(directory, 'docs', 'index.html'), '<h1>Docs</h1>');
      expect(await normalizeStaticExport(directory)).toBe(1);
      expect(await readFile(join(directory, 'docs', '__next.docs.__PAGE__.txt'), 'utf8')).toBe('verified-rsc-data');
      expect(await normalizeStaticExport(directory)).toBe(0);
      expect(await readFile(join(directory, 'docs', 'index.html'), 'utf8')).toBe('<h1>Docs</h1>');
    } finally { if (directory.startsWith(prefix)) await rm(directory, { recursive: true, force: true }); }
  });
});
