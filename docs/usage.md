# Usage guide

Step-by-step: take your existing React Native app (iOS/Android, maybe
macOS via `react-native-macos`) and add Windows + Linux, from the same
component code, in one repo. (Русская версия: [`usage.ru.md`](./usage.ru.md).
Why this exists at all: [README.md](../README.md).)

## What you'll end up with

```
my-app/                    your existing React Native app — UNCHANGED
  src/                     shared components (e.g. App.tsx) — same files
                           every platform renders, no Desktop-Runtime import
                           in sight
  index.js  ios/  android/  macos/  package.json    untouched, no Cargo

  desktop/                 NEW — the only folder that knows Desktop-Runtime exists
    js/
      package.json
      build.mjs             bundles src/index.tsx -> dist/bundle.js
      src/
        index.tsx           desktop-only bootstrap: wires react-reconciler to
                           the engine, renders your shared App from ../../../src
    Cargo.toml              its own [workspace]
    windows/                depends on rn-windows
      Cargo.toml
      src/main.rs
    linux/                  depends on rn-linux
      Cargo.toml
      src/main.rs
```

`ios/`, `android/`, `macos/` never change and never see Cargo. Your
shared `src/` stays plain React Native components — the react-reconciler
bootstrap that's specific to Desktop-Runtime lives entirely inside
`desktop/js/src/index.tsx`, a separate file, not mixed into it. Everything
below happens inside `desktop/`.

## Step 1 — Add the two registries

Rust crates and the JS package are hosted on this repo's own registries
(why not crates.io/npm: [`registry.md`](./registry.md)). Add both,
once, to `~/.cargo/config.toml` (or `desktop/.cargo/config.toml` for a
project-local setup):

```toml
[registries]
desktop-runtime = { index = "sparse+https://zxcloli666.github.io/SoundCloud-desktop-runtime/registry/" }
rusty-hermes-fork = { index = "sparse+https://zxcloli666.github.io/rusty_hermes/registry/" }
```

## Step 2 — Scaffold `desktop/`

```sh
mkdir -p desktop/js/src desktop/windows/src desktop/linux/src
```

`desktop/Cargo.toml` — a workspace of its own, so `cargo build` anywhere
else in your repo never touches it:

```toml
[workspace]
members = ["windows", "linux"]
resolver = "2"
```

## Step 3 — Add the Rust crate for each platform

`desktop/windows/Cargo.toml`:

```toml
[package]
name = "my-app-windows"
version = "0.1.0"
edition = "2021"

[dependencies]
rn-windows = { version = "0.1.0", registry = "desktop-runtime" }
```

`desktop/linux/Cargo.toml` — identical, except:

```toml
[dependencies]
rn-linux = { version = "0.1.0", registry = "desktop-runtime" }
```

Each binary only ever depends on its own platform's crate — building
`desktop/linux` never pulls in `rn-windows`, and vice versa.

## Step 4 — Write `main.rs`

Same content in both `desktop/windows/src/main.rs` and
`desktop/linux/src/main.rs` (swap `rn_windows`/`rn_linux` for the crate
you added in step 3 — both expose the identical `run(RunConfig)`):

```rust
fn main() {
    rn_linux::run(rn_linux::RunConfig {
        bundle_path: "../js/dist/bundle.js".into(),
        window_title: "My App".to_string(),
        ..Default::default()
    });
}
```

`RunConfig`'s full surface:

```rust
pub struct RunConfig {
    pub bundle_path: PathBuf,
    pub window_title: String,
    pub initial_size: (f64, f64),
    pub before_bundle_eval: Option<Box<dyn FnOnce(&js_host::Runtime) -> Result<(), String>>>,
}
```

## Step 5 — Install the JS shim package

```sh
cd desktop/js
npm config set @zxcloli666:registry https://npm.pkg.github.com
npm install @zxcloli666/desktop-runtime-js esbuild
```

## Step 6 — Write `build.mjs`

This is the one piece of magic: it makes your app's normal
`react-native`/`@shopify/react-native-skia`/`react-native-reanimated`
imports resolve to Desktop-Runtime's shims instead of the real native
modules, at bundle time — your component code never imports anything
Desktop-Runtime-specific.

