# compat/

Structural compatibility checks for the three real packages this engine
provides shims for: `react-native`, `@shopify/react-native-skia`,
`react-native-reanimated`. We never run these packages — the whole point
of the engine is to reimplement their JS-visible API surface on top of
real Yoga/Hermes/Skia instead — so "compatible with version X" can only
mean "the exact subset of the API `@sc/ui` uses still type-checks the
same way against both the real package and our shim."

## How it works

`snippets/*.tsx` are small, hand-written components mirroring real usage
patterns grepped straight from `@sc/ui`'s source (not the full shim
surface — just what's actually exercised in production). Each snippet
compiles twice:

- `pnpm check:real` — `tsconfig.real.json`, plain `node_modules`
  resolution, against the real npm packages (devDependencies here, at the
  versions pinned in `VERSIONS.json`).
- `pnpm check:shims` — `tsconfig.shims.json`, same snippets, with
  `react-native`/`@shopify/react-native-skia`/`react-native-reanimated`
  path-aliased to `../js/src/*.tsx` — the actual engine shims.

Both must pass clean. `pnpm check` runs both in sequence.

This is a **structural**, not behavioral, check — it proves the shims'
type surface still matches what real consumer code expects for a given
upstream version. It does not (and can't, without executing real RN)
prove pixel-identical runtime behavior. Behavioral coverage against the
real, unmodified `@sc/ui` package lives in `../e2e/` and needs `Core`
checked out as a sibling of this repo — see `../docs/pitfalls/` for bugs
that only that layer ever caught.

`react-reconciler` is tracked in `VERSIONS.json` too, but isn't shimmed —
`js/` depends on the real package directly. Its compatibility is whatever
the normal `cargo test --workspace` / `pnpm build` suite already proves
when it's bumped, not a dedicated snippet here.

## Updating

`VERSIONS.json` + the compatibility table (`generate-table.mjs`, embedded
in `../README.md` and the usage guides) are updated automatically by
[`.github/workflows/compat-check.yml`](../.github/workflows/compat-check.yml)
— a weekly cron that bumps the pinned versions here, re-runs `pnpm check`,
and opens a PR only if everything still passes. Don't hand-edit
`VERSIONS.json`.
