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

/// Shims for host globals bare Hermes doesn't provide (no browser, no Node):
/// react-reconciler/scheduler need `setTimeout`-family and `queueMicrotask`.
/// Timers run the callback immediately — we render a static tree synchronously,
/// nothing here actually needs to wait.
const PRELUDE_JS: &str = r#"
globalThis.setTimeout = function (fn) { fn(); return 0; };
globalThis.clearTimeout = function () {};
globalThis.setInterval = function () { return 0; };
globalThis.clearInterval = function () {};
// Hermes' own Promise internals reach for a host `setImmediate` when
// scheduling `.then()` jobs — without it, `Promise.resolve().then()` throws.
globalThis.setImmediate = function (fn) { fn(); return 0; };
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
    append_child::register(rt)?;
    remove_child::register(rt)?;
    set_style::register(rt)?;
    set_root::register(rt)?;
    console_log::register(rt)?;
    rt.eval(PRELUDE_JS).map_err(|e| {
        rusty_hermes::Error::RuntimeError(format!("failed to install JS prelude shims: {e}"))
    })?;
    Ok(())
}
