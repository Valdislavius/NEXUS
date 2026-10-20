import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { resolve, extname, sep } from 'node:path';
const root = resolve('out');
const portIndex = process.argv.indexOf('--port');
const port = Number(portIndex >= 0 ? process.argv[portIndex + 1] : 3000);
const mime = { '.html': 'text/html; charset=utf-8', '.css': 'text/css; charset=utf-8', '.js': 'application/javascript; charset=utf-8', '.json': 'application/json; charset=utf-8', '.txt': 'text/plain; charset=utf-8', '.xml': 'application/xml; charset=utf-8', '.svg': 'image/svg+xml', '.webp': 'image/webp', '.png': 'image/png', '.ico': 'image/x-icon', '.woff2': 'font/woff2' };
await stat(resolve(root, 'index.html'));
createServer(async (request, response) => {
  try {
    if (!['GET', 'HEAD'].includes(request.method ?? '')) { response.writeHead(405, { Allow: 'GET, HEAD' }); response.end(); return; }
    const url = new URL(request.url ?? '/', 'http://localhost');
    const pathname = decodeURIComponent(url.pathname);
    if (pathname.includes('\0')) throw new Error('Invalid path');
    let target = resolve(root, `.${pathname}`);
    if (target !== root && !target.startsWith(root + sep)) { response.writeHead(403); response.end(); return; }
    let statusCode = 200;
    try {
      const info = await stat(target);
      if (info.isDirectory()) {
        if (!pathname.endsWith('/')) { response.writeHead(308, { Location: url.pathname + '/' + url.search }); response.end(); return; }
        target = resolve(target, 'index.html');
      }
      await stat(target);
    } catch { target = resolve(root, '404.html'); statusCode = 404; }
    const content = await readFile(target);
    response.writeHead(statusCode, { 'Content-Type': mime[extname(target)] ?? 'application/octet-stream', 'Content-Length': content.byteLength, 'X-Content-Type-Options': 'nosniff', 'Cache-Control': 'no-cache' });
    response.end(request.method === 'HEAD' ? undefined : content);
  } catch { response.writeHead(400); response.end('Invalid request'); }
}).listen(port, '127.0.0.1', () => process.stdout.write(`NEXUS static preview: http://127.0.0.1:${port}\n`));
