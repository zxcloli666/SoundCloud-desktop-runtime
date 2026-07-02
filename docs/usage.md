# Usage guide

How to depend on Desktop-Runtime from your own project: install, render a
minimal tree, write a bundle against the shims, and plug in your own host
functions. (Русская версия: [`usage.ru.md`](./usage.ru.md).)

## 0. The cross-platform story — bringing an existing RN app to desktop

If you already have a React Native app — iOS, Android, maybe macOS via
`react-native-macos` — using `@shopify/react-native-skia` for custom
drawing, Desktop-Runtime is how the *same* app reaches Windows and Linux
too, without a fork and without rewriting your components:

1. Your component code (`View`/`Text`/`Pressable`/`Canvas`/`Group`/
   `useSharedValue`/...) doesn't change. It already only talks to
   `react-native`/`@shopify/react-native-skia`/`react-native-reanimated`'s
   public API — that's the whole contract.
2. On mobile and macOS, those imports resolve to the real native modules,
   same as always.
3. On Windows/Linux, your build tags a *desktop* bundle where those same
   imports resolve to Desktop-Runtime's shims instead (section 3 below) —
   one esbuild `alias` entry, the same trick `react-native-web` uses for
   browser builds. Nothing in your component tree needs to know which
   target it's running on.
4. A small, desktop-only Rust binary (section 2) opens the window and
   hosts that bundle — this is new code you write once per app (it's your
   app's desktop entry point, Desktop-Runtime can't own that for you), not
   something you fork out of an existing project.

The result: one component codebase, five platforms. See
[`compat/README.md`](../compat/README.md) for exactly how "your Skia code
still works on Windows/Linux" gets verified, not just asserted.

## 1. Install

Rust crates and the JS package are published to this repo's own
registries — see [`registry.md`](./registry.md) for why not crates.io/npm
directly.

**`.cargo/config.toml`** (project-level or `~/.cargo/config.toml`):

```toml
[registries]
desktop-runtime = { index = "sparse+https://zxcloli666.github.io/SoundCloud-desktop-runtime/registry/" }
rusty-hermes-fork = { index = "sparse+https://zxcloli666.github.io/rusty_hermes/registry/" }
```

**`Cargo.toml`:**

```toml
[dependencies]
rn-linux = { version = "0.1.0", registry = "desktop-runtime" }     # Linux
rn-windows = { version = "0.1.0", registry = "desktop-runtime" }   # Windows
js-host = { version = "0.1.0", registry = "desktop-runtime" }
```

