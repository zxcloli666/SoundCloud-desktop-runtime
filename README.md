# Desktop-Runtime

**Run React Native components on Windows and Linux desktop — no Electron,
no Tauri, no webview, no Meta Fabric C++.**

[![CI](https://github.com/zxcloli666/SoundCloud-desktop-runtime/actions/workflows/ci.yml/badge.svg)](https://github.com/zxcloli666/SoundCloud-desktop-runtime/actions/workflows/ci.yml)
[![Compatibility check](https://github.com/zxcloli666/SoundCloud-desktop-runtime/actions/workflows/compat-check.yml/badge.svg)](https://github.com/zxcloli666/SoundCloud-desktop-runtime/actions/workflows/compat-check.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)

Desktop-Runtime is a React Native host built from the same real building
blocks React Native itself uses — [Hermes](https://github.com/facebook/hermes)
(the JS engine), real [Yoga](https://github.com/facebook/yoga) flexbox
layout, and real [Skia](https://skia.org) GPU rendering — glued together
by a thin Rust host and the real
[`react-reconciler`](https://www.npmjs.com/package/react-reconciler),
instead of vendoring Meta's Fabric C++ renderer. It ships drop-in
compatibility shims for `react-native`,
[`@shopify/react-native-skia`](https://shopify.github.io/react-native-skia/),
and [`react-native-reanimated`](https://docs.swmansion.com/react-native-reanimated/),
resolved at bundle time the same way `react-native-web` swaps in a
DOM-backed `react-native` — so existing React Native component code can
run on desktop largely unmodified.

`@shopify/react-native-skia` has no desktop backend today (only iOS/
Android/macOS) — Desktop-Runtime exists to close that gap without pulling
in a browser engine.

## Why not Electron/Tauri/a webview

Those run your UI in a browser engine — a different rendering pipeline
than the one your React Native components were built and tested against,
plus the memory/binary-size overhead of shipping a browser. Desktop-Runtime
renders the *same* Yoga layout and Skia draw calls a phone would, through
the *same* Hermes engine, so desktop isn't a second UI to maintain.

## Why not Meta's Fabric C++

Fabric's shadow-tree/scheduler is the least documented, most tightly
coupled part of React Native's internals — even React Native Windows
doesn't reuse it as a library, it reimplements its own. Desktop-Runtime
takes the same approach deliberately: real Yoga, real Hermes, real Skia,
the real `react-reconciler` package, with a small host-config gluing them
together, instead of vendoring Fabric.

## Compatibility

<!-- COMPAT_TABLE:START -->
| Package | Verified against | How |
| --- | --- | --- |
| `react-native` | `0.86.0` | [structural type check](../compat/) against real types + `e2e/` against real `@sc/ui` |
| `@shopify/react-native-skia` | `2.6.9` | [structural type check](../compat/) against real types + `e2e/` against real `@sc/ui` |
| `react-native-reanimated` | `4.5.0` | [structural type check](../compat/) against real types + `e2e/` against real `@sc/ui` |
| `react-reconciler` | `0.33.0` | full test suite against the real package (no shim — used as-is) |

_Last verified: 2026-07-02. Updated automatically by [.github/workflows/compat-check.yml](.github/workflows/compat-check.yml)._
<!-- COMPAT_TABLE:END -->

This is a structural check, not a runtime one — Desktop-Runtime never
executes the real `react-native`/Skia/Reanimated packages, it reimplements
their JS-visible API surface. See [`compat/README.md`](./compat/README.md)
for exactly what that does and doesn't prove.

## Install

Rust crates and the JS engine package are published to this repo's own
registries (not crates.io/npm — see [`docs/registry.md`](./docs/registry.md)
for why and the full mechanics):

```toml
# .cargo/config.toml
[registries]
desktop-runtime = { index = "sparse+https://zxcloli666.github.io/SoundCloud-desktop-runtime/registry/" }
```

```toml
# Cargo.toml
[dependencies]
rn-linux = { version = "0.1.0", registry = "desktop-runtime" }
```

```sh
npm config set @zxcloli666:registry https://npm.pkg.github.com
npm install @zxcloli666/desktop-runtime-js
```

Full walkthrough, including how to write a bundle against the shims and
plug in your own host functions: [`docs/usage.md`](./docs/usage.md)
([русская версия](./docs/usage.ru.md)).

## Repository layout

```
crates/          the engine (Rust): skia-desktop, js-host, rn-linux
js/              the engine's JS half: react-native/skia/reanimated shims,
                 react-reconciler host-config, and a zero-dependency
                 "playground" demo
compat/          structural compatibility checks against real RN/Skia/
                 Reanimated types
examples/soundcloud/   how SoundCloud itself uses this engine
e2e/             integration tests against the real SoundCloud example
docs/            usage guides, the pitfalls/gotchas log, registry mechanics
```

`crates/` + `js/` have zero SoundCloud-specific dependencies — `cargo
build`/`cargo test` at the repo root and `pnpm build`/`pnpm typecheck` in
`js/` work on a bare clone, nothing else needs to be checked out.
`examples/`, `e2e/`, and `compat/` are each their own workspace/package
specifically so that stays true.

## Quickstart (building from source)

```sh
cd js && pnpm install && pnpm build   # builds js/dist/playground.js
cd .. && cargo run -p rn-linux        # opens a window rendering it
```

`rn-linux` takes an optional bundle path (arg 1, or `RN_LINUX_BUNDLE`) to
point it at a bundle of your own, built against the same shims — see
`js/build.mjs`/`js/build-support.mjs` for the esbuild alias config.

## Status

Linux works end to end. Windows' architectural blocker — getting Hermes
to build under MSVC — is solved (see
[`docs/pitfalls/windows-msvc-build.md`](./docs/pitfalls/windows-msvc-build.md)),
a `rn-windows` binary hasn't been scaffolded yet.

## Contributing

See [`CLAUDE.md`](./CLAUDE.md) for the architecture and how to build/test
every part of the repo, and [`docs/pitfalls/`](./docs/pitfalls/) for the
non-obvious bugs already found and fixed — worth a skim before touching
the engine internals.

## License

MIT — see [`LICENSE`](./LICENSE).
