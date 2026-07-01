//! Hermes embedded in our own Rust process via `rusty_hermes` (safe wrapper
//! over Hermes' JSI). Owns the mounted scene tree (`scene`) and the host
//! functions (`host`) a JS-side `react-reconciler` host-config calls into —
//! this is where Fabric's job (mounting + Yoga layout) happens for us.

pub mod dto_json;
pub mod host;
pub mod live_data;
pub mod scene;

pub use rusty_hermes::Runtime;
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

#[cfg(test)]
mod tests {
    use rusty_hermes::{Runtime, hermes_op};

    #[hermes_op]
    fn add(a: f64, b: f64, c: f64) -> f64 {
        a + b + c
    }

    #[test]
    fn evaluates_js_and_calls_back_into_rust() {
        let rt = Runtime::new().expect("failed to create Hermes runtime");
        add::register(&rt).expect("failed to register add()");

        let result = rt.eval("add(10, 20, 30)").expect("eval failed");
        assert_eq!(result.as_number(), Some(60.0));
    }

    #[test]
    fn mounts_a_tree_via_host_functions_and_computes_layout() {
        let rt = Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");

        rt.eval(
            r#"
            const root = __scCreateView();
            __scSetStyle(root, JSON.stringify({ width: 400, height: 200, flexDirection: "row" }));

            const a = __scCreateView();
            __scSetStyle(a, JSON.stringify({ flexGrow: 1, backgroundColor: [0.2, 0.4, 0.9, 1.0] }));
            __scAppendChild(root, a);

            const b = __scCreateView();
            __scSetStyle(b, JSON.stringify({ flexGrow: 1, backgroundColor: [0.9, 0.3, 0.5, 1.0] }));
            __scAppendChild(root, b);

            __scSetRoot(root);
            "#,
        )
        .expect("eval failed");

        super::host::with_scene(|scene| {
            scene.compute_layout(400.0, 200.0);
            let root = scene.root.expect("root should be set");
            assert_eq!(scene.layout_of(root), (0.0, 0.0, 400.0, 200.0));

            // Two equal flex-grow children in a row should split 400pt evenly.
            let children = scene.children_of(root);
            assert_eq!(children.len(), 2);
            assert_eq!(scene.layout_of(children[0]), (0.0, 0.0, 200.0, 200.0));
            assert_eq!(scene.layout_of(children[1]), (200.0, 0.0, 200.0, 200.0));
        });
    }
}

/// Guards the `js/` bundle against regressions: react-reconciler's host-config
/// surface has grown undocumented required methods across React versions
/// (see Desktop-Runtime/CLAUDE.md) — this catches it breaking again without
/// needing a GPU window. Requires `pnpm build` in `js/` to have run first.
#[cfg(test)]
mod bundle_test {
    #[test]
    fn react_reconciler_bundle_mounts_a_tree() {
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");
        let bundle = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../js/dist/bundle.js"
        ))
        .unwrap_or_else(|e| panic!("read js/dist/bundle.js: {e} (run `pnpm build` in js/)"));
        rt.eval(&bundle).expect("bundle JS failed");
        // ConcurrentRoot schedules its initial commit through the same
        // deferred-timer path as any other update — it doesn't complete
        // inline within the `eval()` call above. See `pump_frames`.
        super::pump_frames(&rt, 10);

        super::host::with_scene(|scene| {
            let root = scene.root.expect("bundle should have set a scene root");
            scene.compute_layout(1024.0, 640.0);
            let (_, _, w, h) = scene.layout_of(root);
            assert!(w > 0.0 && h > 0.0, "root should have a non-empty layout");
        });
    }
}

/// Spike 6: proves the reanimated tick loop actually advances a `withTiming`
/// animation over real wall-clock time — no GPU window needed, since the
/// interpolated value shows up as a real Yoga-computed layout width.
#[cfg(test)]
mod reanimated_test {
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn with_timing_animation_advances_and_settles() {
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");
        let bundle = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../js/dist/bundle.js"
        ))
        .unwrap_or_else(|e| panic!("read js/dist/bundle.js: {e} (run `pnpm build` in js/)"));
        rt.eval(&bundle).expect("bundle JS failed");
        // ConcurrentRoot's initial commit needs a frame pump too — see
        // `pump_frames` and `bundle_test`.
        super::pump_frames(&rt, 10);

        let badge_width = |rt: &super::Runtime| -> f32 {
            rt.eval("if (typeof __reanimatedTick === 'function') __reanimatedTick();").expect("tick failed");
            super::host::with_scene(|scene| {
                let root = scene.root.expect("bundle should have set a scene root");
                scene.compute_layout(1024.0, 640.0);
                // PulseBadge is App's last child (see index.tsx) — robust to
                // however many siblings render before it.
                let badge = *scene.children_of(root).last().expect("root should have children");
                scene.layout_of(badge).2
            })
        };

        let initial = badge_width(&rt);
        assert_eq!(initial, 24.0, "badge should start at its useSharedValue(24) initial width");

        sleep(Duration::from_millis(300));
        let mid = badge_width(&rt);
        assert!(mid > initial, "width should have grown partway through the 1200ms withTiming");

        sleep(Duration::from_millis(1200));
        let settled = badge_width(&rt);
        assert_eq!(settled, 220.0, "animation should settle exactly at its withTiming target");
    }
}

