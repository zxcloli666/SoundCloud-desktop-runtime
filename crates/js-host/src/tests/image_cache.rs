//! Real network fetch + real Skia decode for `<Image>` (task #20) — same
//! "hit a real external endpoint, poll like rn-linux's render loop does"
//! approach as `live_data_test` (no test-only shortcut, no mock server).

use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;

use crate::scene::NodeId;

// `image_cache::drain_ready()` fully empties the one process-global
// mpsc channel on every call — `skia_safe::Image` wraps a raw
// `NonNull` ref-counted handle with no `unsafe impl Send`/`Sync`
// anywhere in skia-safe, so a result can't be hand off to another
// thread via a shared stash either. Concretely: three separate
// `#[test]` fns each polling `drain_ready()` from their own OS thread
// (cargo's default) could steal and silently discard each other's
// genuinely-ready result — the same class of bug `live_data_test`'s
// callback-id collision hit, but distinctive ids alone don't fix it
// this time, since the problem isn't collision, it's that `drain_ready`
// hands *every* thread *all* ready results and non-matches get thrown
// away. Folding all three scenarios into one test keeps every
// `request`/poll on a single thread, so there's nothing to steal.
#[test]
fn fetch_lifecycle_real_url_bad_url_and_duplicate_request() {
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

    let mut pending: std::collections::HashSet<NodeId> = [real_id, bad_id, dup_id].into_iter().collect();
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
}
