// Shared esbuild config + the Hermes for-of/let engine-bug postprocess
// (see the comment in `patchHermesForOfBug` below) — one source of truth
// for both this package's own `build.mjs` (playground) and any consumer's
// build.mjs (e.g. examples/soundcloud/js/build.mjs), so the alias table
// and the workaround never drift apart between them.
import { readFileSync, writeFileSync } from 'node:fs';

// Spike 7: real RN packages (`@shopify/react-native-skia`, `react-native-
// reanimated`) and anything targeting real RN (`@sc/ui`) never know they're
// not running on Android/iOS — same trick react-native-web uses to swap in
// a DOM-backed `react-native`. Paths are relative to *this file's own
// directory* (js/), so a consumer's build.mjs elsewhere still resolves the
// engine's real shim files, not its own.
export function shimAliases(engineJsDir = new URL('.', import.meta.url).pathname) {
  return {
    'react-native': `${engineJsDir}src/react-native.tsx`,
    '@shopify/react-native-skia': `${engineJsDir}src/rnskia.tsx`,
    'react-native-reanimated': `${engineJsDir}src/reanimated.tsx`,
  };
}

// Hermes engine bug (reproduced in isolation, unrelated to our code): a
// `for (let key of ...)` loop whose body defines a closure via
// `Object.defineProperty` doesn't get a fresh `key` binding per iteration —
// every getter ends up seeing the *last* key. esbuild's own CJS→ESM interop
// helper (`__copyProps`, injected into a bundle when it imports named
// exports from a CommonJS package) hits exactly this pattern, so every
// property read off the wrapped module silently returns the last one
// instead of the real value. `for...of` + `let` is ES2015, so downleveling
// the whole bundle to es5 was the "proper" fix, but esbuild can't fully
// transpile this codebase that far (const/destructuring errors). Patching
// just the helper is surgical: swap the loop for a `.forEach` callback,
// where each `key` is a function *parameter* — Hermes gets that right.
const COPY_PROPS_FOR_OF =
  /for \(let key of __getOwnPropNames\(from\)\)\s*\n\s*if \(!__hasOwnProp\.call\(to, key\) && key !== except\)\s*\n\s*(__defProp\(to, key, \{ get: \(\) => from\[key\], enumerable: !\(desc = __getOwnPropDesc\(from, key\)\) \|\| desc\.enumerable \}\);)/;

/**
 * @param {string} bundlePath
 * @param {{ required?: boolean }} [opts] `required: true` (default) throws
 *   if the vulnerable helper shape isn't found — a regression guard for
 *   bundles known to trigger it (real `@sc/ui`'s named CJS imports from
 *   `react`). Pass `required: false` for a bundle that may or may not
 *   trigger it (e.g. a minimal fixture using only default/namespace-style
 *   imports) — patches it if present, no-ops silently if not.
 */
export function patchHermesForOfBug(bundlePath, { required = true } = {}) {
  const bundle = readFileSync(bundlePath, 'utf8');
  if (!COPY_PROPS_FOR_OF.test(bundle)) {
    if (required) {
      throw new Error(
        'esbuild __copyProps helper text changed shape — update the Hermes for-of/let workaround in build-support.mjs (see comment above) or confirm the bug is fixed upstream.',
      );
    }
    return false;
  }
  const patched = bundle.replace(COPY_PROPS_FOR_OF, (_match, defPropCall) => `__getOwnPropNames(from).forEach(function(key) {
      if (!__hasOwnProp.call(to, key) && key !== except)
        ${defPropCall}
    });`);
  writeFileSync(bundlePath, patched);
  return true;
}