/// Tiling window managers hand out whatever aspect ratio fits their layout,
/// ignoring the app's requested size (found while chasing a screenshot that
/// looked cut off at the requested 1024x640 — the actual window was
/// 847x1388). This renders offscreen (no GPU window needed) at that same
/// unusual aspect ratio and checks the root's background genuinely covers
/// it, guarding against the root/Canvas silently falling back to a fixed size.
#[cfg(test)]
mod fills_arbitrary_aspect_ratio_test {
    #[test]
    fn root_background_covers_a_tall_narrow_window() {
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");
        let bundle = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../js/dist/bundle.js"
        ))
        .unwrap_or_else(|e| panic!("read js/dist/bundle.js: {e} (run `pnpm build` in js/)"));
        rt.eval(&bundle).expect("bundle JS failed");
        // ConcurrentRoot's initial commit needs a frame pump too — see
        // `pump_frames` and `bundle_test`.
        super::pump_frames(&rt, 10);

        let (width, height) = (847, 1388);
        let image_info = skia_safe::ImageInfo::new_n32_premul((width, height), None);
        let mut surface = skia_safe::surfaces::raster(&image_info, None, None).expect("raster surface");
        super::host::with_scene(|scene| {
            scene.compute_layout(width as f32, height as f32);
            scene.draw(surface.canvas());
        });

        let image = surface.image_snapshot();
        let pixmap = image.peek_pixels().expect("raster surface should be readable");
        // Root's backgroundColor is [0.04, 0.05, 0.08, 1.0] — check well below
        // where the demo's hardcoded-pixel-position orbs/panel stop (~500px),
        // near the bottom of the actual window rather than a fixed old size.
        let color = pixmap.get_color((width / 2, height - 20));
        assert_eq!((color.r(), color.g(), color.b()), (10, 13, 20), "root background should reach the true window bottom");
    }
}

/// Documents a genuine Hermes engine bug found while wiring up real `@sc/ui`
/// (spike 7): a `for (let key of ...)` loop whose body defines a closure via
/// `Object.defineProperty` doesn't get a fresh `key` binding per iteration —
/// every getter ends up seeing the *last* key. This is exactly the shape of
/// esbuild's own CJS→ESM interop helper (`__copyProps`), which `js/build.mjs`
/// patches post-build (swaps the loop for `.forEach`, where `key` is a
/// function parameter instead of a loop-scoped `let`). If Hermes ever fixes
/// this, both `createContextType` assertions below would need to flip to
/// "function" — that's the signal to remove the build.mjs patch too.
#[cfg(test)]
mod hermes_for_of_let_closure_bug_test {
    #[test]
    fn reproduces_in_isolation() {
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        let js = r#"
        var __getOwnPropNames = Object.getOwnPropertyNames;
        var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
        var __hasOwnProp = Object.prototype.hasOwnProperty;
        var __defProp = Object.defineProperty;
        var copyPropsForOf = (to, from, except, desc) => {
          if (from && typeof from === "object" || typeof from === "function") {
            for (let key of __getOwnPropNames(from))
              if (!__hasOwnProp.call(to, key) && key !== except)
                __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
          }
          return to;
        };

        var fakeModule = { firstFn: function () { return "ok"; }, version: "1.2.3" };
        var wrapped = copyPropsForOf({}, fakeModule);
        JSON.stringify({ firstFnType: typeof wrapped.firstFn, firstFnValue: String(wrapped.firstFn) });
        "#;
        let result = rt.eval(js).expect("eval failed").into_string().expect("result should be a string").to_rust_string().expect("valid utf8");
        assert_eq!(
            result,
            r#"{"firstFnType":"string","firstFnValue":"1.2.3"}"#,
            "if this ever reads back as a function, the Hermes bug is fixed — go remove the build.mjs __copyProps patch",
        );
    }
}

/// Spike 7b: proves the whole async round-trip actually works — JS calls a
/// host function with a callback id, `sc_rn::auth_status()` runs to
/// completion on `live_data`'s background tokio runtime, and its result
/// reaches JS as a resolved Promise value, entirely through the same
/// `deliver()` polling rn-linux's render loop uses (no test-only shortcut).
#[cfg(test)]
mod live_data_test {
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
                globalThis.__testDone = true;
                globalThis.__testOk = ok;
                globalThis.__testPayload = payload;
            };
            __scAuthStatus(1);
            "#,
        )
        .expect("eval failed");

        // Mirrors rn-linux's render loop: poll `deliver()` once per "frame"
        // instead of blocking on the background runtime directly.
        for _ in 0..200 {
            super::live_data::deliver(&rt);
            let done = rt.eval("globalThis.__testDone").expect("poll eval failed").as_bool().unwrap_or(false);
            if done {
                break;
            }
            sleep(Duration::from_millis(25));
        }

        let done = rt.eval("globalThis.__testDone").expect("poll eval failed").as_bool().unwrap_or(false);
        assert!(done, "auth_status() should have resolved or rejected within 5s");

        let ok = rt.eval("globalThis.__testOk").expect("poll eval failed").as_bool().expect("ok should be a bool");
        assert!(ok, "auth_status() should succeed with a fresh, empty data dir (no error expected)");

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
            __scHomeClusters(2, 5, JSON.stringify([]), false);
            "#,
        )
        .expect("eval failed");
        rt.eval(
            r#"
            var __origDeliver = globalThis.__scDeliverResult;
            globalThis.__scDeliverResult = function (id, ok, payload) {
                if (id === 2) { globalThis.__testDone2 = true; return; }
                __origDeliver(id, ok, payload);
            };
            "#,
        )
        .expect("eval failed");

        for _ in 0..200 {
            super::live_data::deliver(&rt);
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
}
