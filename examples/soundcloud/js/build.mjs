import * as esbuild from 'esbuild';

import { patchHermesForOfBug, shimAliases } from '../../../js/build-support.mjs';

// Same shape as the engine's own js/build.mjs — see its comments for why
// (IIFE/no module loader, NODE_ENV `--define`d not real, alias trick).
// `shimAliases()` — no argument — resolves relative to build-support.mjs's
// OWN location (always `js/`, an absolute path derived from its
// `import.meta.url`), regardless of which build.mjs calls it, so this
// points at the engine's real shim files, the single source of truth
// shared with the engine's own build — never a second hand-rolled copy.
await esbuild.build({
  entryPoints: ['src/index.tsx'],
  bundle: true,
  outfile: 'dist/bundle.js',
  format: 'iife',
  platform: 'neutral',
  mainFields: ['main'],
  target: 'es2020',
  jsx: 'automatic',
  define: { 'process.env.NODE_ENV': '"development"' },
  alias: shimAliases(),
});

// `required: true` (default) — this bundle imports named exports from
// `react` via `@sc/ui`'s `ThemeProvider.tsx`, which is known to trigger the
// Hermes engine bug (Desktop-Runtime/CLAUDE.md, spike 7a) — a missing match
// here is a real regression, not just an absent edge case.
patchHermesForOfBug('dist/bundle.js');

console.log('built dist/bundle.js (patched __copyProps for a Hermes for-of/let engine bug)');
