//! Re-exports `rn_linux`'s `run`/`RunConfig` under this crate's own name —
//! nothing in the engine is actually Linux-specific (see `crates/rn-linux/
//! src/lib.rs`'s own doc comment), so there's no separate implementation
//! to maintain here. This is the crate's real public API: without a `lib`
//! target, `rn-windows` was bin-only and couldn't be depended on as a
//! library at all (`cargo` would silently ignore it: "missing a lib
//! target") — every consumer calling `rn_windows::run(rn_windows::
//! RunConfig { .. })` per `docs/usage.md` needs this to actually exist.

pub use rn_linux::{RunConfig, run};
