# Usage guide

How to depend on Desktop-Runtime from your own project: install, render a
minimal tree, write a bundle against the shims, and plug in your own host
functions. (Русская версия: [`usage.ru.md`](./usage.ru.md).)

## 1. Install

Rust crates and the JS package are published to this repo's own
registries — see [`registry.md`](./registry.md) for why not crates.io/npm
directly.

**`.cargo/config.toml`** (project-level or `~/.cargo/config.toml`):

```toml
[registries]
desktop-runtime = { index = "sparse+https://zxcloli666.github.io/SoundCloud-desktop-runtime/registry/" }
```

**`Cargo.toml`:**

```toml
[dependencies]
rn-linux = { version = "0.1.0", registry = "desktop-runtime" }
js-host = { version = "0.1.0", registry = "desktop-runtime" }
```

or `cargo add rn-linux --registry desktop-runtime`. `js-host`'s own
dependency on `rusty_hermes` (the Hermes binding) resolves transitively
from the same registry — nothing extra to configure. The first build
compiles Hermes from source, ~7-8 minutes on Linux; later builds reuse
the compiled artifact like any other dependency.

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
section) — `rn-linux` doesn't care what's in it beyond that it calls
`react-reconciler`'s `updateContainer` against the host-config the engine
already registered.

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
`js_host::hermes_op` functions on top of the engine's 15 generic ones, via
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

Known gaps, not yet implemented:
- `<Image resizeMode="repeat">` falls back to `cover` (no tiling).
- Numeric `require()`'d image assets render as an empty box (no
  bundler-level asset pipeline) — `source={{ uri }}` works fully,
  including real network fetch + decode.
- List reordering (`insertBefore` with a real `beforeChild`) isn't
  implemented — append-only. Fine for the vast majority of UI, not fine
  for drag-to-reorder lists.
- Windows: the architectural blocker (Hermes under MSVC) is solved, but
  there's no `rn-windows` binary yet — only `rn-linux` today.
