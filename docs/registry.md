# Package registry

Rust crates (`skia-desktop`, `js-host`, `rn-linux`) and the JS engine
package (`js/`) aren't published to crates.io/npm — this repo hosts its
own.

## Why not crates.io / npm directly

- **Rust**: `js-host` depends on [`rusty_hermes`](https://github.com/zxcloli666/rusty_hermes)
  (a fork with real upstream build fixes — see `docs/pitfalls/
  windows-msvc-build.md`). `cargo package`/`cargo publish` categorically
  refuse a version-less git dependency, and upstream's own crates.io
  listing is yanked. Rather than depend on a third party unyanking a
  release we don't control, we publish `rusty_hermes` (and its two
  sub-crates, `libhermes-sys`/`rusty_hermes_macros`) to our own registry
  too — see below.
- **npm**: no blocker here, just simplicity — GitHub Packages needs no
  external account, only the `GITHUB_TOKEN` Actions already has.

## How it works

**npm** — plain [GitHub Packages](https://npm.pkg.github.com), the real
product, nothing custom. The engine's JS package is published as
`@zxcloli666/desktop-runtime-js`.

**Rust** — a self-hosted [sparse registry](https://doc.rust-lang.org/cargo/reference/registries.html)
(the same protocol crates.io itself uses, just not crates.io):
- The index (`ops/cargo-registry/config.json` + one JSON-lines file per
  crate, generated — never hand-edited) is deployed to this repo's GitHub
  Pages by `.github/workflows/publish.yml`.
- Each crate's packaged `.crate` tarball is attached as a GitHub Release
  asset, named to match the index's `dl` download template.
- `ops/cargo-registry/publish-crate.mjs` does the actual packaging +
  index-entry generation (via `cargo metadata`, not hand-parsed TOML) for
  one crate at a time, idempotently (safe to re-run a workflow).
- `ops/cargo-registry/serve-local.mjs` serves the in-progress index over
  plain HTTP for the DURATION of one publish run only — later crates in
  the same run (e.g. `js-host`, depending on the just-published
  `rusty_hermes`) need to resolve before the real GitHub Pages deploy
  exists yet. The public deploy happens once, at the end, from the final
  index state.

## Consuming it

Add to `.cargo/config.toml` (this repo already ships one at its own
root, for its own build):

```toml
[registries]
desktop-runtime = { index = "sparse+https://zxcloli666.github.io/SoundCloud-desktop-runtime/registry/" }
```

Then it's ordinary Cargo:

```toml
[dependencies]
js-host = { version = "0.1.0", registry = "desktop-runtime" }
rn-linux = { version = "0.1.0", registry = "desktop-runtime" }
```

or `cargo add js-host --registry desktop-runtime`. `js-host`'s own
dependency on `rusty_hermes` resolves transitively from the same
registry — nothing extra to configure for it.

The first build still compiles Hermes from source (~7-8 minutes on
Linux) — publishing only changes *where the source comes from*
(a versioned download instead of a live git clone), not what happens
after.

For the npm package:

```
npm config set @zxcloli666:registry https://npm.pkg.github.com
npm install @zxcloli666/desktop-runtime-js
```