```js
// desktop/js/build.mjs
import * as esbuild from 'esbuild';

await esbuild.build({
  entryPoints: ['src/index.tsx'],   // desktop/js/src/index.tsx — step 7, NOT your app's shared src/
  bundle: true,
  outfile: 'dist/bundle.js',
  format: 'iife',          // Hermes has no module loader
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

## Step 7 — Write `desktop/js/src/index.tsx`

This file — not anything in your app's shared `src/` — is where
Desktop-Runtime-specific code lives: it hands your React tree to the
engine. Every consumer writes this once (the engine can't do it for you,
since it doesn't own your tree):

```tsx
// desktop/js/src/index.tsx
import React from 'react';
import Reconciler from 'react-reconciler';
import { ConcurrentRoot } from 'react-reconciler/constants';
import { hostConfig } from '@zxcloli666/desktop-runtime-js/src/hostConfig';

const Renderer = Reconciler(hostConfig);

// Your actual, shared UI — the same component Metro renders for
// iOS/Android/macOS. It imports only plain react-native/
// @shopify/react-native-skia/react-native-reanimated, nothing
// Desktop-Runtime-specific — that's what makes it shared in the first
// place.
import { App } from '../../../src/App';

const root = Renderer.createContainer(
  { rootId: null }, ConcurrentRoot, null, false, null, '',
  (e) => { throw e; }, (e) => { throw e; }, (e) => { throw e; }, null,
);
Renderer.updateContainer(<App />, root, null, null);
```

If you're starting from scratch and don't have a shared `App` yet, a
trivial one to prove the pipeline works:

```tsx
// src/App.tsx (your app's shared root — no Desktop-Runtime import here)
import React from 'react';
import { Text, View } from 'react-native';

export function App() {
  return (
    <View style={{ backgroundColor: [0.04, 0.05, 0.08, 1.0] }}>
      <Text style={{ color: [1, 1, 1, 1], margin: 16 }}>Hello, Desktop-Runtime</Text>
    </View>
  );
}
```

Colors are `[r, g, b, a]` tuples (0-1 floats) or CSS strings
(`"#5a8cff"`, `"rgba(0,0,0,0.35)"`) — both work, same as real React
Native.

A fuller example (pressable tiles, a scrollable list, a `withTiming`
animation) using the same pattern: `js/playground/src/index.tsx` in this
repo — it plays the role of `desktop/js/src/index.tsx` above, just with
its own zero-dependency demo components instead of an imported shared
`App`.

## Step 8 — Build and run

```sh
cd desktop/js && pnpm install && node build.mjs   # -> dist/bundle.js

# Windows:
cd desktop/windows && cargo run --release

# Linux:
cd desktop/linux && cargo run --release
```

First build compiles Hermes from source (~7-8 minutes); later builds
reuse it like any other dependency. The same `dist/bundle.js` runs on
both — you don't rebuild it per platform.

## Step 9 (optional) — Your own native functions

If your app needs native capabilities beyond rendering (auth, local
storage, whatever), register your own host function on top of the
engine's 16 built-in ones:

```rust
use js_host::hermes_op;

#[hermes_op(name = "__myGetVersion")]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn main() {
    rn_linux::run(rn_linux::RunConfig {
        bundle_path: "../js/dist/bundle.js".into(),
        before_bundle_eval: Some(Box::new(|rt| {
            get_version::register(rt).map_err(|e| e.to_string())
        })),
        ..Default::default()
    });
}
```

`before_bundle_eval` runs once, before your bundle is read — also the
right place for other one-time setup (opening a database, reading
config). For something async that shouldn't block the render thread, use
`js_host::async_bridge::spawn_call` — see
`examples/soundcloud/crates/sc-desktop-ops` in this repo for a full real
example, and `examples/soundcloud/crates/sc-desktop-example` for how it's
wired into `RunConfig`.

## Reference

- **Compatibility**: [compatibility table](../README.md#compatibility) —
  which `react-native`/`@shopify/react-native-skia`/`react-native-reanimated`
  versions the shims are verified against.
- **Known bugs already found and fixed**: [`docs/pitfalls/`](./pitfalls/)
  — worth a skim if something behaves unexpectedly.
- **Windows specifically**: `rn-windows` runs on the identical engine as
  `rn-linux` — nothing in `crates/` is actually OS-specific beyond
  `winit`/`glutin`/`skia-safe`/`rusty_hermes` themselves, which already
  support Windows upstream. The one genuinely Windows-specific piece —
  getting Hermes itself through MSVC — is covered in
  [`docs/pitfalls/windows-msvc-build.md`](./pitfalls/windows-msvc-build.md).
