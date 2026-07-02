//! `Scene::hit_test` (pointer input, rn-linux's winit event loop) is pure
//! Rust logic against real Yoga layout — no Hermes/JS involved, so it's
//! tested directly through `Scene`'s own API rather than through a bundle.

use crate::scene::{Scene, StyleInput};

fn style(json: &str) -> StyleInput {
    serde_json::from_str(json).expect("valid style JSON")
}

#[test]
fn finds_the_watched_pressable_and_ignores_an_unwatched_sibling() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 300, "height": 300}"#));
    scene.set_root(root);

    let plain = scene.create_view();
    scene.set_style(plain, style(r#"{"width": 100, "height": 100, "position": "absolute", "left": 0, "top": 0}"#));
    scene.append_child(root, plain);

    let pressable = scene.create_view();
    scene.set_style(pressable, style(r#"{"width": 100, "height": 100, "position": "absolute", "left": 100, "top": 0}"#));
    scene.append_child(root, pressable);
    scene.watch_press(pressable);

    scene.compute_layout(300.0, 300.0);

    assert_eq!(scene.hit_test(150.0, 50.0).map(|(id, _, _)| id), Some(pressable), "inside the watched pressable");
    assert_eq!(scene.hit_test(50.0, 50.0), None, "inside the plain view — never watched, so invisible to hit-testing");
    assert_eq!(scene.hit_test(250.0, 250.0), None, "outside every node");
}

#[test]
fn a_nested_pressable_wins_over_its_pressable_ancestor() {
    // Matches real touch semantics: tapping a Button inside a pressable
    // Card should hit the Button, not bubble to the Card underneath it.
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 200, "height": 200}"#));
    scene.set_root(root);

    let card = scene.create_view();
    scene.set_style(card, style(r#"{"width": 200, "height": 200}"#));
    scene.append_child(root, card);
    scene.watch_press(card);

    let button = scene.create_view();
    scene.set_style(button, style(r#"{"width": 50, "height": 50, "position": "absolute", "left": 75, "top": 75}"#));
    scene.append_child(card, button);
    scene.watch_press(button);

    scene.compute_layout(200.0, 200.0);

    assert_eq!(scene.hit_test(100.0, 100.0).map(|(id, _, _)| id), Some(button), "tapping inside the button should hit it, not the card behind it");
    assert_eq!(scene.hit_test(10.0, 10.0).map(|(id, _, _)| id), Some(card), "tapping elsewhere on the card should still hit the card");
}

#[test]
fn local_coordinates_are_relative_to_the_hit_nodes_own_origin() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 300, "height": 300}"#));
    scene.set_root(root);

    let pressable = scene.create_view();
    scene.set_style(pressable, style(r#"{"width": 100, "height": 60, "position": "absolute", "left": 50, "top": 20}"#));
    scene.append_child(root, pressable);
    scene.watch_press(pressable);
    scene.compute_layout(300.0, 300.0);

    let (id, local_x, local_y) = scene.hit_test(70.0, 45.0).expect("should hit the pressable");
    assert_eq!(id, pressable);
    assert_eq!((local_x, local_y), (20.0, 25.0));
}

#[test]
fn unwatch_press_makes_a_node_invisible_to_hit_testing_again() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 100, "height": 100}"#));
    scene.set_root(root);
    scene.watch_press(root);
    scene.compute_layout(100.0, 100.0);
    assert!(scene.hit_test(50.0, 50.0).is_some());

    scene.unwatch_press(root);
    assert!(scene.hit_test(50.0, 50.0).is_none());
}
