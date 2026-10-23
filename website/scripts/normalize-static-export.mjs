import { readdir, readFile, copyFile } from 'node:fs/promises';
import { constants } from 'node:fs';
import { resolve, relative, sep, join } from 'node:path';
import { pathToFileURL } from 'node:url';

// Next.js #92339: Windows export emits nested RSC segment paths while the
// browser requests dot-separated names. Copy those files to the expected names
// without modifying Next.js or requiring a special production server rewrite.
export async function normalizeStaticExport(outputRoot) {
  const root = resolve(outputRoot);
  let copied = 0;
  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const source = join(directory, entry.name);
      if (entry.isDirectory()) { await visit(source); continue; }
      if (!entry.isFile() || !entry.name.endsWith('.txt')) continue;
      const parts = relative(root, source).split(sep);
      const segmentIndex = parts.findIndex(part => part.startsWith('__next.'));
      if (segmentIndex < 0 || segmentIndex === parts.length - 1) continue;
      const destination = join(root, ...parts.slice(0, segmentIndex), parts.slice(segmentIndex).join('.'));
      try { await copyFile(source, destination, constants.COPYFILE_EXCL); copied++; }
      catch (error) {
        if (error.code !== 'EEXIST') throw error;
        if (!(await readFile(source)).equals(await readFile(destination))) throw new Error(`Conflicting RSC segment: ${destination}`);
      }
    }
  }
  await visit(root);
  return copied;
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  process.stdout.write(`Normalized static export: ${await normalizeStaticExport('out')} segment files.\n`);
}
