//! Bridges Hermes host functions to the `Scene`. Hermes runs single-threaded,
//! so a thread-local holds the one scene per process — no closure-captured
//! state or unsafe context pointers needed.

use std::cell::RefCell;

use rusty_hermes::{Runtime, hermes_op};

use crate::scene::{Scene, StyleInput};

thread_local! {
    static SCENE: RefCell<Scene> = RefCell::new(Scene::new());
}

/// Read/mutate the scene from outside Hermes (e.g. the render loop, after
/// `Runtime::eval()` has run the JS that builds/updates the tree). This is our
/// own bundled application JS, not untrusted input — there's no code-injection
/// concern here, `eval()` is simply Hermes' script-execution entry point.
pub fn with_scene<R>(f: impl FnOnce(&mut Scene) -> R) -> R {
    SCENE.with(|s| f(&mut s.borrow_mut()))
}

#[hermes_op(name = "__scCreateView")]
fn create_view() -> u32 {
    SCENE.with(|s| s.borrow_mut().create_view())
}

#[hermes_op(name = "__scCreateText")]
fn create_text(text: String) -> u32 {
    SCENE.with(|s| s.borrow_mut().create_text(text))
}

#[hermes_op(name = "__scSetText")]
fn set_text(id: u32, text: String) {
    SCENE.with(|s| s.borrow_mut().set_text(id, text));
}

#[hermes_op(name = "__scCreateSkNode")]
fn create_sk_node(kind: String) -> u32 {
    SCENE.with(|s| s.borrow_mut().create_sk_node(&kind))
}

#[hermes_op(name = "__scSetSkProps")]
fn set_sk_props(id: u32, props_json: String) {
    let props: serde_json::Value = serde_json::from_str(&props_json)
        .unwrap_or_else(|e| panic!("invalid Skia node props JSON for node {id}: {e}"));
    SCENE.with(|s| s.borrow_mut().set_sk_props(id, props));
}

#[hermes_op(name = "__scAppendChild")]
fn append_child(parent: u32, child: u32) {
    SCENE.with(|s| s.borrow_mut().append_child(parent, child));
}

#[hermes_op(name = "__scRemoveChild")]
fn remove_child(parent: u32, child: u32) {
    SCENE.with(|s| s.borrow_mut().remove_child(parent, child));
}

#[hermes_op(name = "__scSetStyle")]
fn set_style(id: u32, style_json: String) {
    let style: StyleInput = serde_json::from_str(&style_json)
        .unwrap_or_else(|e| panic!("invalid style JSON for node {id}: {e}"));
    SCENE.with(|s| s.borrow_mut().set_style(id, style));
}

#[hermes_op(name = "__scSetRoot")]
fn set_root(id: u32) {
    SCENE.with(|s| s.borrow_mut().set_root(id));
}

#[hermes_op(name = "__scConsoleLog")]
fn console_log(message: String) {
    println!("[js] {message}");
}

// --- sc-rn (Core/shared) live data — spike 7b ------------------------------
//
// `home_clusters`/`wave`/`resolve_tracks`/`me`/`auth_status` are async: they
// spawn onto `crate::live_data`'s background runtime and report back through
// `live_data::drain()`, polled once per frame by rn-linux — never block this
// thread. `languages`/`urns` come in as a JSON-encoded string (`hermes_op`'s
// `FromJsArg` only covers scalars — see `js/src/live-data.ts`).

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
    crate::live_data::spawn_call(callback_id, async move {
        sc_rn::auth_status().await.map(|dto| crate::dto_json::auth_status(&dto).to_string()).map_err(|e| e.to_string())
    });
}

#[hermes_op(name = "__scMe")]
fn me(callback_id: u32) {
    crate::live_data::spawn_call(callback_id, async move {
        sc_rn::me().await.map(|dto| crate::dto_json::me(&dto).to_string()).map_err(|e| e.to_string())
    });
}

#[hermes_op(name = "__scHomeClusters")]
fn home_clusters(callback_id: u32, limit: u32, languages_json: String, hide_listened: bool) {
    let languages: Vec<String> = serde_json::from_str(&languages_json).unwrap_or_default();
    crate::live_data::spawn_call(callback_id, async move {
        sc_rn::home_clusters(limit, languages, hide_listened)
            .await
            .map(|clusters| serde_json::Value::Array(clusters.iter().map(crate::dto_json::cluster).collect()).to_string())
            .map_err(|e| e.to_string())
    });
}

#[hermes_op(name = "__scWave")]
fn wave(callback_id: u32, limit: u32, cursor: String, languages_json: String, hide_listened: bool) {
    let cursor = if cursor.is_empty() { None } else { Some(cursor) };
    let languages: Vec<String> = serde_json::from_str(&languages_json).unwrap_or_default();
    crate::live_data::spawn_call(callback_id, async move {
        sc_rn::wave(limit, cursor, languages, hide_listened)
            .await
            .map(|dto| crate::dto_json::wave(&dto).to_string())
            .map_err(|e| e.to_string())
    });
}

