//! The SoundCloud-specific host functions — the plugin half of the engine/
//! example split. `js-host` itself (the engine) knows nothing about
//! `sc-rn`; this crate is the ONLY place in
//! the whole repo tree where `sc-rn` is reachable, and it plugs its ops in
//! from the outside, the same way any third-party consumer of the engine
//! would add their own: register more `js_host::hermes_op` functions on a
//! `js_host::Runtime` that already has the 16 generic ops installed.
//!
//! `home_clusters`/`wave`/`resolve_tracks`/`me`/`auth_status` are async:
//! they spawn onto `js_host::async_bridge`'s background runtime (generic —
//! it never imports `sc_rn`) and report back through `async_bridge::drain()`,
//! polled once per frame by `sc-desktop-example`'s render loop, never
//! blocking it. `languages`/`urns` come in as a JSON-encoded string
//! (`hermes_op`'s `FromJsArg` only covers scalars — see
//! examples/soundcloud/js/src/live-data.ts).

mod dto_json;

// `hermes_op`'s generated code contains hardcoded, unqualified
// `rusty_hermes::...` paths — this `use` brings the crate itself into local
// scope under that exact name (re-exported by `js_host`, see its lib.rs),
// which is what actually makes those paths resolve; a real, direct
// `rusty_hermes` Cargo dependency would work too, but would mean this
// nested workspace compiles its own second, independently-pinned copy of a
// from-source, ~8-minute git dependency.
use js_host::rusty_hermes;
use js_host::hermes_op;

#[hermes_op(name = "__scInitCore")]
fn init_core(data_dir: String, cache_dir: String, dpi_bypass: bool) -> String {
    match sc_rn::init_runtime(data_dir, cache_dir, dpi_bypass) {
        Ok(()) => String::new(),
        Err(e) => e.to_string(),
    }
}

#[hermes_op(name = "__scSetSession")]
fn set_session(token: String) -> String {
    let token = if token.is_empty() { None } else { Some(token) };
    match sc_rn::set_session(token) {
        Ok(()) => String::new(),
        Err(e) => e.to_string(),
    }
}

#[hermes_op(name = "__scAuthStatus")]
fn auth_status(callback_id: u32) {
    js_host::async_bridge::spawn_call(callback_id, async move {
        sc_rn::auth_status().await.map(|dto| dto_json::auth_status(&dto).to_string()).map_err(|e| e.to_string())
    });
}

#[hermes_op(name = "__scMe")]
fn me(callback_id: u32) {
    js_host::async_bridge::spawn_call(callback_id, async move {
        sc_rn::me().await.map(|dto| dto_json::me(&dto).to_string()).map_err(|e| e.to_string())
    });
}

#[hermes_op(name = "__scHomeClusters")]
fn home_clusters(callback_id: u32, limit: u32, languages_json: String, hide_listened: bool) {
    let languages: Vec<String> = serde_json::from_str(&languages_json).unwrap_or_default();
    js_host::async_bridge::spawn_call(callback_id, async move {
        sc_rn::home_clusters(limit, languages, hide_listened)
            .await
            .map(|clusters| serde_json::Value::Array(clusters.iter().map(dto_json::cluster).collect()).to_string())
            .map_err(|e| e.to_string())
    });
}

#[hermes_op(name = "__scWave")]
fn wave(callback_id: u32, limit: u32, cursor: String, languages_json: String, hide_listened: bool) {
    let cursor = if cursor.is_empty() { None } else { Some(cursor) };
    let languages: Vec<String> = serde_json::from_str(&languages_json).unwrap_or_default();
    js_host::async_bridge::spawn_call(callback_id, async move {
        sc_rn::wave(limit, cursor, languages, hide_listened)
            .await
            .map(|dto| dto_json::wave(&dto).to_string())
            .map_err(|e| e.to_string())
    });
}

#[hermes_op(name = "__scResolveTracks")]
fn resolve_tracks(callback_id: u32, urns_json: String) {
    let urns: Vec<String> = serde_json::from_str(&urns_json).unwrap_or_default();
    js_host::async_bridge::spawn_call(callback_id, async move {
        sc_rn::resolve_tracks(urns)
            .await
            .map(|tracks| serde_json::Value::Array(tracks.iter().map(dto_json::track).collect()).to_string())
            .map_err(|e| e.to_string())
    });
}

/// Registers all 7 SoundCloud-specific ops on top of an already-`js_host::
/// host::install`ed `Runtime` — mirrors that function's own shape 1:1, just
/// for the ops this crate owns instead of the engine's generic ones.
pub fn install(rt: &js_host::Runtime) -> js_host::Result<()> {
    init_core::register(rt)?;
    set_session::register(rt)?;
    auth_status::register(rt)?;
    me::register(rt)?;
    home_clusters::register(rt)?;
    wave::register(rt)?;
    resolve_tracks::register(rt)?;
    Ok(())
}
