//! `Scene::scroll_by`/`hit_test_scrollable` — real ScrollView scrolling
//! (rn-linux's winit `MouseWheel` handler) is Rust-owned state, tested the
//! same way as `hit_test_test`: pure `Scene` API, no Hermes/JS needed.

use crate::scene::{Scene, StyleInput};

fn style(json: &str) -> StyleInput {
    serde_json::from_str(json).expect("valid style JSON")
}

/// A 100x100 scroll container with a 100x500 content child (mirrors
/// `ScrollView`'s two-View shape: outer clipping container + inner
/// content wrapper — `react-native.tsx`).
fn vertical_scroll_scene() -> (Scene, u32, u32) {
    let mut scene = Scene::new();
    let container = scene.create_view();
    scene.set_style(container, style(r#"{"width": 100, "height": 100, "overflow": "hidden", "scrollable": true, "scrollHorizontal": false}"#));
    scene.set_root(container);

    let content = scene.create_view();
    scene.set_style(content, style(r#"{"width": 100, "height": 500}"#));
    scene.append_child(container, content);

    scene.compute_layout(100.0, 100.0);
    (scene, container, content)
}

#[test]
fn hit_test_scrollable_finds_a_registered_container() {
    let (scene, container, _content) = vertical_scroll_scene();
    assert_eq!(scene.hit_test_scrollable(50.0, 50.0), Some(container));
    assert_eq!(scene.hit_test_scrollable(150.0, 150.0), None, "outside the container entirely");
}

#[test]
fn scrolling_shifts_where_a_child_hit_tests() {
    let mut scene = Scene::new();
    let container = scene.create_view();
    scene.set_style(container, style(r#"{"width": 100, "height": 100, "overflow": "hidden", "scrollable": true, "scrollHorizontal": false}"#));
    scene.set_root(container);

    let content = scene.create_view();
    scene.set_style(content, style(r#"{"width": 100, "height": 500}"#));
    scene.append_child(container, content);

    let button = scene.create_view();
    scene.set_style(button, style(r#"{"width": 100, "height": 40, "position": "absolute", "left": 0, "top": 200}"#));
    scene.append_child(content, button);
    scene.watch_press(button);

    scene.compute_layout(100.0, 100.0);
    // Button sits at content-relative y=200..240, well below the
    // container's own 100pt viewport — unscrolled, nothing to hit here.
    assert_eq!(scene.hit_test(50.0, 50.0).map(|(id, _, _)| id), None);

    scene.scroll_by(container, 0.0, 200.0);
    // Scrolled down 200pt, the button's content-space y=200 now lines
    // up with the container's own y=0 — hitting near the top now finds it.
    let (hit_id, local_x, local_y) = scene.hit_test(50.0, 10.0).expect("button should now be scrolled into view");
    assert_eq!(hit_id, button);
    assert_eq!((local_x, local_y), (50.0, 10.0));
}

#[test]
fn scroll_offset_clamps_to_content_size_minus_container_size() {
    let (mut scene, container, content) = vertical_scroll_scene();
    // A probe child placed exactly at the max-scroll boundary (content
    // is 500pt tall, container 100pt — max scroll is 400pt): if
    // scrolling clamps correctly, it should land exactly at the
    // container's own top edge once scrolled all the way down.
    let probe = scene.create_view();
    scene.set_style(probe, style(r#"{"width": 10, "height": 10, "position": "absolute", "left": 0, "top": 400}"#));
    scene.append_child(content, probe);
    scene.watch_press(probe);
    scene.compute_layout(100.0, 100.0);

    // Scroll far past the content's actual end.
    scene.scroll_by(container, 0.0, 10_000.0);

    let (hit_id, _, local_y) = scene.hit_test(5.0, 5.0).expect("the probe should be scrolled into view at the clamped max, not overshot past it");
    assert_eq!(hit_id, probe);
    assert_eq!(local_y, 5.0, "scrolling far past the content end should clamp to exactly the max, not overshoot");
}

#[test]
fn horizontal_scroll_maps_vertical_wheel_delta_to_the_x_axis() {
    let mut scene = Scene::new();
    let container = scene.create_view();
    scene.set_style(container, style(r#"{"width": 100, "height": 100, "overflow": "hidden", "scrollable": true, "scrollHorizontal": true}"#));
    scene.set_root(container);

    let content = scene.create_view();
    scene.set_style(content, style(r#"{"width": 500, "height": 100, "flexDirection": "row"}"#));
    scene.append_child(container, content);

    let item = scene.create_view();
    scene.set_style(item, style(r#"{"width": 50, "height": 100, "position": "absolute", "left": 200, "top": 0}"#));
    scene.append_child(content, item);
    scene.watch_press(item);

    scene.compute_layout(100.0, 100.0);
    assert_eq!(scene.hit_test(25.0, 50.0).map(|(id, _, _)| id), None, "item at x=200 isn't visible in the first 100pt yet");

    // A plain vertical wheel tick (dy only) — HorizontalScroll relies on
    // this mapping to x, since a mouse rarely has a horizontal wheel axis.
    scene.scroll_by(container, 0.0, 200.0);
    assert_eq!(scene.hit_test(25.0, 50.0).map(|(id, _, _)| id), Some(item), "vertical wheel delta should have scrolled the horizontal list");
}
