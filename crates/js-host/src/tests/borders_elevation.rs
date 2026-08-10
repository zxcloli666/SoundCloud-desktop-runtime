//! Per-edge borders (separator strips — Titlebar/Sidebar dividers) and
//! `elevation`-only shadows.

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

#[test]
fn border_bottom_draws_a_separator_strip() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 40, "height": 40, "backgroundColor": [0.0, 0.0, 0.0, 1.0]}"#));
    scene.set_root(root);

    let bar = scene.create_view();
    scene.set_style(
        bar,
        style(r#"{"width": 40, "height": 20, "borderBottomWidth": 2, "borderBottomColor": [1.0, 0.0, 0.0, 1.0]}"#),
    );
    scene.append_child(root, bar);

    scene.compute_layout(40.0, 40.0);
    let snapshot = render(&scene, 40, 40);
    let pixmap = snapshot.peek_pixels().expect("readable");

    assert_eq!(pixmap.get_color((20, 19)).r(), 255, "bottom strip drawn at the bar's last rows");
    assert_eq!(pixmap.get_color((20, 10)).r(), 0, "bar interior untouched");
    assert_eq!(pixmap.get_color((20, 25)).r(), 0, "below the bar untouched");
}

#[test]
fn elevation_alone_synthesizes_a_drop_shadow() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 60, "height": 60, "backgroundColor": [1.0, 1.0, 1.0, 1.0], "alignItems": "center", "justifyContent": "center"}"#));
    scene.set_root(root);

    let card = scene.create_view();
    scene.set_style(card, style(r#"{"width": 20, "height": 20, "backgroundColor": [1.0, 1.0, 1.0, 1.0], "elevation": 6}"#));
    scene.append_child(root, card);

    scene.compute_layout(60.0, 60.0);
    let snapshot = render(&scene, 60, 60);
    let pixmap = snapshot.peek_pixels().expect("readable");

    // Below the card (shadow offset points down) the white root must darken.
    let below = pixmap.get_color((30, 43));
    assert!(below.r() < 250, "elevation should cast a visible shadow below the card, got r={}", below.r());
    // Far corner stays pure white.
    let corner = pixmap.get_color((3, 3));
    assert_eq!(corner.r(), 255, "far corner unaffected");
}
