import * as esbuild from 'esbuild';

// IIFE, not ESM/CJS — Hermes has no module loader, it just runs a script.
// NODE_ENV is `--define`d (not a real `process.env`) so esbuild's own dead-code
// elimination strips react/react-reconciler's production-vs-development
// branches at bundle time; Hermes never needs a `process` global at runtime.
await esbuild.build({
  entryPoints: ['src/index.ts'],
  bundle: true,
  outfile: 'dist/bundle.js',
  format: 'iife',
  platform: 'neutral',
  target: 'es2020',
  define: { 'process.env.NODE_ENV': '"development"' },
});

console.log('built dist/bundle.js');
