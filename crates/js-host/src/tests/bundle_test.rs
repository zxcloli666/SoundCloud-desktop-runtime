//! Guards the `js/` bundle against regressions: react-reconciler's host-config
//! surface has grown undocumented required methods across React versions
//! (see Desktop-Runtime/CLAUDE.md) — this catches it breaking again without
//! needing a GPU window. Requires `pnpm build` in `js/` to have run first.
//!
//! `rt.eval(&bundle)` below runs our own esbuild-produced `dist/bundle.js`
//! inside Hermes — Hermes' `eval` is this embedded JS engine's ordinary
//! script-execution entry point, not a code-injection risk: the input is a
//! locally built artifact, never untrusted/external data.

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

/// Proves `hostConfig.ts`'s press-registration path for real — not just
/// `Scene::hit_test`'s own algorithm (see `hit_test_test`, which calls
/// `watch_press` directly, bypassing JS entirely). The demo bundle's
/// `CoreUiProbe` mounts several real `@sc/ui` components with `onPress`
/// set (Button/Card/TrackRow); if `<Pressable onPress={...}>` reaching
/// `applyStyle` correctly calls `__scWatchPress`, at least one real,
/// on-screen coordinate should hit-test successfully.
#[test]
fn real_pressable_components_register_with_scene_hit_test() {
    let rt = super::Runtime::new().expect("failed to create Hermes runtime");
    super::host::install(&rt).expect("failed to install host functions");
    let bundle = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../js/dist/bundle.js"
    ))
    .unwrap_or_else(|e| panic!("read js/dist/bundle.js: {e} (run `pnpm build` in js/)"));
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
    assert!(found_a_hit, "expected at least one registered pressable node somewhere in the demo tree (Button/Card/TrackRow all set onPress)");
}

/// Guards a real bug found manually verifying scroll (task #19): the
/// content wrapper `ScrollView` renders around a real `<Card>` list is a
/// column-direction child of the (also column-direction) outer clipping
/// container, so Yoga's default `alignItems: stretch` clamped its width
/// to match the container's — exactly the one dimension a *horizontal*
/// scroll's content needs to size naturally past the container (nothing
/// to scroll otherwise). `HorizontalScroll` in the demo bundle is
/// deliberately narrower (`width: 200`) than its three `Card`s combined.
#[test]
fn horizontal_scroll_content_is_wider_than_its_narrow_container() {
    let rt = super::Runtime::new().expect("failed to create Hermes runtime");
    super::host::install(&rt).expect("failed to install host functions");
    let bundle = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../js/dist/bundle.js"
    ))
    .unwrap_or_else(|e| panic!("read js/dist/bundle.js: {e} (run `pnpm build` in js/)"));
    rt.eval(&bundle).expect("bundle JS failed");
    super::pump_frames(&rt, 10);

    super::host::with_scene(|scene| {
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
