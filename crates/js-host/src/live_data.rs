//! Spike 7b: live data from `sc-rn` (Core/shared's uniffi bridge to real
//! network/auth/cache). `sc-rn`'s functions are plain Rust `async fn` here —
//! we call them directly, not through uniffi's generated FFI (that's for
//! Kotlin/Swift consumers; calling from Rust just needs an executor).
//!
//! Network calls must never block the render thread, so this is fire-and-
//! forget from JS's side: a call spawns onto our own background tokio
//! runtime, and the result lands in a channel that `drain()` — called every
//! frame from rn-linux, alongside the reanimated tick — delivers to JS by
//! resolving/rejecting the Promise the JS-side `callAsync()` wrapper is
//! waiting on (see js/src/live-data.ts).

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};

type AsyncResult = (u32, Result<String, String>);

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static CHANNEL: OnceLock<(Sender<AsyncResult>, Mutex<Receiver<AsyncResult>>)> = OnceLock::new();

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("failed to create live-data tokio runtime"))
}

fn channel_pair() -> &'static (Sender<AsyncResult>, Mutex<Receiver<AsyncResult>>) {
    CHANNEL.get_or_init(|| {
        let (tx, rx) = channel();
        (tx, Mutex::new(rx))
    })
}

/// Drives `fut` to completion on our background runtime; `fut` itself calls
/// straight into a `sc_rn::*` async fn, which internally hops onto `sc-rn`'s
/// own runtime for the actual I/O (see Core/shared/crates/sc-rn/src/runtime.rs)
/// — this is just the outer executor context that `.await`s it.
pub fn spawn_call<F>(callback_id: u32, fut: F)
where
    F: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    let tx = channel_pair().0.clone();
    runtime().spawn(async move {
        let result = fut.await;
        let _ = tx.send((callback_id, result));
    });
}

/// Called once per frame — never blocks, just collects whatever finished
/// since the last call.
pub fn drain() -> Vec<AsyncResult> {
    let rx = channel_pair().1.lock().expect("live-data receiver lock poisoned");
    let mut out = Vec::new();
    while let Ok(item) = rx.try_recv() {
        out.push(item);
    }
    out
}

/// Delivers whatever finished since the last frame to JS's `callAsync()`
/// wrapper (js/src/live-data.ts) by calling `__scDeliverResult(callbackId,
/// ok, payload)` — `payload` goes through `create_value_from_json` rather
/// than a string-interpolated `eval()`, so JSON escaping is never our
/// problem. Called once per frame from rn-linux, right alongside the
/// reanimated tick and microtask drain.
pub fn deliver(rt: &rusty_hermes::Runtime) {
    let pending = drain();
    if pending.is_empty() {
        return;
    }
    let Ok(deliver_fn) = rt
        .global()
        .get("__scDeliverResult")
        .and_then(|v| v.into_function())
    else {
        return;
    };
    for (callback_id, result) in pending {
        let id = rusty_hermes::Value::from_number(callback_id as f64);
        let (ok, payload_json) = match result {
            Ok(json) => (true, json),
            Err(err) => (false, serde_json::to_string(&err).expect("string always serializes")),
        };
        let Ok(payload) = rt.create_value_from_json(&payload_json) else {
            continue;
        };
        let _ = deliver_fn.call(&[id, rusty_hermes::Value::from_bool(ok), payload]);
    }
}
