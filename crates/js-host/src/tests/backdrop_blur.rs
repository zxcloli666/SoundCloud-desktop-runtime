//! `<BackdropBlur>` — real backdrop sampling: content already drawn beneath
//! the node must come out blurred inside its clip. A hard black/white edge
//! under the glass region turns gray at the boundary; outside the clip it
//! stays razor sharp.

use crate::scene::{Scene, StyleInput};

fn style(json: &str) -> StyleInput {
    serde_json::from_str(json).expect("valid style JSON")
}

#[test]
fn backdrop_blur_softens_the_content_beneath() {
    let mut scene = Scene::new();
    // 40x40: left half white, right half black (hard edge at x=20).
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 40, "height": 40, "backgroundColor": [0.0, 0.0, 0.0, 1.0], "flexDirection": "row"}"#));
    scene.set_root(root);

    let white_half = scene.create_view();
    scene.set_style(white_half, style(r#"{"width": 20, "height": 40, "backgroundColor": [1.0, 1.0, 1.0, 1.0]}"#));
    scene.append_child(root, white_half);

    // Glass overlay over the TOP half only (clip y 0..20).
    let canvas_node = scene.create_sk_node("Canvas");
    scene.set_style(canvas_node, style(r#"{"position": "absolute", "left": 0, "top": 0, "width": 40, "height": 40}"#));
    scene.append_child(root, canvas_node);

    let glass = scene.create_sk_node("BackdropBlur");
    scene.set_sk_props(
        glass,
        serde_json::json!({
            "blur": 6.0,
            "clip": {"x": 0.0, "y": 0.0, "width": 40.0, "height": 20.0},
        }),
    );
    scene.append_child(canvas_node, glass);

    scene.compute_layout(40.0, 40.0);
    let image_info = skia_safe::ImageInfo::new_n32_premul((40, 40), None);
    let mut surface = skia_safe::surfaces::raster(&image_info, None, None).expect("raster surface");
    scene.draw(surface.canvas());
    let snapshot = surface.image_snapshot();
    let pixmap = snapshot.peek_pixels().expect("readable");

    // Inside the glass (y=10): the edge column blends toward gray.
    let blurred_edge = pixmap.get_color((20, 10));
    assert!(
        blurred_edge.r() > 40 && blurred_edge.r() < 215,
        "edge under the glass should be blurred gray, got r={}",
        blurred_edge.r()
    );
    // Far away from the edge the halves keep their color even under glass.
    assert!(pixmap.get_color((3, 10)).r() > 200, "deep white side stays white under glass");
    assert!(pixmap.get_color((37, 10)).r() < 55, "deep black side stays black under glass");

    // Below the clip (y=30) the edge is untouched — still razor sharp.
    assert!(pixmap.get_color((18, 30)).r() > 230, "unclipped white side untouched");
    assert!(pixmap.get_color((22, 30)).r() < 25, "unclipped black side untouched");
}
