#!/usr/bin/env node
// Serves a directory as a static sparse-registry index, for use WITHIN a
// single publish workflow run only — earlier-published crates need to
// resolve via HTTP before later ones can be packaged, and real GitHub
// Pages deploy propagation mid-job would be slow and flaky. The public
// registry gets its one real deploy at the end of the run.
//
// Usage: node serve-local.mjs <dir> <port> — prints "READY" once listening.
import { createServer } from 'node:http';
import { createReadStream, existsSync, statSync } from 'node:fs';
import { join, normalize } from 'node:path';

const [, , dirArg, portArg] = process.argv;
const dir = dirArg ?? '.';
const port = Number(portArg ?? 8080);

const server = createServer((req, res) => {
  const reqPath = normalize(decodeURIComponent(req.url.split('?')[0])).replace(/^(\.\.[/\\])+/, '');
  const filePath = join(dir, reqPath);
  if (!filePath.startsWith(dir) || !existsSync(filePath) || !statSync(filePath).isFile()) {
    res.writeHead(404);
    res.end('not found');
    return;
  }
  res.writeHead(200, { 'content-type': 'application/json' });
  createReadStream(filePath).pipe(res);
});

server.listen(port, '127.0.0.1', () => {
  console.log('READY');
});