`rn-linux` and `rn-windows` expose the identical `run(RunConfig)` API
(`rn-windows` is a thin binary crate over the same platform-agnostic
`rn_linux::run` — nothing in the engine is actually Linux-specific, so
there's no separate `rn_windows::run` to learn); pick whichever matches
your `cargo build --target` and only depend on that one, or gate both
behind `[target.'cfg(windows)'.dependencies]` / `[target.'cfg(unix)'.
dependencies]` in your own `Cargo.toml` if you build for both from one
crate. `cargo add rn-linux --registry desktop-runtime` (or `rn-windows`).
`js-host`'s own
dependency on `rusty_hermes` (the Hermes binding, published from its own
repo's registry — see [`registry.md`](./registry.md)) resolves
transitively — nothing extra to configure beyond the two registries
above. The first build compiles Hermes from source, ~7-8 minutes on
Linux; later builds reuse the compiled artifact like any other
dependency.

**JS package** (the shims + react-reconciler host-config):

```sh
npm config set @zxcloli666:registry https://npm.pkg.github.com
npm install @zxcloli666/desktop-runtime-js
```

## 2. A minimal window

`rn-linux::run` takes a `RunConfig` and never returns — it opens a window,
evals your bundle, and drives the render loop. The whole public surface:

```rust
pub struct RunConfig {
    pub bundle_path: PathBuf,
    pub window_title: String,
    pub initial_size: (f64, f64),
    pub before_bundle_eval: Option<Box<dyn FnOnce(&js_host::Runtime) -> Result<(), String>>>,
}
```

```rust
fn main() {
    rn_linux::run(rn_linux::RunConfig {
        bundle_path: "dist/bundle.js".into(),
        window_title: "My App".to_string(),
        ..Default::default()
    });
}
```

`bundle_path` points at a JS bundle built against the engine's shims (next
section) — `rn-linux`/`rn-windows` don't care what's in it beyond that it
calls `react-reconciler`'s `updateContainer` against the host-config the
engine already registered.

## 3. Writing a bundle against the shims

Your app's JS imports `react-native` / `@shopify/react-native-skia` /
`react-native-reanimated` completely normally — an esbuild `alias` (the
same trick `react-native-web` uses) resolves them to the engine's shims at
bundle time, not the real native modules:

```js
// build.mjs
import * as esbuild from 'esbuild';

await esbuild.build({
  entryPoints: ['src/index.tsx'],
  bundle: true,
  outfile: 'dist/bundle.js',
  format: 'iife',       // Hermes has no module loader
  platform: 'neutral',
  mainFields: ['main'],
  target: 'es2020',
  jsx: 'automatic',
  define: { 'process.env.NODE_ENV': '"development"' },
  alias: {
    'react-native': 'node_modules/@zxcloli666/desktop-runtime-js/src/react-native.tsx',
    '@shopify/react-native-skia': 'node_modules/@zxcloli666/desktop-runtime-js/src/rnskia.tsx',
    'react-native-reanimated': 'node_modules/@zxcloli666/desktop-runtime-js/src/reanimated.tsx',
  },
});
```

Your entry point wires up `react-reconciler` against the engine's
host-config and mounts your tree — this is boilerplate every consumer
needs once, not something the engine can hide (the engine doesn't own
your React tree, you do):

```tsx
import React from 'react';
import Reconciler from 'react-reconciler';
import { ConcurrentRoot } from 'react-reconciler/constants';
import { hostConfig } from '@zxcloli666/desktop-runtime-js/src/hostConfig';
import { Text, View } from 'react-native';

const Renderer = Reconciler(hostConfig);

function App() {
  return (
    <View style={{ backgroundColor: [0.04, 0.05, 0.08, 1.0] }}>
      <Text style={{ color: [1, 1, 1, 1], margin: 16 }}>Hello, Desktop-Runtime</Text>
    </View>
  );
}

const root = Renderer.createContainer(
  { rootId: null }, ConcurrentRoot, null, false, null, '',
  (e) => { throw e; }, (e) => { throw e; }, (e) => { throw e; }, null,
);
Renderer.updateContainer(<App />, root, null, null);
```

For a fuller, real example (pressable tiles, a scrollable list, a
`withTiming` animation), see `js/playground/src/index.tsx` in this repo —
it's exactly this pattern, just exercising more of the shim surface.

Colors are `[r, g, b, a]` tuples (0-1 floats) or CSS strings
(`"#5a8cff"`, `"rgba(0,0,0,0.35)"`) — both work, same as real React
Native.

## 4. Your own host functions

If your app needs native capabilities beyond rendering (auth, local data,
platform APIs — whatever your app is for), register your own
`js_host::hermes_op` functions on top of the engine's 16 generic ones, via
`RunConfig::before_bundle_eval`:

```rust
use js_host::hermes_op;

#[hermes_op(name = "__myGetVersion")]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn main() {
    rn_linux::run(rn_linux::RunConfig {
        bundle_path: "dist/bundle.js".into(),
        before_bundle_eval: Some(Box::new(|rt| {
            get_version::register(rt).map_err(|e| e.to_string())
        })),
        ..Default::default()
    });
}
```

`before_bundle_eval` runs once, after the engine's generic ops are
registered but before your bundle is read — the right place for one-time
setup too (opening a database, reading config, whatever your app needs
before any JS runs). Async host functions that shouldn't block the render
thread can use `js_host::async_bridge::spawn_call` — see
`examples/soundcloud/crates/sc-desktop-ops` in this repo for a complete,
real example (SoundCloud's own auth/data host functions), and
`examples/soundcloud/crates/sc-desktop-example` for how it wires into
`RunConfig`.

## 5. Compatibility and limitations

See the [compatibility table](../README.md#compatibility) for which
`react-native`/`@shopify/react-native-skia`/`react-native-reanimated`
versions the shims are verified against, and
[`docs/pitfalls/`](./pitfalls/) for real bugs already found and fixed —
worth a skim if something behaves unexpectedly.

`rn-windows` builds and renders on the same platform-agnostic engine as
`rn-linux` (nothing OS-specific in `crates/` beyond `winit`/`glutin`/
`skia-safe`/`rusty_hermes` themselves, all of which already support
Windows upstream — see `docs/pitfalls/windows-msvc-build.md` for the one
thing that genuinely was Windows-specific: getting Hermes itself through
MSVC).
