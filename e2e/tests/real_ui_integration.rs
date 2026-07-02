//! Proves the real, unmodified `@sc/ui` package still mounts/hit-tests/
//! scrolls correctly on this runtime — not just that the generic mechanism
//! works (the engine's own `crates/js-host/src/tests/bundle_test.rs`
//! already proves that, zero-dependency, against a synthetic playground
//! fixture). Intentional duplication, not redundancy: per Desktop-Runtime/
//! CLAUDE.md's "Спайк 8", 9 real bugs were found only by rendering the
//! real `@sc/ui` tree — this is the permanent regression guard for that
//! class of bug. Requires `Core` checked out as a sibling of
//! `Desktop-Runtime`, and `pnpm build` in `examples/soundcloud/js` to have
//! produced its real `dist/bundle.js` — see e2e/Cargo.toml.
//!
//! `rt.eval(&bundle)` below runs our own esbuild-produced `dist/bundle.js`
//! inside Hermes — Hermes' `eval` is this embedded JS engine's ordinary
//! script-execution entry point, not a code-injection risk: the input is a
//! locally built artifact, never untrusted/external data.

/// Mirrors the engine's own per-frame pump (drain due timers, then
/// microtasks they produced) — `ConcurrentRoot`'s initial mount schedules
/// its commit through that same path instead of completing inline inside a
/// single `eval()` call, so tests that load a real bundle need to pump a
/// few frames before the scene tree exists. Duplicated from js-host's own
/// (private, `#[cfg(test)]`-only) helper — this crate is an external
/// integration-test consumer, it can't reach that one.
fn pump_frames(rt: &js_host::Runtime, count: u32) {
    for _ in 0..count {
        rt.eval("if (typeof __scDrainTimers === 'function') __scDrainTimers();").expect("drain timers failed");
        rt.drain_microtasks().expect("drain microtasks failed");
    }
}

fn bundle_path() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/soundcloud/js/dist/bundle.js")
}

/// Proves `hostConfig.ts`'s press-registration path for real — not just
/// `Scene::hit_test`'s own algorithm (`crates/js-host/src/tests/
/// hit_test.rs`, which calls `watch_press` directly, bypassing JS
/// entirely). The real demo's `CoreUiProbe` mounts several real `@sc/ui`
/// components with `onPress` set (Button/Card/TrackRow); if `<Pressable
/// onPress={...}>` reaching `applyStyle` correctly calls `__scWatchPress`,
/// at least one real, on-screen coordinate should hit-test successfully.
#[test]
fn real_pressable_components_register_with_scene_hit_test() {
    let rt = js_host::Runtime::new().expect("failed to create Hermes runtime");
    js_host::host::install(&rt).expect("failed to install host functions");
    let bundle = std::fs::read_to_string(bundle_path())
        .unwrap_or_else(|e| panic!("read {}: {e} (run `pnpm build` in examples/soundcloud/js)", bundle_path()));
    rt.eval(&bundle).expect("bundle JS failed");
    pump_frames(&rt, 10);

    let (width, height) = (1024.0, 640.0);
    let found_a_hit = js_host::host::with_scene(|scene| {
        scene.compute_layout(width, height);
        let (cols, rows) = (40, 40);
        for i in 0..cols {
            for j in 0..rows {
                let x = width * (i as f32 + 0.5) / cols as f32;
                let y = height * (j as f32 + 0.5) / rows as f32;
                if scene.hit_test(x, y).is_some() {
                    return true;
                }
            }
        }
        false
    });
    assert!(found_a_hit, "expected at least one registered pressable node somewhere in the real @sc/ui demo tree (Button/Card/TrackRow all set onPress)");
}

/// Guards a real bug found manually verifying scroll (Desktop-Runtime/
/// CLAUDE.md, spike 8 item 8): the content wrapper `ScrollView` renders
/// around a real `<Card>` list is a column-direction child of the (also
/// column-direction) outer clipping container, so Yoga's default
/// `alignItems: stretch` clamped its width to match the container's —
/// exactly the one dimension a *horizontal* scroll's content needs to size
/// naturally past the container (nothing to scroll otherwise).
/// `HorizontalScroll` in the demo bundle is deliberately narrower
/// (`width: 200`) than its three `Card`s combined.
#[test]
fn horizontal_scroll_content_is_wider_than_its_narrow_container() {
    let rt = js_host::Runtime::new().expect("failed to create Hermes runtime");
    js_host::host::install(&rt).expect("failed to install host functions");
    let bundle = std::fs::read_to_string(bundle_path())
        .unwrap_or_else(|e| panic!("read {}: {e} (run `pnpm build` in examples/soundcloud/js)", bundle_path()));
    rt.eval(&bundle).expect("bundle JS failed");
    pump_frames(&rt, 10);

    js_host::host::with_scene(|scene| {
        scene.compute_layout(1024.0, 640.0);
        let scrollables = scene.scrollable_node_ids();
        assert!(!scrollables.is_empty(), "the demo's HorizontalScroll should have registered as scrollable");
        let found_overflowing_content = scrollables.iter().any(|&container| {
            let (_, _, container_w, _) = scene.layout_of(container);
            scene.children_of(container).first().is_some_and(|&content| {
                let (_, _, content_w, _) = scene.layout_of(content);
                content_w > container_w
            })
        });
        assert!(found_overflowing_content, "HorizontalScroll's content should measure wider than its container — otherwise there's nothing to scroll");
    });
}
