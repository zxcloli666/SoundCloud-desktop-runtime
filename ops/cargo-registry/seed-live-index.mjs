#!/usr/bin/env node
// Seeds registry-index/ with whatever's already live on GitHub Pages,
// before publish-crate.mjs runs for each crate. Without this,
// `mkdir -p registry-index` starts every workflow run from an empty
// index, so publish-crate.mjs's "already indexed — skipping" check can
// never fire (the file it checks doesn't exist yet THIS run) — every run
// re-packages every crate, even ones whose version didn't change. A
// freshly re-packaged tarball can hash differently than what's already
// live (`cargo package` embeds the current git commit in
// .cargo_vcs_info.json), and "upload release assets" skips re-uploading
// an existing release — so the freshly written index entry would then
// carry a checksum that doesn't match the actually-downloadable asset.
// Seeding first makes "already indexed" real, so unchanged crates are
// correctly skipped end to end, not just within one run.
//
// Usage: node seed-live-index.mjs <base-url> <registry-index-dir> <crate-name...>
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [, , baseUrl, indexDirArg, ...crateNames] = process.argv;
if (!baseUrl || !indexDirArg || crateNames.length === 0) {
  console.error('usage: node seed-live-index.mjs <base-url> <registry-index-dir> <crate-name...>');
  process.exit(1);
}

// Same convention as publish-crate.mjs's own indexPathFor — kept as an
// independent copy since these are two small, standalone CLI scripts.
function indexPathFor(name) {
  const n = name.length;
  if (n === 1) return `1/${name}`;
  if (n === 2) return `2/${name}`;
  if (n === 3) return `3/${name[0]}/${name}`;
  return `${name.slice(0, 2)}/${name.slice(2, 4)}/${name}`;
}

for (const name of crateNames) {
  const path = indexPathFor(name);
  const url = `${baseUrl.replace(/\/$/, '')}/${path}`;
  const res = await fetch(url);
  if (!res.ok) {
    console.log(`${name}: not live yet (HTTP ${res.status}) — nothing to seed`);
    continue;
  }
  const body = await res.text();
  const dest = join(indexDirArg, path);
  mkdirSync(join(dest, '..'), { recursive: true });
  writeFileSync(dest, body);
  console.log(`seeded ${name} from ${url}`);
}
