import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';

const origin = new URL(process.argv[2] ?? 'http://127.0.0.1:3000').origin;
const results = [];
async function check(path, expectedStatus, redirect = 'follow') {
  const response = await fetch(`${origin}${path}`, { redirect, signal: AbortSignal.timeout(10000) });
  assert.equal(response.status, expectedStatus, `${path}: unexpected HTTP status`);
  const result = { path, status: response.status, contentType: response.headers.get('content-type'), cacheControl: response.headers.get('cache-control'), location: response.headers.get('location') };
  results.push(result);
  return response;
}
for (const path of ['/', '/features/', '/download/', '/docs/', '/roadmap/', '/about/']) {
  const response = await check(path, 200);
  assert.match(response.headers.get('server') ?? '', /^nginx$/);
  assert.match(await response.text(), /NEXUS/);
  assert.equal(response.headers.get('cache-control'), 'no-cache');
}
const redirect = await check('/features', 301, 'manual');
assert.equal(redirect.headers.get('location'), '/features/');
const missing = await check('/nonexistent-page', 404);
assert.match(await missing.text(), /Здесь пока ничего нет/);
await check('/missing.js', 404);
await check('/robots.txt', 200);
await check('/sitemap.xml', 200);
const icon = await check('/favicon.svg', 200);
assert.match(icon.headers.get('content-type') ?? '', /image\/svg\+xml/);
for (const name of ['overview', 'cpu', 'storage']) await check(`/images/${name}.webp`, 404);
for (const path of ['/docs/__next._tree.txt', '/docs/__next.docs.__PAGE__.txt']) {
  const response = await check(path, 200);
  assert.match(response.headers.get('content-type') ?? '', /text\/plain/);
  assert.equal(response.headers.get('cache-control'), 'no-cache');
}
const homepage = await fetch(origin).then(response => response.text());
const chunk = homepage.match(/(?:src|href)="([^" ]*\/_next\/static\/[^" ]+\.(?:js|css))"/);
assert.ok(chunk, 'A hashed Next.js asset is required');
const staticAsset = await check(chunk[1], 200);
assert.match(staticAsset.headers.get('cache-control') ?? '', /max-age=31536000.*immutable/);
await mkdir('qa', { recursive: true });
await writeFile('qa/docker-http-results.json', JSON.stringify({ checkedAt: new Date().toISOString(), origin, passed: results.length, results }, null, 2) + '\n');
process.stdout.write(`Docker HTTP checks passed: ${results.length}\n`);
