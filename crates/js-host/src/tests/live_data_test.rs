//! Spike 7b: proves the whole async round-trip actually works — JS calls a
//! host function with a callback id, `sc_rn::auth_status()` runs to
//! completion on `async_bridge`'s background tokio runtime, and its result
//! reaches JS as a resolved Promise value, entirely through the same
//! `deliver()` polling rn-linux's render loop uses (no test-only shortcut).
//!
//! `rt.eval(...)` below only ever runs small, hardcoded inline JS owned by
//! this test, never external input — Hermes' ordinary script-execution
//! entry point, not a code-injection risk.

use std::thread::sleep;
use std::time::Duration;

#[test]
fn auth_status_round_trips_through_the_real_async_bridge() {
    let rt = super::Runtime::new().expect("failed to create Hermes runtime");
    super::host::install(&rt).expect("failed to install host functions");

    let tmp = std::env::temp_dir().join(format!("sc-rn-live-data-test-{}", std::process::id()));
    let data_dir = tmp.join("data");
    let cache_dir = tmp.join("cache");
    std::fs::create_dir_all(&data_dir).expect("create data dir");
    std::fs::create_dir_all(&cache_dir).expect("create cache dir");

    let init_err = rt
        .eval(&format!(
            "__scInitCore({:?}, {:?}, false)",
            data_dir.to_str().unwrap(),
            cache_dir.to_str().unwrap(),
        ))
        .expect("init eval failed")
        .into_string()
        .expect("init_core returns a string")
        .to_rust_string()
        .expect("valid utf8");
    assert_eq!(init_err, "", "sc-rn init_runtime failed: {init_err}");

    rt.eval(
        r#"
        globalThis.__testDone = false;
        globalThis.__testOk = null;
        globalThis.__testPayload = null;
        globalThis.__scDeliverResult = function (id, ok, payload) {
            if (id !== 100001) return;
            globalThis.__testDone = true;
            globalThis.__testOk = ok;
            globalThis.__testPayload = payload;
        };
        // A distinctive, out-of-range callback id — not 1: `async_bridge`'s
        // tokio runtime/mpsc channel (js-host/src/async_bridge.rs) is one
        // process-global instance shared by every `Runtime::new()` in
        // this test binary. The bundle's own `live-data.ts` module
        // (evaluated by bundle_test/reanimated_test/
        // fills_arbitrary_aspect_ratio_test, each a *separate* Hermes
        // runtime) fires its `LiveDataProbe`'s `authStatus()` with
        // callback ids starting at 1 too — a small hardcoded id here
        // could receive one of *their* stale results instead of this
        // call's own.
        __scAuthStatus(100001);
        "#,
    )
    .expect("eval failed");

    // Mirrors rn-linux's render loop: poll `deliver()` once per "frame"
    // instead of blocking on the background runtime directly.
    for _ in 0..200 {
        super::async_bridge::deliver(&rt);
        let done = rt.eval("globalThis.__testDone").expect("poll eval failed").as_bool().unwrap_or(false);
        if done {
            break;
        }
        sleep(Duration::from_millis(25));
    }

    let done = rt.eval("globalThis.__testDone").expect("poll eval failed").as_bool().unwrap_or(false);
    assert!(done, "auth_status() should have resolved or rejected within 5s");

    let ok = rt.eval("globalThis.__testOk").expect("poll eval failed").as_bool().expect("ok should be a bool");
    let debug_payload = rt.eval("String(globalThis.__testPayload)").expect("poll eval failed").into_string().expect("string").to_rust_string().expect("utf8");
    assert!(ok, "auth_status() should succeed with a fresh, empty data dir (no error expected): {debug_payload}");

    let payload = rt
        .eval("JSON.stringify(globalThis.__testPayload)")
        .expect("stringify failed")
        .into_string()
        .expect("result should be a string")
        .to_rust_string()
        .expect("valid utf8");
    let payload: serde_json::Value = serde_json::from_str(&payload).expect("payload should be valid JSON");
    assert_eq!(
        payload,
        serde_json::json!({
            "hasSession": false,
            "authenticated": false,
            "sessionId": null,
            "username": null,
            "tokenState": null,
        }),
        "a fresh sc-rn runtime with no stored session should report has_session=false",
    );

    // `home_clusters`/`wave`/`resolve_tracks` need real auth/network and
    // will legitimately fail unauthenticated — this isn't asserting they
    // succeed, just that their callback_id plumbing and `dto_json`
    // builders don't panic or hang on a second, concurrent in-flight
    // call multiplexed through the same `deliver()` channel.
    rt.eval(
        r#"
        globalThis.__testDone2 = false;
        __scHomeClusters(100002, 5, JSON.stringify([]), false);
        "#,
    )
    .expect("eval failed");
    rt.eval(
        r#"
        var __origDeliver = globalThis.__scDeliverResult;
        globalThis.__scDeliverResult = function (id, ok, payload) {
            if (id === 100002) { globalThis.__testDone2 = true; return; }
            __origDeliver(id, ok, payload);
        };
        "#,
    )
    .expect("eval failed");

    for _ in 0..200 {
        super::async_bridge::deliver(&rt);
        let done = rt.eval("globalThis.__testDone2").expect("poll eval failed").as_bool().unwrap_or(false);
        if done {
            break;
        }
        sleep(Duration::from_millis(25));
    }
    let done2 = rt.eval("globalThis.__testDone2").expect("poll eval failed").as_bool().unwrap_or(false);
    assert!(done2, "home_clusters() should have resolved or rejected within 5s, not hung");

    std::fs::remove_dir_all(&tmp).ok();
}
