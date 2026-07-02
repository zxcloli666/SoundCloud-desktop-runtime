# Desktop-Runtime

A React Native runtime for the desktop, built from real building blocks —
[Hermes](https://github.com/facebook/hermes) (the JS engine RN itself uses),
real [Yoga](https://github.com/facebook/yoga) flexbox layout, real
[Skia](https://skia.org) rendering, and the real
[`react-reconciler`](https://www.npmjs.com/package/react-reconciler) — glued
together with a thin Rust host, instead of vendoring Meta's Fabric C++.

It also ships compatibility shims for `react-native`,
[`@shopify/react-native-skia`](https://shopify.github.io/react-native-skia/),
and [`react-native-reanimated`](https://docs.swmansion.com/react-native-reanimated/),
so existing RN component libraries can run on it largely unmodified — see
[`CLAUDE.md`](./CLAUDE.md) for the full story, including every bug found
building it.

## Layout

```
crates/          the engine (Rust): skia-desktop, js-host, rn-linux
js/               the engine's JS half: react-native/skia/reanimated shims,
                  react-reconciler host-config, and a zero-dependency
                  "playground" demo
examples/soundcloud/   how SoundCloud itself uses this engine — needs
                        SoundCloud's own `Core` repo checked out as a
                        sibling of this one
e2e/              integration tests against the real SoundCloud example
                  (needs Core + examples/soundcloud built)
```

**`crates/` + `js/` have zero SoundCloud-specific dependencies.** `cargo
build`/`cargo test` at the repo root and `pnpm build`/`pnpm typecheck` in
`js/` work on a bare clone — nothing else needs to be checked out.
`examples/` and `e2e/` are each their own separate Cargo workspace
specifically so that stays true.

## Quickstart

```sh
cd js && pnpm install && pnpm build   # builds js/dist/playground.js
cd .. && cargo run -p rn-linux        # opens a window rendering it
```

`rn-linux` takes an optional bundle path (arg 1, or `RN_LINUX_BUNDLE`) if
you want to point it at a bundle of your own, built with the same shims —
see `js/build.mjs` and `js/build-support.mjs` for the esbuild config that
does the alias resolution.

## Status

Linux is working end to end. Windows' architectural blocker (getting Hermes
to build under MSVC) is solved — see `CLAUDE.md` — a `rn-windows` binary
hasn't been scaffolded yet.

## License

MIT — see [`LICENSE`](./LICENSE).
