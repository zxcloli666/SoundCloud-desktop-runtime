//! `Scene::insert_child_before` (react-reconciler's keyed-list reordering,
//! `hostConfig.ts`'s `insertBefore`) — pure Rust logic, no Hermes/JS
//! involved, same testing style as `hit_test_test`/`scroll_test`.

use crate::scene::{Scene, StyleInput};

fn style(json: &str) -> StyleInput {
    serde_json::from_str(json).expect("valid style JSON")
}

#[test]
fn fresh_insert_lands_at_the_requested_position() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 300, "height": 100}"#));
    scene.set_root(root);

    let a = scene.create_view();
    scene.append_child(root, a);
    let c = scene.create_view();
    scene.append_child(root, c);

    // b is a brand-new node, never appended anywhere yet — this is the
    // "initial mount ordering" case, not a reorder of an existing child.
    let b = scene.create_view();
    scene.insert_child_before(root, b, c);

    assert_eq!(scene.children_of(root), vec![a, b, c]);
}

#[test]
fn moving_an_existing_child_does_not_duplicate_it() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 300, "height": 100}"#));
    scene.set_root(root);

    let a = scene.create_view();
    scene.append_child(root, a);
    let b = scene.create_view();
    scene.append_child(root, b);
    let c = scene.create_view();
    scene.append_child(root, c);
    assert_eq!(scene.children_of(root), vec![a, b, c]);

    // Real keyed-list reorder: c already exists as root's 3rd child,
    // moves to sit right before a — must not leave a stale second entry
    // for c anywhere in the list.
    scene.insert_child_before(root, c, a);
    assert_eq!(scene.children_of(root), vec![c, a, b]);
}

#[test]
fn reordering_moves_paint_and_hit_test_order_too() {
    // Scene::hit_test walks children in reverse (topmost-last-painted
    // wins) — reordering the Vec must actually change which node a given
    // point resolves to, not just cosmetically reorder metadata.
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 100, "height": 100}"#));
    scene.set_root(root);

    let back = scene.create_view();
    scene.set_style(back, style(r#"{"width": 100, "height": 100, "position": "absolute", "left": 0, "top": 0}"#));
    scene.append_child(root, back);
    scene.watch_press(back);

    let front = scene.create_view();
    scene.set_style(front, style(r#"{"width": 100, "height": 100, "position": "absolute", "left": 0, "top": 0}"#));
    scene.append_child(root, front);
    scene.watch_press(front);

    scene.compute_layout(100.0, 100.0);
    // `front` was appended last, so it paints on top and wins the hit-test.
    assert_eq!(scene.hit_test(50.0, 50.0).map(|(id, _, _)| id), Some(front));

    // Move `front` to be first (painted first, so now underneath) — same
    // two nodes, same coordinates, the hit should flip to `back`.
    scene.insert_child_before(root, front, back);
    scene.compute_layout(100.0, 100.0);
    assert_eq!(scene.hit_test(50.0, 50.0).map(|(id, _, _)| id), Some(back));
}
