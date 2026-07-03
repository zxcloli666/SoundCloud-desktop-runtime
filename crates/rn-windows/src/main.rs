//! The engine's own default binary for Windows: runs whatever bundle it's
//! pointed at, with zero SoundCloud awareness — no `sc-rn` init, no extra
//! host ops. Thin wrapper over this crate's own `lib.rs` (a re-export of
//! `rn_linux::run`/`RunConfig` — see its doc comment for why), same shape
//! as `rn-linux`'s own binary (`crates/rn-linux/src/main.rs`) calling its
//! own `lib.rs`. A separate crate/binary exists so Windows users get a
//! `rn-windows.exe` of their own, not a binary named after the wrong
//! platform.
//!
//! `pnpm build` in `js/` produces `js/dist/playground.js`, this engine's own
//! zero-dependency demo; point elsewhere via arg 1 or `RN_WINDOWS_BUNDLE` to
//! run any other bundle built against the same shims.

use std::path::PathBuf;

fn main() {
    let bundle_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("RN_WINDOWS_BUNDLE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../js/dist/playground.js")));

    rn_windows::run(rn_windows::RunConfig {
        bundle_path,
        window_title: "rn-windows — Hermes+Yoga+Skia engine".to_string(),
        ..Default::default()
    });
}
