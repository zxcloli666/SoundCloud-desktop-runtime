//! Spike 6: proves the reanimated tick loop actually advances a `withTiming`
//! animation over real wall-clock time — no GPU window needed, since the
//! interpolated value shows up as a real Yoga-computed layout width.
//!
//! `rt.eval(...)` below only ever runs our own locally built bundle/tick
//! shim JS, never external input — Hermes' ordinary script-execution entry
//! point, not a code-injection risk.

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
