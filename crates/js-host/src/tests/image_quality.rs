//! Image sampling quality + the Skia `<Image>` node. Linear filtering must
//! produce intermediate colors when upscaling a hard checker edge (nearest —
//! the old default — only ever emits the two source colors), and a Skia
//! `<Image>` must draw a real decoded bitmap into its rect.

use crate::scene::{Scene, StyleInput};

fn style(json: &str) -> StyleInput {
    serde_json::from_str(json).expect("valid style JSON")
}

/// 2x2 black/white checker.
fn checker_image() -> skia_safe::Image {
    let image_info = skia_safe::ImageInfo::new_n32_premul((2, 2), None);
    let mut surface = skia_safe::surfaces::raster(&image_info, None, None).expect("raster surface");
    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::BLACK);
    let mut white = skia_safe::Paint::default();
    white.set_color(skia_safe::Color::WHITE);
    canvas.draw_rect(skia_safe::Rect::from_xywh(1.0, 0.0, 1.0, 1.0), &white);
    canvas.draw_rect(skia_safe::Rect::from_xywh(0.0, 1.0, 1.0, 1.0), &white);
    surface.image_snapshot()
}

fn render(scene: &Scene, w: i32, h: i32) -> skia_safe::Image {
    let image_info = skia_safe::ImageInfo::new_n32_premul((w, h), None);
    let mut surface = skia_safe::surfaces::raster(&image_info, None, None).expect("raster surface");
    scene.draw(surface.canvas());
    surface.image_snapshot()
}

#[test]
fn plain_image_upscale_is_linearly_filtered() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 32, "height": 32, "imageUri": "test://checker", "imageResizeMode": "stretch"}"#));
    scene.set_root(root);
    scene.set_image(root, Some(checker_image()));
    scene.compute_layout(32.0, 32.0);

    let snapshot = render(&scene, 32, 32);
    let pixmap = snapshot.peek_pixels().expect("readable");

    // Sweep the horizontal center line: linear sampling blends the checker
    // columns into in-between grays somewhere; nearest never does.
    let mut has_intermediate = false;
    for x in 0..32 {
        let c = pixmap.get_color((x, 8)).r();
        if c > 40 && c < 215 {
            has_intermediate = true;
            break;
        }
    }
    assert!(has_intermediate, "upscaled checker should show linearly blended pixels, not pure black/white");
}

#[test]
fn sk_image_draws_the_decoded_bitmap() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 40, "height": 40, "backgroundColor": [1.0, 0.0, 0.0, 1.0]}"#));
    scene.set_root(root);

    let canvas_node = scene.create_sk_node("Canvas");
    scene.set_style(canvas_node, style(r#"{"position": "absolute", "left": 0, "top": 0, "width": 40, "height": 40}"#));
    scene.append_child(root, canvas_node);

    let image_node = scene.create_sk_node("Image");
    scene.set_sk_props(
        image_node,
        serde_json::json!({
            "image": {"uri": "test://checker"},
            "x": 10.0, "y": 10.0, "width": 20.0, "height": 20.0,
            "fit": "fill",
        }),
    );
    scene.append_child(canvas_node, image_node);
    // Bypass the network half of the pipe — inject the decoded bitmap the
    // way rn-linux's drain_ready() loop would.
    scene.set_image(image_node, Some(checker_image()));

    scene.compute_layout(40.0, 40.0);
    let snapshot = render(&scene, 40, 40);
    let pixmap = snapshot.peek_pixels().expect("readable");

    // Checker corners inside the image rect (10..30): top-left black-ish,
    // top-right white-ish.
    let tl = pixmap.get_color((12, 12));
    let tr = pixmap.get_color((27, 12));
    assert!(tl.r() < 90 && tl.g() < 90, "image top-left quadrant should be dark, got r={}", tl.r());
    assert!(tr.r() > 165, "image top-right quadrant should be light, got r={}", tr.r());
    // Outside the image rect the red background is untouched.
    let outside = pixmap.get_color((5, 20));
    assert_eq!((outside.r(), outside.g(), outside.b()), (255, 0, 0), "outside the sk image rect stays background");
}
