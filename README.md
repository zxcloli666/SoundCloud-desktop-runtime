# Desktop-Runtime

**React Native, on every platform — Desktop-Runtime closes the last two
gaps: Linux and Windows.**

[![CI](https://github.com/zxcloli666/SoundCloud-desktop-runtime/actions/workflows/ci.yml/badge.svg)](https://github.com/zxcloli666/SoundCloud-desktop-runtime/actions/workflows/ci.yml)
[![Compatibility check](https://github.com/zxcloli666/SoundCloud-desktop-runtime/actions/workflows/compat-check.yml/badge.svg)](https://github.com/zxcloli666/SoundCloud-desktop-runtime/actions/workflows/compat-check.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)

## One React Native codebase, every platform

React Native — including [`@shopify/react-native-skia`](https://shopify.github.io/react-native-skia/)-based
UI — already reaches iOS, Android, and macOS natively. Two platforms were
still missing a Skia-capable host. Desktop-Runtime fills exactly those:

| Platform | Coverage | Notes |
| --- | :---: | --- |
| 📱 iOS | ✅ | React Native, native — nothing to add |
| 🤖 Android | ✅ | React Native, native — nothing to add |
| 🖥️ macOS | ✅ | React Native + `@shopify/react-native-skia` both already ship a macOS target |
| 🪟 Windows | ✅ **via Desktop-Runtime** | `react-native-windows` exists and is Microsoft-maintained, but `@shopify/react-native-skia` has no Windows backend — Desktop-Runtime supplies one |
| 🐧 Linux | ✅ **via Desktop-Runtime** | No official React Native distribution exists for Linux at all — Desktop-Runtime is a complete, from-scratch host |

Same JS, same components, same Skia drawing code — the platforms that had
no implementation now do.

## How

Desktop-Runtime is built from the same real engines React Native itself
runs on — [Hermes](https://github.com/facebook/hermes) (the JS engine),
real [Yoga](https://github.com/facebook/yoga) flexbox layout, real
[Skia](https://skia.org) GPU rendering, and the real
[`react-reconciler`](https://www.npmjs.com/package/react-reconciler) —
instead of a browser (Electron/Tauri/any webview) or a from-scratch UI
toolkit. `react-native`, `@shopify/react-native-skia`, and
`react-native-reanimated` imports resolve to drop-in shims at bundle time
(the same trick `react-native-web` uses to swap in a DOM-backed
`react-native`), so existing component code runs largely unmodified.

It deliberately doesn't vendor Meta's Fabric C++ — the shadow-tree/
scheduler is the least documented, most tightly coupled part of React
Native's internals; even `react-native-windows` reimplements its own
rather than reusing it. Real Yoga + real Hermes + real Skia + the real
`react-reconciler` package, glued by a small host-config, gets the same
result with far less to maintain — and it's why Windows gets the same
clean engine as Linux instead of a patch bolted onto Fabric.

## Compatibility

<!-- COMPAT_TABLE:START -->
| Package | Status | Current | Verified since | Track record |
| --- | :---: | --- | --- | --- |
| `react-native` | ✅ | `0.86.0` | 2026-07-02 | `0.86.0` |
| `@shopify/react-native-skia` | ✅ | `2.6.9` | 2026-07-02 | `2.6.9` |
| `react-native-reanimated` | ✅ | `4.5.1` | 2026-07-02 | `4.5.0` → `4.5.1` |
| `react-reconciler` | ✅ | `0.33.0` | 2026-07-02 | `0.33.0` |
<!-- COMPAT_TABLE:END -->

Kept current automatically — [`.github/workflows/compat-check.yml`](.github/workflows/compat-check.yml)
watches upstream weekly and re-verifies before bumping anything. ✅ means
the shims still structurally type-check against that exact upstream
version and the full test suite (including a real, unmodified
consumer app) still passes — see [`compat/README.md`](./compat/README.md)
for precisely what that does and doesn't prove.

## Install

```toml
# .cargo/config.toml
[registries]
desktop-runtime = { index = "sparse+https://zxcloli666.github.io/SoundCloud-desktop-runtime/registry/" }
rusty-hermes-fork = { index = "sparse+https://zxcloli666.github.io/rusty_hermes/registry/" }
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

**Already have a React Native app?** [`docs/usage.md`](./docs/usage.md)
([русская версия](./docs/usage.ru.md)) walks through pointing your
existing `@shopify/react-native-skia` codebase at Desktop-Runtime for
Windows/Linux, alongside your existing iOS/Android/macOS targets — one
codebase, not a fork.

## Quickstart (building from source)

```sh
cd js && pnpm install && pnpm build   # builds js/dist/playground.js
cd .. && cargo run -p rn-linux        # opens a window rendering it
```

## License

MIT — see [`LICENSE`](./LICENSE).
