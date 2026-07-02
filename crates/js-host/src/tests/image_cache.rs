//! Real network fetch + real Skia decode for `<Image>` — hits a real
//! external endpoint and polls like the render loop does, no mock server.

use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;

use crate::scene::{NodeId, Scene};

// `image_cache::drain_ready()` fully empties the one process-global
// mpsc channel on every call — `skia_safe::Image` wraps a raw
// `NonNull` ref-counted handle with no `unsafe impl Send`/`Sync`
// anywhere in skia-safe, so a result can't be hand off to another
// thread via a shared stash either. Concretely: separate `#[test]` fns
// each polling `drain_ready()` from their own OS thread (cargo's
// default) could steal and silently discard each other's genuinely-
// ready result — the same class of bug `live_data_test`'s callback-id
// collision hit, but distinctive ids alone don't fix it this time,
// since the problem isn't collision, it's that `drain_ready` hands
// *every* thread *all* ready results and non-matches get thrown away.
// Folding every scenario that touches `drain_ready()` into this one
// test keeps every `request`/poll on a single thread, so there's
// nothing to steal — this includes the `require()`-asset scenario
// below (`RequiredAssetTile` in `js/playground/src/index.tsx`), which
// needs its own Hermes runtime + bundle mount but must still poll
// `drain_ready()` on this same thread, not a fresh `#[test]` fn's.
#[test]
fn fetch_lifecycle_real_url_bad_url_duplicate_request_and_required_asset() {
    let real_id: NodeId = 200_001;
    let bad_id: NodeId = 200_002;
    let dup_id: NodeId = 200_003;
    let dup_url = "https://picsum.photos/id/1/100/100.jpg".to_string();

    super::image_cache::request(real_id, "https://picsum.photos/id/237/200/200.jpg".to_string());
    super::image_cache::request(bad_id, "https://picsum.photos/id/237/200/200.jpg/this-path-does-not-exist-404".to_string());
    // Duplicate request for the same (id, url) — should be a no-op,
    // not spawn a second fetch or panic on reentrant state.
    super::image_cache::request(dup_id, dup_url.clone());
    super::image_cache::request(dup_id, dup_url);

    // Mounting the playground bundle fires `image_cache::request` for
    // `RequiredAssetTile`'s `data:image/png;base64,...` source as a side
    // effect of `set_style`'s `imageUri` branch (scene.rs) — same trigger
    // a real fetched `source={{ uri }}` would use, just with a build-time-
    // embedded `data:` URI instead of a network URL (see
    // `js/build-support.mjs`'s `imageAssetLoaders`).
    let rt = super::Runtime::new().expect("failed to create Hermes runtime");
    super::host::install(&rt).expect("failed to install host functions");
    let bundle = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../js/dist/playground.js"
    ))
    .unwrap_or_else(|e| panic!("read js/dist/playground.js: {e} (run `pnpm build` in js/)"));
    rt.eval(&bundle).expect("bundle JS failed");
    super::pump_frames(&rt, 10);
    let asset_id = super::host::with_scene(|scene| {
        scene.compute_layout(1024.0, 640.0);
        let root = scene.root.expect("bundle should have set a scene root");
        find_node_by_size(scene, root, 20.0, 20.0)
    })
    .expect("RequiredAssetTile's own 20x20 Image node should exist in the mounted tree");

    let mut pending: std::collections::HashSet<NodeId> = [real_id, bad_id, dup_id, asset_id].into_iter().collect();
    let mut results: HashMap<NodeId, Option<skia_safe::Image>> = HashMap::new();
    for _ in 0..200 {
        if pending.is_empty() {
            break;
        }
        for (ready_id, image) in super::image_cache::drain_ready() {
            if pending.remove(&ready_id) {
                results.insert(ready_id, image);
            }
        }
        if !pending.is_empty() {
            sleep(Duration::from_millis(25));
        }
    }
    assert!(pending.is_empty(), "image fetches for {pending:?} did not complete within 5s");

    let real_image = results.remove(&real_id).flatten().expect("a real, reachable JPEG URL should decode successfully");
    assert!(real_image.width() > 0 && real_image.height() > 0, "decoded image should have real dimensions");

    assert!(results.remove(&bad_id).flatten().is_none(), "a 404 should decode to nothing, not panic the render loop");

    assert!(results.remove(&dup_id).flatten().is_some(), "the de-duplicated request should still resolve normally");

    let asset_image = results
        .remove(&asset_id)
        .flatten()
        .expect("a require()'d asset's data: URI should decode successfully, not silently fail like a 404 would");
    assert_eq!(
        (asset_image.width(), asset_image.height()),
        (2, 2),
        "should decode to test-asset.png's real 2x2 dimensions, not a placeholder"
    );
}

fn find_node_by_size(scene: &Scene, id: NodeId, w: f32, h: f32) -> Option<NodeId> {
    let (_, _, node_w, node_h) = scene.layout_of(id);
    if (node_w - w).abs() < 0.5 && (node_h - h).abs() < 0.5 {
        return Some(id);
    }
    scene.children_of(id).into_iter().find_map(|child| find_node_by_size(scene, child, w, h))
}
