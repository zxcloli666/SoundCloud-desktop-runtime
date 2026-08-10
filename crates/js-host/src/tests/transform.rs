//! RN `transform` on Views (center-pivot scale/translate/rotate, inherited
//! by the subtree) and full rn-skia `Group` transforms (scale, not just
//! translate). Pixel-level: a transform that silently no-ops leaves the
//! shape at its untransformed spot, which these scenes are built to catch.

use crate::scene::{Scene, StyleInput};

fn style(json: &str) -> StyleInput {
    serde_json::from_str(json).expect("valid style JSON")
}

fn render(scene: &Scene, w: i32, h: i32) -> skia_safe::Image {
    let image_info = skia_safe::ImageInfo::new_n32_premul((w, h), None);
    let mut surface = skia_safe::surfaces::raster(&image_info, None, None).expect("raster surface");
    scene.draw(surface.canvas());
    surface.image_snapshot()
}

/// scale 0.5 on a full-bleed red view: red shrinks to the middle quarter
/// (10..30 of 40), corners uncover the black root.
#[test]
fn view_scale_shrinks_around_the_center() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 40, "height": 40, "backgroundColor": [0.0, 0.0, 0.0, 1.0]}"#));
    scene.set_root(root);

    let red = scene.create_view();
    scene.set_style(
        red,
        style(r#"{"width": 40, "height": 40, "backgroundColor": [1.0, 0.0, 0.0, 1.0], "transform": [{"scale": 0.5}]}"#),
    );
    scene.append_child(root, red);

    scene.compute_layout(40.0, 40.0);
    let snapshot = render(&scene, 40, 40);
    let pixmap = snapshot.peek_pixels().expect("readable");

    assert_eq!(pixmap.get_color((20, 20)).r(), 255, "center stays red");
    assert_eq!(pixmap.get_color((3, 3)).r(), 0, "corner uncovered by the shrink");
    assert_eq!(pixmap.get_color((36, 36)).r(), 0, "opposite corner uncovered");
    assert_eq!(pixmap.get_color((12, 20)).r(), 255, "inside the scaled half");
}

/// translateX moves the view; the vacated strip shows the root again.
#[test]
fn view_translate_moves_the_subtree() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 40, "height": 20, "backgroundColor": [0.0, 0.0, 0.0, 1.0]}"#));
    scene.set_root(root);

    let red = scene.create_view();
    scene.set_style(
        red,
        style(r#"{"width": 20, "height": 20, "backgroundColor": [1.0, 0.0, 0.0, 1.0], "transform": [{"translateX": 15}]}"#),
    );
    scene.append_child(root, red);

    scene.compute_layout(40.0, 20.0);
    let snapshot = render(&scene, 40, 20);
    let pixmap = snapshot.peek_pixels().expect("readable");

    assert_eq!(pixmap.get_color((5, 10)).r(), 0, "vacated left strip is background");
    assert_eq!(pixmap.get_color((25, 10)).r(), 255, "moved view covers 15..35");
    assert_eq!(pixmap.get_color((38, 10)).r(), 0, "past the moved right edge");
}

/// Skia `Group` scale around `origin` — previously only translate applied.
#[test]
fn group_scale_scales_its_children() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 40, "height": 40, "backgroundColor": [0.0, 0.0, 0.0, 1.0]}"#));
    scene.set_root(root);

    let canvas_node = scene.create_sk_node("Canvas");
    scene.set_style(canvas_node, style(r#"{"position": "absolute", "left": 0, "top": 0, "width": 40, "height": 40}"#));
    scene.append_child(root, canvas_node);

    let group = scene.create_sk_node("Group");
    scene.set_sk_props(
        group,
        serde_json::json!({
            "transform": [{"scale": 2.0}],
            "origin": {"x": 20.0, "y": 20.0},
        }),
    );
    scene.append_child(canvas_node, group);

    // 10x10 rect centered at (20, 20) — scaled x2 around that same origin
    // it must cover 10..30.
    let rect = scene.create_sk_node("Rect");
    scene.set_sk_props(rect, serde_json::json!({"rect": {"x": 15.0, "y": 15.0, "width": 10.0, "height": 10.0}, "color": [1.0, 0.0, 0.0, 1.0]}));
    scene.append_child(group, rect);

    scene.compute_layout(40.0, 40.0);
    let snapshot = render(&scene, 40, 40);
    let pixmap = snapshot.peek_pixels().expect("readable");

    assert_eq!(pixmap.get_color((12, 20)).r(), 255, "scaled rect reaches x=12 (unscaled started at 15)");
    assert_eq!(pixmap.get_color((28, 20)).r(), 255, "scaled rect reaches x=28 (unscaled ended at 25)");
    assert_eq!(pixmap.get_color((5, 20)).r(), 0, "outside even the scaled rect");
}
