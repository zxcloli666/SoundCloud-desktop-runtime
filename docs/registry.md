# Package registry

Rust crates (`skia-desktop`, `js-host`, `rn-linux`) and the JS engine
package (`js/`) aren't published to crates.io/npm — this repo hosts its
own. `rusty_hermes` (a dependency, not this repo's own code) is hosted
from **its own repo's** registry, never this one's — every repo's
releases page only ever carries its own artifacts.

## Why not crates.io / npm directly

- **Rust**: `js-host` depends on [`rusty_hermes`](https://github.com/zxcloli666/rusty_hermes)
  (a fork with real upstream build fixes — see `docs/pitfalls/
  windows-msvc-build.md`). `cargo package`/`cargo publish` categorically
  refuse a version-less git dependency, and upstream's own crates.io
  listing is yanked. `rusty_hermes` publishes itself to its own registry
  instead (below).
- **npm**: no blocker here, just simplicity — GitHub Packages needs no
  external account, only the `GITHUB_TOKEN` Actions already has.

## Two separate Rust registries, one per repo

| Registry | Hosted from | Hosts |
| --- | --- | --- |
| `desktop-runtime` | `zxcloli666/SoundCloud-desktop-runtime` (this repo)'s Pages + Releases | `skia-desktop`, `js-host`, `rn-linux` |
| `rusty-hermes-fork` | `zxcloli666/rusty_hermes`'s own Pages + Releases | `rusty_hermes`, `libhermes-sys`, `rusty_hermes_macros` |

`js-host`'s dependency on `rusty_hermes` points at the `rusty-hermes-fork`
registry explicitly (`registry = "rusty-hermes-fork"` in its
`Cargo.toml`) — Cargo resolves it transitively, consumers don't need to
think about it beyond having both registries configured (already shipped
in this repo's own `.cargo/config.toml`).

Both are [sparse registries](https://doc.rust-lang.org/cargo/reference/registries.html)
(the same protocol crates.io itself uses, just self-hosted): a
`config.json` + one JSON-lines index file per crate, deployed to the
owning repo's GitHub Pages; `.crate` tarballs attached as GitHub Release
assets on that same repo, named to match the index's `dl` download
template. Since a static index has no publish API,
[`ops/cargo-registry/publish-crate.mjs`](../ops/cargo-registry/publish-crate.mjs)
hand-builds the index entries (via `cargo metadata`, not hand-parsed
TOML) instead of calling `cargo publish` — idempotent, safe to re-run a
workflow. [`ops/cargo-registry/serve-local.mjs`](../ops/cargo-registry/serve-local.mjs)
serves the in-progress index over plain HTTP for the duration of one
publish run only, so crates that depend on each other (e.g. `rn-linux`
on `js-host`) can resolve mid-run without waiting on real Pages
propagation — the real deploy happens once, at the end.

## npm

Plain [GitHub Packages](https://npm.pkg.github.com), the real product,
nothing custom. The engine's JS package publishes as
`@zxcloli666/desktop-runtime-js`.

## Consuming it

Add to `.cargo/config.toml` (this repo already ships one at its own
root, for its own build — copy both lines):

```toml
[registries]
desktop-runtime = { index = "sparse+https://zxcloli666.github.io/SoundCloud-desktop-runtime/registry/" }
rusty-hermes-fork = { index = "sparse+https://zxcloli666.github.io/rusty_hermes/registry/" }
```

Then it's ordinary Cargo:

```toml
[dependencies]
js-host = { version = "0.1.0", registry = "desktop-runtime" }
rn-linux = { version = "0.1.0", registry = "desktop-runtime" }
```

or `cargo add js-host --registry desktop-runtime`. `rusty_hermes`
resolves transitively — nothing extra to configure for it.

The first build still compiles Hermes from source (~7-8 minutes on
Linux) — publishing only changes *where the source comes from*
(a versioned download instead of a live git clone), not what happens
after.

For the npm package:

```
npm config set @zxcloli666:registry https://npm.pkg.github.com
npm install @zxcloli666/desktop-runtime-js
```
