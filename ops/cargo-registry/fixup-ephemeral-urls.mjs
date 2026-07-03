#!/usr/bin/env node
// Global find-and-replace over registry-index/: swaps the ephemeral local
// server URL (only there so same-run packaging steps can resolve against
// each other without waiting on real GitHub Pages propagation) for the
// real, public registry URL, across every already-written index entry.
// Run once, after ALL crates in a run are packaged, right before the
// Pages deploy.
//
// Doing this translation per-dependency inside publish-crate.mjs instead
// (an earlier attempt) breaks multi-hop same-run chains: an
// already-indexed entry with the real URL written in gets read back by
// cargo's own dependency resolution for a LATER crate in the same run,
// which then treats it as a completely separate, not-yet-live registry
// and tries to fetch straight from it — a real failure this hit once
// publishing js-host/rn-linux/rn-windows together in one run.
//
// Usage: node fixup-ephemeral-urls.mjs <ephemeral-url> <real-url> <registry-index-dir>
import { readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [, , ephemeralUrl, realUrl, dir] = process.argv;
if (!ephemeralUrl || !realUrl || !dir) {
  console.error('usage: node fixup-ephemeral-urls.mjs <ephemeral-url> <real-url> <registry-index-dir>');
  process.exit(1);
}

function walk(path) {
  for (const entry of readdirSync(path)) {
    const full = join(path, entry);
    if (statSync(full).isDirectory()) {
      walk(full);
      continue;
    }
    const content = readFileSync(full, 'utf8');
    if (content.includes(ephemeralUrl)) {
      writeFileSync(full, content.split(ephemeralUrl).join(realUrl));
      console.log(`fixed up ${full}`);
    }
  }
}

walk(dir);
