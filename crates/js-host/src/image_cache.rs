//! Async fetch + decode for the plain (non-Skia) `<Image>` component
//! (`react-native.tsx`) — `source={{ uri }}` on Avatar/Card/TrackRow.
//! `@shopify/react-native-skia`'s own `<Image>`/`useImage()` (drawn inside a
//! `Canvas`, `NodeKind::SkImage`) is a separate, still-unimplemented path —
//! `@sc/ui` doesn't currently use it (see the shim-coverage audit).
//!
//! Same background-tokio-runtime + mpsc-channel shape as `live_data.rs`, but
//! keyed by `NodeId` instead of an ad hoc callback id, and delivering raw
//! bytes rather than JSON: `skia_safe::Image` wraps a ref-counted native
//! handle that isn't safe to construct or move across threads without care,
//! so the actual decode happens back on the main thread in `drain_ready()`,
//! not on the fetch task.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};

use skia_safe::{Data, Image};

use crate::scene::NodeId;

type FetchResult = (NodeId, Result<Vec<u8>, String>);

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static CHANNEL: OnceLock<(Sender<FetchResult>, Mutex<Receiver<FetchResult>>)> = OnceLock::new();
// Which URL each node last requested — `request()` no-ops on a duplicate
// call for the same (id, url) pair (react-reconciler's commitUpdate can run
// far more often than the source actually changes).
static REQUESTED: OnceLock<Mutex<HashMap<NodeId, String>>> = OnceLock::new();

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("failed to create image-cache tokio runtime"))
}

fn channel_pair() -> &'static (Sender<FetchResult>, Mutex<Receiver<FetchResult>>) {
    CHANNEL.get_or_init(|| {
        let (tx, rx) = channel();
        (tx, Mutex::new(rx))
    })
}

fn requested() -> &'static Mutex<HashMap<NodeId, String>> {
    REQUESTED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Fire-and-forget: fetches `url` on the background runtime, delivering raw
/// bytes (or an error, silently dropped by `drain_ready` — a fetch failure
/// just leaves the node showing no image, the same as still-loading) back
/// through `drain_ready()`. No-ops if `id` already requested this exact URL.
pub fn request(id: NodeId, url: String) {
    let mut map = requested().lock().expect("image-cache requested-map lock poisoned");
    if map.get(&id) == Some(&url) {
        return;
    }
    map.insert(id, url.clone());
    drop(map);

    let tx = channel_pair().0.clone();
    runtime().spawn(async move {
        let result = fetch(&url).await;
        let _ = tx.send((id, result));
    });
}

pub fn forget(id: NodeId) {
    requested().lock().expect("image-cache requested-map lock poisoned").remove(&id);
}

async fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let response = reqwest::get(url).await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    Ok(bytes.to_vec())
}

/// Decodes whatever finished since the last call — called once per frame
/// from rn-linux, same pattern as `live_data::deliver`. `None` for a
/// genuinely undecodable response (corrupt data, unsupported format), not
/// just "still loading" (those never appear here at all until they finish).
pub fn drain_ready() -> Vec<(NodeId, Option<Image>)> {
    let rx = channel_pair().1.lock().expect("image-cache receiver lock poisoned");
    let mut out = Vec::new();
    while let Ok((id, result)) = rx.try_recv() {
        let image = result.ok().and_then(|bytes| Image::from_encoded(Data::new_copy(&bytes)));
        out.push((id, image));
    }
    out
}
