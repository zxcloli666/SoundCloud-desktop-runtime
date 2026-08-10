//! Gradient shaders in an *offset* Canvas (bug #3) — shader geometry is
//! canvas-local, so a gradient must land on its shape wherever the Canvas
//! sits in the window. Before the fix, shaders stayed anchored to the
//! window origin: an offset canvas's linear gradient rendered as a single
//! clamped color. Pure Rust + real Skia offscreen rendering, no Hermes/JS.

use crate::scene::{Scene, StyleInput};

fn style(json: &str) -> StyleInput {
    serde_json::from_str(json).expect("valid style JSON")
}

fn draw_to_pixmap(scene: &Scene, w: i32, h: i32) -> skia_safe::Image {
    let image_info = skia_safe::ImageInfo::new_n32_premul((w, h), None);
    let mut surface = skia_safe::surfaces::raster(&image_info, None, None).expect("raster surface");
    scene.draw(surface.canvas());
    surface.image_snapshot()
}

/// A black→white horizontal linear gradient across a 20px rect inside a
/// Canvas at window x=40. Clamped-at-origin rendering (the bug) would paint
/// the whole rect the end-stop white; correct rendering keeps the left edge
/// near-black and the right edge near-white.
#[test]
fn linear_gradient_follows_its_offset_canvas() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 60, "height": 20, "flexDirection": "row"}"#));
    scene.set_root(root);

    let spacer = scene.create_view();
    scene.set_style(spacer, style(r#"{"width": 40, "height": 20}"#));
    scene.append_child(root, spacer);

    let canvas_node = scene.create_sk_node("Canvas");
    scene.set_style(canvas_node, style(r#"{"width": 20, "height": 20}"#));
    scene.append_child(root, canvas_node);

    let rect = scene.create_sk_node("Rect");
    scene.set_sk_props(rect, serde_json::json!({"rect": {"x": 0, "y": 0, "width": 20, "height": 20}}));
    scene.append_child(canvas_node, rect);

    let gradient = scene.create_sk_node("LinearGradient");
    scene.set_sk_props(
        gradient,
        serde_json::json!({
            "start": {"x": 0.0, "y": 0.0},
            "end": {"x": 20.0, "y": 0.0},
            "colors": [[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]],
        }),
    );
    scene.append_child(rect, gradient);

    scene.compute_layout(60.0, 20.0);
    let snapshot = draw_to_pixmap(&scene, 60, 20);
    let pixmap = snapshot.peek_pixels().expect("raster surface should be readable");

    let left = pixmap.get_color((41, 10));
    let right = pixmap.get_color((58, 10));
    assert!(left.r() < 60, "left edge of the offset gradient should be near-black, got r={}", left.r());
    assert!(right.r() > 195, "right edge of the offset gradient should be near-white, got r={}", right.r());
}

/// A radial gradient with an explicit canvas-local center `c` on a circle in
/// a Canvas at window (40, 0) — the white core must sit on the circle's own
/// center, not at the window-origin projection of `c`.
#[test]
fn radial_gradient_center_follows_its_offset_canvas() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 60, "height": 20, "flexDirection": "row"}"#));
    scene.set_root(root);

    let spacer = scene.create_view();
    scene.set_style(spacer, style(r#"{"width": 40, "height": 20}"#));
    scene.append_child(root, spacer);

    let canvas_node = scene.create_sk_node("Canvas");
    scene.set_style(canvas_node, style(r#"{"width": 20, "height": 20}"#));
    scene.append_child(root, canvas_node);

    let circle = scene.create_sk_node("Circle");
    scene.set_sk_props(circle, serde_json::json!({"c": {"x": 10.0, "y": 10.0}, "r": 10.0}));
    scene.append_child(canvas_node, circle);

    let gradient = scene.create_sk_node("RadialGradient");
    scene.set_sk_props(
        gradient,
        serde_json::json!({
            "c": {"x": 10.0, "y": 10.0},
            "r": 10.0,
            "colors": [[1.0, 1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0]],
        }),
    );
    scene.append_child(circle, gradient);

    scene.compute_layout(60.0, 20.0);
    let snapshot = draw_to_pixmap(&scene, 60, 20);
    let pixmap = snapshot.peek_pixels().expect("raster surface should be readable");

    let center = pixmap.get_color((50, 10));
    let rim = pixmap.get_color((42, 10));
    assert!(center.r() > 195, "circle center should be near-white, got r={}", center.r());
    assert!(rim.r() < 90, "circle rim should be near-black, got r={}", rim.r());
}
