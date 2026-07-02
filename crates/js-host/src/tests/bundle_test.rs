//! Guards `js/playground/src/index.tsx` — the engine's own zero-dependency
//! demo bundle — against react-reconciler regressions: its host-config
//! surface has grown undocumented required methods across React versions
//! (see Desktop-Runtime/CLAUDE.md) — this catches it breaking again without
//! needing a GPU window. Requires `pnpm build` in `js/` to have run first.
//!
//! The real `@sc/ui` bundle gets its own, permanent duplicate of the two
//! contract-shaped assertions below in `e2e/tests/real_ui_integration.rs` —
//! intentional duplication, not redundancy: these prove the generic
//! mechanism works at all (provable with zero `Core` on disk), that one
//! proves the *real, unmodified* `@sc/ui` package still mounts/hit-tests/
//! scrolls correctly on this runtime (Desktop-Runtime/CLAUDE.md, "Спайк 8").
//!
//! `rt.eval(&bundle)` below runs our own esbuild-produced `dist/
//! playground.js` inside Hermes — Hermes' `eval` is this embedded JS
//! engine's ordinary script-execution entry point, not a code-injection
//! risk: the input is a locally built artifact, never untrusted/external
//! data.

#[test]
fn react_reconciler_bundle_mounts_a_tree() {
    let rt = super::Runtime::new().expect("failed to create Hermes runtime");
    super::host::install(&rt).expect("failed to install host functions");
    let bundle = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../js/dist/playground.js"
    ))
    .unwrap_or_else(|e| panic!("read js/dist/playground.js: {e} (run `pnpm build` in js/)"));
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

/// Proves `hostConfig.ts`'s press-registration path for real — not just
/// `Scene::hit_test`'s own algorithm (see `hit_test_test`, which calls
/// `watch_press` directly, bypassing JS entirely). The playground mounts
/// two `PressableTile`s with `onPress` set; if `<Pressable onPress={...}>`
/// reaching `applyStyle` correctly calls `__scWatchPress`, at least one
/// real, on-screen coordinate should hit-test successfully.
#[test]
fn real_pressable_components_register_with_scene_hit_test() {
    let rt = super::Runtime::new().expect("failed to create Hermes runtime");
    super::host::install(&rt).expect("failed to install host functions");
    let bundle = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../js/dist/playground.js"
    ))
    .unwrap_or_else(|e| panic!("read js/dist/playground.js: {e} (run `pnpm build` in js/)"));
    rt.eval(&bundle).expect("bundle JS failed");
    super::pump_frames(&rt, 10);

    let (width, height) = (1024.0, 640.0);
    let found_a_hit = super::host::with_scene(|scene| {
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
    assert!(found_a_hit, "expected at least one registered pressable node somewhere in the playground tree (both PressableTiles set onPress)");
}

/// Guards a real bug found manually verifying scroll (Desktop-Runtime/
/// CLAUDE.md, spike 8 item 8): a horizontal `ScrollView`'s content wrapper
/// is a column-direction child of the (also column-direction) outer
/// clipping container, so Yoga's default `alignItems: stretch` clamped its
/// width to match the container's — exactly the one dimension a
/// *horizontal* scroll's content needs to size naturally past the
/// container (nothing to scroll otherwise). The playground's
/// `OverflowCarousel` deliberately reproduces this exact nesting shape,
/// narrower (`width: 140`) than its three tiles combined.
#[test]
fn horizontal_scroll_content_is_wider_than_its_narrow_container() {
    let rt = super::Runtime::new().expect("failed to create Hermes runtime");
    super::host::install(&rt).expect("failed to install host functions");
    let bundle = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../js/dist/playground.js"
    ))
    .unwrap_or_else(|e| panic!("read js/dist/playground.js: {e} (run `pnpm build` in js/)"));
    rt.eval(&bundle).expect("bundle JS failed");
    super::pump_frames(&rt, 10);

    super::host::with_scene(|scene| {
        scene.compute_layout(1024.0, 640.0);
        let scrollables = scene.scrollable_node_ids();
        assert!(!scrollables.is_empty(), "the playground's OverflowCarousel should have registered as scrollable");
        let found_overflowing_content = scrollables.iter().any(|&container| {
            let (_, _, container_w, _) = scene.layout_of(container);
            scene.children_of(container).first().is_some_and(|&content| {
                let (_, _, content_w, _) = scene.layout_of(content);
                content_w > container_w
            })
        });
        assert!(found_overflowing_content, "OverflowCarousel's content should measure wider than its container — otherwise there's nothing to scroll");
    });
}
