import { readFileSync, writeFileSync } from 'node:fs';

import * as esbuild from 'esbuild';

// IIFE, not ESM/CJS — Hermes has no module loader, it just runs a script.
// NODE_ENV is `--define`d (not a real `process.env`) so esbuild's own dead-code
// elimination strips react/react-reconciler's production-vs-development
// branches at bundle time; Hermes never needs a `process` global at runtime.
await esbuild.build({
  entryPoints: ['src/index.tsx'],
  bundle: true,
  outfile: 'dist/bundle.js',
  format: 'iife',
  platform: 'neutral',
  // "neutral" doesn't default to honoring package.json's `main` (unlike
  // node/browser) — `@sc/ui` and friends need it spelled out.
  mainFields: ['main'],
  target: 'es2020',
  jsx: 'automatic',
  define: { 'process.env.NODE_ENV': '"development"' },
  // Spike 7: `@sc/ui` (and anything importing these) targets real RN — it
  // never knows it's not running on Android/iOS. Same trick react-native-web
  // uses to swap in a DOM-backed `react-native`.
  alias: {
    'react-native': './src/react-native.tsx',
    '@shopify/react-native-skia': './src/rnskia.tsx',
    'react-native-reanimated': './src/reanimated.tsx',
  },
});

// Hermes engine bug (reproduced in isolation, unrelated to our code): a
// `for (let key of ...)` loop whose body defines a closure via
// `Object.defineProperty` doesn't get a fresh `key` binding per iteration —
// every getter ends up seeing the *last* key. esbuild's own CJS→ESM interop
// helper (`__copyProps`, injected into every bundle that imports a named
// export from a CommonJS package — here, `react`'s `createContext`/`useMemo`)
// hits exactly this pattern, so every property read off the wrapped module
// silently returns the last one (`version`, a string) instead of the real
// function. `for...of` + `let` is ES2015, so downleveling the whole bundle to
// es5 was the "proper" fix, but esbuild can't fully transpile this codebase
// that far (const/destructuring errors). Patching just the helper is
// surgical: swap the loop for a `.forEach` callback, where each `key` is a
// function *parameter* — Hermes gets that right (proven by the rest of this
// app running correctly on far more call-heavy code paths).
const copyPropsForOf =
  /for \(let key of __getOwnPropNames\(from\)\)\s*\n\s*if \(!__hasOwnProp\.call\(to, key\) && key !== except\)\s*\n\s*(__defProp\(to, key, \{ get: \(\) => from\[key\], enumerable: !\(desc = __getOwnPropDesc\(from, key\)\) \|\| desc\.enumerable \}\);)/;

const bundle = readFileSync('dist/bundle.js', 'utf8');
if (!copyPropsForOf.test(bundle)) {
  throw new Error(
    'esbuild __copyProps helper text changed shape — update the Hermes for-of/let workaround in build.mjs (see comment above) or confirm the bug is fixed upstream.',
  );
}
const patchedBundle = bundle.replace(copyPropsForOf, (_match, defPropCall) => `__getOwnPropNames(from).forEach(function(key) {
      if (!__hasOwnProp.call(to, key) && key !== except)
        ${defPropCall}
    });`);
writeFileSync('dist/bundle.js', patchedBundle);

console.log('built dist/bundle.js (patched __copyProps for a Hermes for-of/let engine bug)');
