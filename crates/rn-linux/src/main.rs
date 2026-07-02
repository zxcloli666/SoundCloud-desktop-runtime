//! The engine's own default binary: runs whatever bundle it's pointed at,
//! with zero SoundCloud awareness — no `sc-rn` init, no extra host ops.
//! `pnpm build` in `js/` produces `js/dist/playground.js`, this engine's own
//! zero-dependency demo; point elsewhere via arg 1 or `RN_LINUX_BUNDLE` to
//! run any other bundle built against the same shims (e.g. `examples/
//! soundcloud/crates/sc-desktop-example` does exactly that, plus its own
//! SoundCloud-specific host ops — see rn_linux::RunConfig).

use std::path::PathBuf;

fn main() {
    let bundle_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("RN_LINUX_BUNDLE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../js/dist/playground.js")));

    rn_linux::run(rn_linux::RunConfig {
        bundle_path,
        ..Default::default()
    });
}
