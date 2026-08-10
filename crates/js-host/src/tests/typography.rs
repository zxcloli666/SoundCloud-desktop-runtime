//! Typography pipeline: real font weights, wrapping against Yoga's width
//! constraint, `letterSpacing`, `numberOfLines`, `textAlign`. All relative
//! assertions (bold vs regular, wrapped vs unwrapped) — nothing depends on
//! which exact typeface fontconfig resolves on the host.

use crate::scene::{Scene, StyleInput};

fn style(json: &str) -> StyleInput {
    serde_json::from_str(json).expect("valid style JSON")
}

/// wrapper(View with text styles) → Text child, inside a fixed-width root.
fn text_scene(wrapper_style: &str, text: &str, root_w: f32, root_h: f32) -> (Scene, u32) {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(&format!(r#"{{"width": {root_w}, "height": {root_h}, "alignItems": "flex-start"}}"#)));
    scene.set_root(root);
    let wrapper = scene.create_view();
    scene.set_style(wrapper, style(wrapper_style));
    scene.append_child(root, wrapper);
    let text_node = scene.create_text(text.to_string());
    scene.append_child(wrapper, text_node);
    scene.compute_layout(root_w, root_h);
    (scene, text_node)
}

fn ink_pixels(scene: &Scene, w: i32, h: i32) -> usize {
    let image_info = skia_safe::ImageInfo::new_n32_premul((w, h), None);
    let mut surface = skia_safe::surfaces::raster(&image_info, None, None).expect("raster surface");
    scene.draw(surface.canvas());
    let snapshot = surface.image_snapshot();
    let pixmap = snapshot.peek_pixels().expect("readable");
    let mut count = 0;
    for y in 0..h {
        for x in 0..w {
            if pixmap.get_color((x, y)).r() > 40 {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn bold_weight_resolves_a_heavier_typeface() {
    let (regular, text_r) = text_scene(r#"{"fontSize": 24}"#, "Weight", 400.0, 60.0);
    let (bold, text_b) = text_scene(r#"{"fontSize": 24, "fontWeight": "700"}"#, "Weight", 400.0, 60.0);
    let (_, _, w_r, _) = regular.layout_of(text_r);
    let (_, _, w_b, _) = bold.layout_of(text_b);
    let ink_r = ink_pixels(&regular, 400, 60);
    let ink_b = ink_pixels(&bold, 400, 60);
    // A real bold face inks more pixels (and nearly always measures wider);
    // identical numbers would mean the weight was silently dropped.
    assert!(
        ink_b as f32 > ink_r as f32 * 1.05 || w_b > w_r + 0.5,
        "bold should differ from regular: ink {ink_r} vs {ink_b}, width {w_r} vs {w_b}"
    );
}

#[test]
fn text_wraps_against_the_width_constraint() {
    let long = "one two three four five six seven eight nine ten";
    let (narrow, text_n) = text_scene(r#"{"fontSize": 16, "width": 120}"#, long, 120.0, 300.0);
    let (wide, text_w) = text_scene(r#"{"fontSize": 16}"#, long, 2000.0, 300.0);
    let (_, _, _, h_narrow) = narrow.layout_of(text_n);
    let (_, _, _, h_wide) = wide.layout_of(text_w);
    assert!(
        h_narrow >= h_wide * 2.0,
        "constrained text should wrap to multiple lines: narrow height {h_narrow}, single-line height {h_wide}"
    );
}

#[test]
fn number_of_lines_caps_the_wrapped_height() {
    let long = "one two three four five six seven eight nine ten";
    let (unlimited, text_u) = text_scene(r#"{"fontSize": 16, "width": 120}"#, long, 120.0, 300.0);
    let (capped, text_c) = text_scene(r#"{"fontSize": 16, "width": 120, "numberOfLines": 1}"#, long, 120.0, 300.0);
    let (_, _, _, h_unlimited) = unlimited.layout_of(text_u);
    let (_, _, _, h_capped) = capped.layout_of(text_c);
    assert!(
        h_capped < h_unlimited / 2.0,
        "numberOfLines: 1 should collapse to a single line: capped {h_capped}, unlimited {h_unlimited}"
    );
}

#[test]
fn letter_spacing_widens_the_measured_line() {
    let (plain, text_p) = text_scene(r#"{"fontSize": 16}"#, "TRACKING", 600.0, 60.0);
    let (tracked, text_t) = text_scene(r#"{"fontSize": 16, "letterSpacing": 4}"#, "TRACKING", 600.0, 60.0);
    let (_, _, w_plain, _) = plain.layout_of(text_p);
    let (_, _, w_tracked, _) = tracked.layout_of(text_t);
    // 7 gaps x 4px.
    assert!(
        (w_tracked - w_plain - 28.0).abs() < 2.0,
        "letterSpacing 4 over 8 chars should add ~28px: {w_plain} -> {w_tracked}"
    );
}

#[test]
fn text_align_center_moves_the_ink() {
    let left = text_scene(r#"{"fontSize": 16, "width": 300}"#, "hi", 300.0, 40.0).0;
    let centered = text_scene(r#"{"fontSize": 16, "width": 300, "textAlign": "center"}"#, "hi", 300.0, 40.0).0;

    let leftmost_ink = |scene: &Scene| -> i32 {
        let image_info = skia_safe::ImageInfo::new_n32_premul((300, 40), None);
        let mut surface = skia_safe::surfaces::raster(&image_info, None, None).expect("raster surface");
        scene.draw(surface.canvas());
        let snapshot = surface.image_snapshot();
        let pixmap = snapshot.peek_pixels().expect("readable");
        for x in 0..300 {
            for y in 0..40 {
                if pixmap.get_color((x, y)).r() > 40 {
                    return x;
                }
            }
        }
        -1
    };

    let left_x = leftmost_ink(&left);
    let center_x = leftmost_ink(&centered);
    assert!(left_x >= 0 && center_x >= 0, "both should render ink");
    assert!(
        center_x > left_x + 80,
        "centered text should start far right of left-aligned: {left_x} vs {center_x}"
    );
}

#[test]
fn line_height_stretches_the_block() {
    let long = "one two three four five six seven eight nine ten";
    let (normal, text_n) = text_scene(r#"{"fontSize": 16, "width": 120}"#, long, 120.0, 400.0);
    let (spaced, text_s) = text_scene(r#"{"fontSize": 16, "width": 120, "lineHeight": 40}"#, long, 120.0, 400.0);
    let (_, _, _, h_normal) = normal.layout_of(text_n);
    let (_, _, _, h_spaced) = spaced.layout_of(text_s);
    assert!(
        h_spaced > h_normal * 1.5,
        "lineHeight 40 should stretch the wrapped block: {h_normal} -> {h_spaced}"
    );
}