#[hermes_op(name = "__scResolveTracks")]
fn resolve_tracks(callback_id: u32, urns_json: String) {
    let urns: Vec<String> = serde_json::from_str(&urns_json).unwrap_or_default();
    crate::live_data::spawn_call(callback_id, async move {
        sc_rn::resolve_tracks(urns)
            .await
            .map(|tracks| serde_json::Value::Array(tracks.iter().map(crate::dto_json::track).collect()).to_string())
            .map_err(|e| e.to_string())
    });
}

/// Shims for host globals bare Hermes doesn't provide (no browser, no Node).
///
/// `setTimeout`/`setImmediate` must NOT run their callback inline — the
/// scheduler package (which react-reconciler depends on) picks one of these
/// as its "yield to a fresh task" primitive specifically so it can schedule
/// work from *inside* a commit (e.g. a `useEffect` calling `setState`)
/// without re-entering the reconciler on the same call stack. An
/// inline-synchronous shim defeats that: `scheduleUpdateOnFiber` re-enters
/// `performWorkOnRoot` while the outer commit is still on the stack, and
/// React's own reentrancy guard throws "Should not already be working" —
/// silently, since it surfaces as a swallowed microtask-error console log,
/// not a crash, so any update after the initial mount just goes missing.
/// Real timers defer to a queue instead, drained once per frame by
/// `__scDrainTimers()` (called from rn-linux's render loop, alongside the
/// reanimated tick and the live-data microtask/async drains) — a fresh,
/// un-nested call, exactly like a real event loop's next task.
const PRELUDE_JS: &str = r#"
(function () {
    var nextTimerId = 1;
    var timers = new Map();

    function schedule(fn, delayMs, intervalMs) {
        var id = nextTimerId++;
        timers.set(id, { fireAt: Date.now() + (delayMs || 0), fn: fn, intervalMs: intervalMs });
        return id;
    }

    globalThis.setTimeout = function (fn, delayMs) { return schedule(fn, delayMs, null); };
    globalThis.clearTimeout = function (id) { timers.delete(id); };
    globalThis.setInterval = function (fn, delayMs) { return schedule(fn, delayMs, delayMs || 0); };
    globalThis.clearInterval = function (id) { timers.delete(id); };
    // Hermes' own Promise internals reach for a host `setImmediate` when
    // scheduling `.then()` jobs — without it, `Promise.resolve().then()`
    // throws. Due "now" (not later), but still deferred to the next drain.
    globalThis.setImmediate = function (fn) { return schedule(fn, 0, null); };
    globalThis.clearImmediate = function (id) { timers.delete(id); };

    globalThis.__scDrainTimers = function () {
        var now = Date.now();
        timers.forEach(function (timer, id) {
            if (timer.fireAt > now) return;
            if (timer.intervalMs !== null) {
                timer.fireAt = now + timer.intervalMs;
            } else {
                timers.delete(id);
            }
            try {
                timer.fn();
            } catch (e) {
                console.error('timer error: ' + ((e && e.stack) || e));
            }
        });
    };
})();
globalThis.queueMicrotask = globalThis.queueMicrotask || function (fn) {
    Promise.resolve().then(function () {
        // A throw here is an unhandled rejection — silent by default. Surface
        // it, since this is where react-reconciler's commit work runs.
        try { fn(); } catch (e) { console.error('microtask error: ' + ((e && e.stack) || e)); }
    });
};
globalThis.console = {
    log: function () { __scConsoleLog(Array.prototype.slice.call(arguments).join(' ')); },
    warn: function () { __scConsoleLog('[warn] ' + Array.prototype.slice.call(arguments).join(' ')); },
    error: function () { __scConsoleLog('[error] ' + Array.prototype.slice.call(arguments).join(' ')); },
};
"#;

pub fn install(rt: &Runtime) -> rusty_hermes::Result<()> {
    create_view::register(rt)?;
    create_text::register(rt)?;
    set_text::register(rt)?;
    create_sk_node::register(rt)?;
    set_sk_props::register(rt)?;
    append_child::register(rt)?;
    remove_child::register(rt)?;
    set_style::register(rt)?;
    set_root::register(rt)?;
    console_log::register(rt)?;
    init_core::register(rt)?;
    set_session::register(rt)?;
    auth_status::register(rt)?;
    me::register(rt)?;
    home_clusters::register(rt)?;
    wave::register(rt)?;
    resolve_tracks::register(rt)?;
    rt.eval(PRELUDE_JS).map_err(|e| {
        rusty_hermes::Error::RuntimeError(format!("failed to install JS prelude shims: {e}"))
    })?;
    Ok(())
}
