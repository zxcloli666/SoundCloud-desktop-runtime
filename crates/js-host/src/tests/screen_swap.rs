//! Guards a real production bug: react-reconciler's deletion-effects path
//! calls `hostConfig.detachDeletedInstance` unconditionally for every
//! deleted host fiber — missing it throws inside a passive-effect flush,
//! which our timer-drain catches and logs. That doesn't break the commit
//! that triggered it (the mutation phase already lands before the passive-
//! effect flush runs) — it corrupts the reconciler's own scheduler state,
//! so the *next* commit never happens. `js/playground/src/index.tsx`'s
//! `ScreenSwap` reproduces exactly this: phase0 -> phase1 (first `key`
//! swap, triggers the crash) -> phase2 (second swap, the one that actually
//! goes missing without the fix).

#[test]
fn a_second_screen_swap_still_commits_after_the_first_ones_deletion_effects() {
    let rt = super::Runtime::new().expect("failed to create Hermes runtime");
    super::host::install(&rt).expect("failed to install host functions");
    let bundle = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../js/dist/playground.js"
    ))
    .unwrap_or_else(|e| panic!("read js/dist/playground.js: {e} (run `pnpm build` in js/)"));
    rt.eval(&bundle).expect("bundle JS failed");
    super::pump_frames(&rt, 30);

    super::host::with_scene(|scene| {
        scene.compute_layout(1024.0, 640.0);
        let root = scene.root.expect("scene root should still exist");
        assert!(
            find_node_by_size(scene, root, 66.0, 66.0).is_some(),
            "the second effect-triggered setState (phase2) should have committed, not gotten stuck behind the first swap's deletion-effects crash"
        );
        assert!(find_node_by_size(scene, root, 22.0, 22.0).is_none(), "phase0's box should have been unmounted");
        assert!(find_node_by_size(scene, root, 44.0, 44.0).is_none(), "phase1's box should have been unmounted");
    });
}

fn find_node_by_size(scene: &super::scene::Scene, id: super::scene::NodeId, w: f32, h: f32) -> Option<super::scene::NodeId> {
    let (_, _, node_w, node_h) = scene.layout_of(id);
    if (node_w - w).abs() < 0.5 && (node_h - h).abs() < 0.5 {
        return Some(id);
    }
    scene.children_of(id).into_iter().find_map(|child| find_node_by_size(scene, child, w, h))
}
