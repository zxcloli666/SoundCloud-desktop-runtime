import * as esbuild from 'esbuild';

import { patchHermesForOfBug, shimAliases } from './build-support.mjs';

// IIFE, not ESM/CJS — Hermes has no module loader, it just runs a script.
// NODE_ENV is `--define`d (not a real `process.env`) so esbuild's own dead-code
// elimination strips react/react-reconciler's production-vs-development
// branches at bundle time; Hermes never needs a `process` global at runtime.
await esbuild.build({
  entryPoints: ['playground/src/index.tsx'],
  bundle: true,
  outfile: 'dist/playground.js',
  format: 'iife',
  platform: 'neutral',
  // "neutral" doesn't default to honoring package.json's `main` (unlike
  // node/browser) — the real RN packages the playground imports (spike 7)
  // need it spelled out.
  mainFields: ['main'],
  target: 'es2020',
  jsx: 'automatic',
  define: { 'process.env.NODE_ENV': '"development"' },
  alias: shimAliases(),
});

// See build-support.mjs's `patchHermesForOfBug` doc comment for the full
// story. `required: false` here: the playground only uses default/
// namespace-style imports from CJS packages (proven safe already by the
// real demo bundle before it ever hit this bug), so the vulnerable helper
// shape may legitimately never appear — patch it if esbuild did emit it
// anyway, no-op otherwise.
const patched = patchHermesForOfBug('dist/playground.js', { required: false });

console.log(`built dist/playground.js${patched ? ' (patched __copyProps for a Hermes for-of/let engine bug)' : ''}`);
