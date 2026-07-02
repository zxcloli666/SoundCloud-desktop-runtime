//! Hermes embedded in our own Rust process via `rusty_hermes` (safe wrapper
//! over Hermes' JSI). Owns the mounted scene tree (`scene`) and the host
//! functions (`host`) a JS-side `react-reconciler` host-config calls into —
//! this is where Fabric's job (mounting + Yoga layout) happens for us.

pub mod host;
pub mod image_cache;
pub mod async_bridge;
pub mod scene;

// Re-exported so downstream host-function plugins (e.g. examples/soundcloud's
// `sc-desktop-ops`) can write `#[hermes_op(...)]` fns of their own without
// taking a direct dependency on `rusty_hermes` themselves — keeps exactly
// one pinned, compiled copy of it (a from-source, ~8-minute git dependency)
// across the whole repo tree, even though such plugins live in their own
// separate, nested Cargo workspace with their own `Cargo.lock`.
//
// The `self` re-export (not just the individual items) matters: the
// `#[hermes_op]` macro's generated code contains hardcoded, unqualified
// `rusty_hermes::...` paths (it has no re-export-aware hygiene) — those
// only resolve if a plugin crate brings the *crate itself* into scope
// under that exact name, via `use js_host::rusty_hermes;`, not just the
// handful of items re-exported by name below.
pub use rusty_hermes::{self, Error, Result, Runtime, hermes_op};
pub use scene::Scene;

/// Mirrors rn-linux's per-frame pump (drain due timers, then microtasks they
/// produced). `ConcurrentRoot`'s initial mount schedules its commit through
/// that same path instead of completing inline inside a single `eval()` call
/// (unlike the old `LegacyRoot` + forced `flushSync` setup) — tests that load
/// the real bundle need to pump a few frames before the scene tree exists.
#[cfg(test)]
fn pump_frames(rt: &Runtime, count: u32) {
    for _ in 0..count {
        rt.eval("if (typeof __scDrainTimers === 'function') __scDrainTimers();").expect("drain timers failed");
        rt.drain_microtasks().expect("drain microtasks failed");
    }
}

// Every test module below is a real Hermes/Yoga/Skia exercise, not a mock,
// split into files under `tests/` instead of one long `lib.rs`. All of
// them are zero-dependency: `bundle_test`/`reanimated_test`/
// `fills_arbitrary_aspect_ratio_test` load `js/dist/playground.js` — this
// crate's own synthetic, zero-@sc/ui demo bundle (js/playground/src/
// index.tsx), not the real SoundCloud one.
//
// The real `@sc/ui`-dependent tests (real sc_rn::auth_status() over the
// async bridge, real @sc/ui press/scroll contract checks) live in
// `e2e/tests/` instead — this crate has zero sc-rn awareness (see host.rs/
// Cargo.toml), so they can't compile-and-pass here.
#[cfg(test)]
#[path = "tests/smoke.rs"]
mod tests;
#[cfg(test)]
#[path = "tests/text_metrics.rs"]
mod text_metrics_test;
#[cfg(test)]
#[path = "tests/bundle_test.rs"]
mod bundle_test;
#[cfg(test)]
#[path = "tests/reanimated_test.rs"]
mod reanimated_test;
#[cfg(test)]
#[path = "tests/fills_arbitrary_aspect_ratio_test.rs"]
mod fills_arbitrary_aspect_ratio_test;
#[cfg(test)]
#[path = "tests/hermes_engine_bug.rs"]
mod hermes_for_of_let_closure_bug_test;
#[cfg(test)]
#[path = "tests/hit_test.rs"]
mod hit_test_test;
#[cfg(test)]
#[path = "tests/scroll.rs"]
mod scroll_test;
#[cfg(test)]
#[path = "tests/image_cache.rs"]
mod image_cache_test;
