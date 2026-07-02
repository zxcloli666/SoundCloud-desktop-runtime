//! Hermes embedded in our own Rust process via `rusty_hermes` (safe wrapper
//! over Hermes' JSI). Owns the mounted scene tree (`scene`) and the host
//! functions (`host`) a JS-side `react-reconciler` host-config calls into —
//! this is where Fabric's job (mounting + Yoga layout) happens for us.

pub mod dto_json;
pub mod host;
pub mod image_cache;
pub mod live_data;
pub mod scene;

pub use rusty_hermes::Runtime;
pub use scene::Scene;

/// Mirrors rn-linux's per-frame pump (drain due timers, then microtasks they
/// produced). `ConcurrentRoot`'s initial mount schedules its commit through
/// that same path instead of completing inline inside a single `eval()` call
/// (unlike the old `LegacyRoot` + forced `flushSync` setup) — tests that load
/// the real bundle need to pump a few frames before the scene tree exists.
#[cfg(test)]
fn pump_frames(rt: &Runtime, count: u32) {
    for _ in 0..count {
        rt.eval("if (typeof __scDrainTimers === 'function') __scDrainTimers();").expect("drain timers failed");
        rt.drain_microtasks().expect("drain microtasks failed");
    }
}

#[cfg(test)]
mod tests {
    use rusty_hermes::{Runtime, hermes_op};

    #[hermes_op]
    fn add(a: f64, b: f64, c: f64) -> f64 {
        a + b + c
    }

    #[test]
    fn evaluates_js_and_calls_back_into_rust() {
        let rt = Runtime::new().expect("failed to create Hermes runtime");
        add::register(&rt).expect("failed to register add()");

        let result = rt.eval("add(10, 20, 30)").expect("eval failed");
        assert_eq!(result.as_number(), Some(60.0));
    }

    #[test]
    fn mounts_a_tree_via_host_functions_and_computes_layout() {
        let rt = Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");

        rt.eval(
            r#"
            const root = __scCreateView();
            __scSetStyle(root, JSON.stringify({ width: 400, height: 200, flexDirection: "row" }));

            const a = __scCreateView();
            __scSetStyle(a, JSON.stringify({ flexGrow: 1, backgroundColor: [0.2, 0.4, 0.9, 1.0] }));
            __scAppendChild(root, a);

            const b = __scCreateView();
            __scSetStyle(b, JSON.stringify({ flexGrow: 1, backgroundColor: [0.9, 0.3, 0.5, 1.0] }));
            __scAppendChild(root, b);

            __scSetRoot(root);
            "#,
        )
        .expect("eval failed");

        super::host::with_scene(|scene| {
            scene.compute_layout(400.0, 200.0);
            let root = scene.root.expect("root should be set");
            assert_eq!(scene.layout_of(root), (0.0, 0.0, 400.0, 200.0));

            // Two equal flex-grow children in a row should split 400pt evenly.
            let children = scene.children_of(root);
            assert_eq!(children.len(), 2);
            assert_eq!(scene.layout_of(children[0]), (0.0, 0.0, 200.0, 200.0));
            assert_eq!(scene.layout_of(children[1]), (200.0, 0.0, 200.0, 200.0));
        });
    }
}

/// Guards real Skia-based text measurement (`scene.rs`'s `measure_text`,
/// Yoga's measure-function hook) against regressing back to the old
/// `chars().count() * 8.0` heuristic — every assertion here would pass
/// under a fixed-width-per-character guess too, EXCEPT the font-size one,
/// which only real per-glyph measurement can satisfy.
#[cfg(test)]
mod text_metrics_test {
    /// Builds `__scCreateView()` wrapping `__scCreateText(text)`, with
    /// `fontSize` set on the *View* (matches `react-native.tsx`'s `Text`
    /// shim: `<View style={{fontSize, ...}}>{string}</View>`), and returns
    /// the text child's natural (unconstrained) measured width.
    ///
    /// `alignItems: "flex-start"` on both container levels — Yoga's default
    /// cross-axis alignment (`stretch`) would otherwise stretch `wrap`, and
    /// then the text node itself, to the full 2000pt root width regardless
    /// of what `measure_text` reports, defeating the point of this helper.
    /// Real `@sc/ui` usage never hits this because its containers always
    /// have their own explicit (usually much narrower) width.
    fn measured_text_width(rt: &super::Runtime, text: &str, font_size: f32) -> f32 {
        rt.eval(&format!(
            r#"
            const root = __scCreateView();
            __scSetStyle(root, JSON.stringify({{ width: 2000, height: 200, alignItems: "flex-start" }}));
            const wrap = __scCreateView();
            __scSetStyle(wrap, JSON.stringify({{ fontSize: {font_size}, alignItems: "flex-start" }}));
            const text = __scCreateText({text:?});
            __scAppendChild(wrap, text);
            __scAppendChild(root, wrap);
            __scSetRoot(root);
            "#,
        ))
        .expect("eval failed");

        super::host::with_scene(|scene| {
            scene.compute_layout(2000.0, 200.0);
            let root = scene.root.expect("root should be set");
            let wrap = scene.children_of(root)[0];
            let text_child = scene.children_of(wrap)[0];
            scene.layout_of(text_child).2
        })
    }

    #[test]
    fn longer_text_measures_wider_at_the_same_font_size() {
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");
        let short = measured_text_width(&rt, "A", 16.0);
        let long = measured_text_width(&rt, "A much longer string of text", 16.0);
        assert!(long > short * 2.0, "a much longer string should measure a lot wider, got short={short} long={long}");
    }

    #[test]
    fn larger_font_size_measures_wider_for_the_same_text() {
        // Only real per-glyph Skia measurement can distinguish this — a
        // `chars().count() * 8.0` heuristic ignores font size entirely and
        // would report the exact same width for both.
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");
        let small = measured_text_width(&rt, "SoundCloud", 12.0);
        let large = measured_text_width(&rt, "SoundCloud", 32.0);
        assert!(large > small * 1.5, "the same text at a much bigger font size should measure a lot wider, got small={small} large={large}");
    }

    #[test]
    fn text_node_shrinks_below_its_natural_width_in_a_tight_flex_container() {
        // `numberOfLines={1}` truncation (Card/TrackRow) only makes sense if
        // the text node's *final* layout width can actually end up smaller
        // than its natural measured width — real `@sc/ui` usage always wraps
        // Text in a container with its own (usually narrower) explicit
        // width, and Yoga's default cross-axis `stretch` does the rest.
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");
        let natural = measured_text_width(&rt, "This text is definitely too long to fit", 16.0);

        rt.eval(&format!(
            r#"
            const root = __scCreateView();
            __scSetStyle(root, JSON.stringify({{ width: 60, height: 200 }}));
            const wrap = __scCreateView();
            __scSetStyle(wrap, JSON.stringify({{ fontSize: 16, width: 60 }}));
            const text = __scCreateText({:?});
            __scAppendChild(wrap, text);
            __scAppendChild(root, wrap);
            __scSetRoot(root);
            "#,
            "This text is definitely too long to fit",
        ))
        .expect("eval failed");
        let shrunk = super::host::with_scene(|scene| {
            scene.compute_layout(60.0, 200.0);
            let root = scene.root.expect("root should be set");
            let wrap = scene.children_of(root)[0];
            let text_child = scene.children_of(wrap)[0];
            scene.layout_of(text_child).2
        });

        assert!(shrunk < natural, "a text node inside a 60pt-wide container should shrink well below its {natural}pt natural width, got {shrunk}");
        assert!((shrunk - 60.0).abs() < 1.0, "should shrink down to (approximately) the container's own width, got {shrunk}");
    }
}

/// Guards the `js/` bundle against regressions: react-reconciler's host-config
/// surface has grown undocumented required methods across React versions
/// (see Desktop-Runtime/CLAUDE.md) — this catches it breaking again without
/// needing a GPU window. Requires `pnpm build` in `js/` to have run first.
#[cfg(test)]
mod bundle_test {
    #[test]
    fn react_reconciler_bundle_mounts_a_tree() {
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");
        let bundle = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../js/dist/bundle.js"
        ))
        .unwrap_or_else(|e| panic!("read js/dist/bundle.js: {e} (run `pnpm build` in js/)"));
        rt.eval(&bundle).expect("bundle JS failed");
        // ConcurrentRoot schedules its initial commit through the same
        // deferred-timer path as any other update — it doesn't complete
        // inline within the `eval()` call above. See `pump_frames`.
        super::pump_frames(&rt, 10);

        super::host::with_scene(|scene| {
            let root = scene.root.expect("bundle should have set a scene root");
            scene.compute_layout(1024.0, 640.0);
            let (_, _, w, h) = scene.layout_of(root);
            assert!(w > 0.0 && h > 0.0, "root should have a non-empty layout");
        });
    }

    /// Proves `hostConfig.ts`'s press-registration path for real — not just
    /// `Scene::hit_test`'s own algorithm (see `hit_test_test`, which calls
    /// `watch_press` directly, bypassing JS entirely). The demo bundle's
    /// `CoreUiProbe` mounts several real `@sc/ui` components with `onPress`
    /// set (Button/Card/TrackRow); if `<Pressable onPress={...}>` reaching
    /// `applyStyle` correctly calls `__scWatchPress`, at least one real,
    /// on-screen coordinate should hit-test successfully.
    #[test]
    fn real_pressable_components_register_with_scene_hit_test() {
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");
        let bundle = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../js/dist/bundle.js"
        ))
        .unwrap_or_else(|e| panic!("read js/dist/bundle.js: {e} (run `pnpm build` in js/)"));
        rt.eval(&bundle).expect("bundle JS failed");
        super::pump_frames(&rt, 10);

        let (width, height) = (1024.0, 640.0);
        let found_a_hit = super::host::with_scene(|scene| {
            scene.compute_layout(width, height);
            let (cols, rows) = (40, 40);
            for i in 0..cols {
                for j in 0..rows {
                    let x = width * (i as f32 + 0.5) / cols as f32;
                    let y = height * (j as f32 + 0.5) / rows as f32;
                    if scene.hit_test(x, y).is_some() {
                        return true;
                    }
                }
            }
            false
        });
        assert!(found_a_hit, "expected at least one registered pressable node somewhere in the demo tree (Button/Card/TrackRow all set onPress)");
    }

    /// Guards a real bug found manually verifying scroll (task #19): the
    /// content wrapper `ScrollView` renders around a real `<Card>` list is a
    /// column-direction child of the (also column-direction) outer clipping
    /// container, so Yoga's default `alignItems: stretch` clamped its width
    /// to match the container's — exactly the one dimension a *horizontal*
    /// scroll's content needs to size naturally past the container (nothing
    /// to scroll otherwise). `HorizontalScroll` in the demo bundle is
    /// deliberately narrower (`width: 200`) than its three `Card`s combined.
    #[test]
    fn horizontal_scroll_content_is_wider_than_its_narrow_container() {
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");
        let bundle = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../js/dist/bundle.js"
        ))
        .unwrap_or_else(|e| panic!("read js/dist/bundle.js: {e} (run `pnpm build` in js/)"));
        rt.eval(&bundle).expect("bundle JS failed");
        super::pump_frames(&rt, 10);

        super::host::with_scene(|scene| {
            scene.compute_layout(1024.0, 640.0);
            let scrollables = scene.scrollable_node_ids();
            assert!(!scrollables.is_empty(), "the demo's HorizontalScroll should have registered as scrollable");
            let found_overflowing_content = scrollables.iter().any(|&container| {
                let (_, _, container_w, _) = scene.layout_of(container);
                scene.children_of(container).first().is_some_and(|&content| {
                    let (_, _, content_w, _) = scene.layout_of(content);
                    content_w > container_w
                })
            });
            assert!(found_overflowing_content, "HorizontalScroll's content should measure wider than its container — otherwise there's nothing to scroll");
        });
    }
}

/// Spike 6: proves the reanimated tick loop actually advances a `withTiming`
/// animation over real wall-clock time — no GPU window needed, since the
/// interpolated value shows up as a real Yoga-computed layout width.
#[cfg(test)]
mod reanimated_test {
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn with_timing_animation_advances_and_settles() {
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");
        let bundle = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../js/dist/bundle.js"
        ))
        .unwrap_or_else(|e| panic!("read js/dist/bundle.js: {e} (run `pnpm build` in js/)"));
        rt.eval(&bundle).expect("bundle JS failed");
        // ConcurrentRoot's initial commit needs a frame pump too — see
        // `pump_frames` and `bundle_test`.
        super::pump_frames(&rt, 10);

        let badge_width = |rt: &super::Runtime| -> f32 {
            rt.eval("if (typeof __reanimatedTick === 'function') __reanimatedTick();").expect("tick failed");
            super::host::with_scene(|scene| {
                let root = scene.root.expect("bundle should have set a scene root");
                scene.compute_layout(1024.0, 640.0);
                // PulseBadge is App's last child (see index.tsx) — robust to
                // however many siblings render before it.
                let badge = *scene.children_of(root).last().expect("root should have children");
                scene.layout_of(badge).2
            })
        };

        let initial = badge_width(&rt);
        assert_eq!(initial, 24.0, "badge should start at its useSharedValue(24) initial width");

        sleep(Duration::from_millis(300));
        let mid = badge_width(&rt);
        assert!(mid > initial, "width should have grown partway through the 1200ms withTiming");

        sleep(Duration::from_millis(1200));
        let settled = badge_width(&rt);
        assert_eq!(settled, 220.0, "animation should settle exactly at its withTiming target");
    }
}

/// Tiling window managers hand out whatever aspect ratio fits their layout,
/// ignoring the app's requested size (found while chasing a screenshot that
/// looked cut off at the requested 1024x640 — the actual window was
/// 847x1388). This renders offscreen (no GPU window needed) at that same
/// unusual aspect ratio and checks the root's background genuinely covers
/// it, guarding against the root/Canvas silently falling back to a fixed size.
#[cfg(test)]
mod fills_arbitrary_aspect_ratio_test {
    #[test]
    fn root_background_covers_a_tall_narrow_window() {
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");
        let bundle = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../js/dist/bundle.js"
        ))
        .unwrap_or_else(|e| panic!("read js/dist/bundle.js: {e} (run `pnpm build` in js/)"));
        rt.eval(&bundle).expect("bundle JS failed");
        // ConcurrentRoot's initial commit needs a frame pump too — see
        // `pump_frames` and `bundle_test`.
        super::pump_frames(&rt, 10);

        let (width, height) = (847, 1388);
        let image_info = skia_safe::ImageInfo::new_n32_premul((width, height), None);
        let mut surface = skia_safe::surfaces::raster(&image_info, None, None).expect("raster surface");
        super::host::with_scene(|scene| {
            scene.compute_layout(width as f32, height as f32);
            scene.draw(surface.canvas());
        });

        let image = surface.image_snapshot();
        let pixmap = image.peek_pixels().expect("raster surface should be readable");
        // Root's backgroundColor is [0.04, 0.05, 0.08, 1.0]. Sample a few
        // pixels in from the left edge, near the bottom of the actual window
        // rather than a fixed old size — every demo child (Scene/LiveDataProbe/
        // CoreUiProbe/PulseBadge) has its own margin, so this stays clear of
        // their tinted backgrounds regardless of how tall the demo tree grows.
        let color = pixmap.get_color((5, height - 5));
        assert_eq!((color.r(), color.g(), color.b()), (10, 13, 20), "root background should reach the true window bottom");
    }
}

/// Documents a genuine Hermes engine bug found while wiring up real `@sc/ui`
/// (spike 7): a `for (let key of ...)` loop whose body defines a closure via
/// `Object.defineProperty` doesn't get a fresh `key` binding per iteration —
/// every getter ends up seeing the *last* key. This is exactly the shape of
/// esbuild's own CJS→ESM interop helper (`__copyProps`), which `js/build.mjs`
/// patches post-build (swaps the loop for `.forEach`, where `key` is a
/// function parameter instead of a loop-scoped `let`). If Hermes ever fixes
/// this, both `createContextType` assertions below would need to flip to
/// "function" — that's the signal to remove the build.mjs patch too.
#[cfg(test)]
mod hermes_for_of_let_closure_bug_test {
    #[test]
    fn reproduces_in_isolation() {
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        let js = r#"
        var __getOwnPropNames = Object.getOwnPropertyNames;
        var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
        var __hasOwnProp = Object.prototype.hasOwnProperty;
        var __defProp = Object.defineProperty;
        var copyPropsForOf = (to, from, except, desc) => {
          if (from && typeof from === "object" || typeof from === "function") {
            for (let key of __getOwnPropNames(from))
              if (!__hasOwnProp.call(to, key) && key !== except)
                __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
          }
          return to;
        };

        var fakeModule = { firstFn: function () { return "ok"; }, version: "1.2.3" };
        var wrapped = copyPropsForOf({}, fakeModule);
        JSON.stringify({ firstFnType: typeof wrapped.firstFn, firstFnValue: String(wrapped.firstFn) });
        "#;
        let result = rt.eval(js).expect("eval failed").into_string().expect("result should be a string").to_rust_string().expect("valid utf8");
        assert_eq!(
            result,
            r#"{"firstFnType":"string","firstFnValue":"1.2.3"}"#,
            "if this ever reads back as a function, the Hermes bug is fixed — go remove the build.mjs __copyProps patch",
        );
    }
}

/// `Scene::hit_test` (pointer input, rn-linux's winit event loop) is pure
/// Rust logic against real Yoga layout — no Hermes/JS involved, so it's
/// tested directly through `Scene`'s own API rather than through a bundle.
#[cfg(test)]
mod hit_test_test {
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
}

/// `Scene::scroll_by`/`hit_test_scrollable` — real ScrollView scrolling
/// (rn-linux's winit `MouseWheel` handler) is Rust-owned state, tested the
/// same way as `hit_test_test`: pure `Scene` API, no Hermes/JS needed.
#[cfg(test)]
mod scroll_test {
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
}

/// Spike 7b: proves the whole async round-trip actually works — JS calls a
/// host function with a callback id, `sc_rn::auth_status()` runs to
/// completion on `live_data`'s background tokio runtime, and its result
/// reaches JS as a resolved Promise value, entirely through the same
/// `deliver()` polling rn-linux's render loop uses (no test-only shortcut).
#[cfg(test)]
mod live_data_test {
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn auth_status_round_trips_through_the_real_async_bridge() {
        let rt = super::Runtime::new().expect("failed to create Hermes runtime");
        super::host::install(&rt).expect("failed to install host functions");

        let tmp = std::env::temp_dir().join(format!("sc-rn-live-data-test-{}", std::process::id()));
        let data_dir = tmp.join("data");
        let cache_dir = tmp.join("cache");
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        std::fs::create_dir_all(&cache_dir).expect("create cache dir");

        let init_err = rt
            .eval(&format!(
                "__scInitCore({:?}, {:?}, false)",
                data_dir.to_str().unwrap(),
                cache_dir.to_str().unwrap(),
            ))
            .expect("init eval failed")
            .into_string()
            .expect("init_core returns a string")
            .to_rust_string()
            .expect("valid utf8");
        assert_eq!(init_err, "", "sc-rn init_runtime failed: {init_err}");

        rt.eval(
            r#"
            globalThis.__testDone = false;
            globalThis.__testOk = null;
            globalThis.__testPayload = null;
            globalThis.__scDeliverResult = function (id, ok, payload) {
                if (id !== 100001) return;
                globalThis.__testDone = true;
                globalThis.__testOk = ok;
                globalThis.__testPayload = payload;
            };
            // A distinctive, out-of-range callback id — not 1: `live_data`'s
            // tokio runtime/mpsc channel (js-host/src/live_data.rs) is one
            // process-global instance shared by every `Runtime::new()` in
            // this test binary. The bundle's own `live-data.ts` module
            // (evaluated by bundle_test/reanimated_test/
            // fills_arbitrary_aspect_ratio_test, each a *separate* Hermes
            // runtime) fires its `LiveDataProbe`'s `authStatus()` with
            // callback ids starting at 1 too — a small hardcoded id here
            // could receive one of *their* stale results instead of this
            // call's own.
            __scAuthStatus(100001);
            "#,
        )
        .expect("eval failed");

        // Mirrors rn-linux's render loop: poll `deliver()` once per "frame"
        // instead of blocking on the background runtime directly.
        for _ in 0..200 {
            super::live_data::deliver(&rt);
            let done = rt.eval("globalThis.__testDone").expect("poll eval failed").as_bool().unwrap_or(false);
            if done {
                break;
            }
            sleep(Duration::from_millis(25));
        }

        let done = rt.eval("globalThis.__testDone").expect("poll eval failed").as_bool().unwrap_or(false);
        assert!(done, "auth_status() should have resolved or rejected within 5s");

        let ok = rt.eval("globalThis.__testOk").expect("poll eval failed").as_bool().expect("ok should be a bool");
        let debug_payload = rt.eval("String(globalThis.__testPayload)").expect("poll eval failed").into_string().expect("string").to_rust_string().expect("utf8");
        assert!(ok, "auth_status() should succeed with a fresh, empty data dir (no error expected): {debug_payload}");

        let payload = rt
            .eval("JSON.stringify(globalThis.__testPayload)")
            .expect("stringify failed")
            .into_string()
            .expect("result should be a string")
            .to_rust_string()
            .expect("valid utf8");
        let payload: serde_json::Value = serde_json::from_str(&payload).expect("payload should be valid JSON");
        assert_eq!(
            payload,
            serde_json::json!({
                "hasSession": false,
                "authenticated": false,
                "sessionId": null,
                "username": null,
                "tokenState": null,
            }),
            "a fresh sc-rn runtime with no stored session should report has_session=false",
        );

        // `home_clusters`/`wave`/`resolve_tracks` need real auth/network and
        // will legitimately fail unauthenticated — this isn't asserting they
        // succeed, just that their callback_id plumbing and `dto_json`
        // builders don't panic or hang on a second, concurrent in-flight
        // call multiplexed through the same `deliver()` channel.
        rt.eval(
            r#"
            globalThis.__testDone2 = false;
            __scHomeClusters(100002, 5, JSON.stringify([]), false);
            "#,
        )
        .expect("eval failed");
        rt.eval(
            r#"
            var __origDeliver = globalThis.__scDeliverResult;
            globalThis.__scDeliverResult = function (id, ok, payload) {
                if (id === 100002) { globalThis.__testDone2 = true; return; }
                __origDeliver(id, ok, payload);
            };
            "#,
        )
        .expect("eval failed");

        for _ in 0..200 {
            super::live_data::deliver(&rt);
            let done = rt.eval("globalThis.__testDone2").expect("poll eval failed").as_bool().unwrap_or(false);
            if done {
                break;
            }
            sleep(Duration::from_millis(25));
        }
        let done2 = rt.eval("globalThis.__testDone2").expect("poll eval failed").as_bool().unwrap_or(false);
        assert!(done2, "home_clusters() should have resolved or rejected within 5s, not hung");

        std::fs::remove_dir_all(&tmp).ok();
    }
}

/// Real network fetch + real Skia decode for `<Image>` (task #20) — same
/// "hit a real external endpoint, poll like rn-linux's render loop does"
/// approach as `live_data_test` (no test-only shortcut, no mock server).
#[cfg(test)]
mod image_cache_test {
    use std::thread::sleep;
    use std::time::Duration;

    use crate::scene::NodeId;

    fn wait_for_result(id: NodeId) -> Option<skia_safe::Image> {
        for _ in 0..200 {
            for (ready_id, image) in super::image_cache::drain_ready() {
                if ready_id == id {
                    return image;
                }
            }
            sleep(Duration::from_millis(25));
        }
        panic!("image fetch for node {id} did not complete within 5s");
    }

    // `image_cache`'s state is process-global, keyed by bare NodeId — the
    // demo bundle (`bundle_test`) now fetches real images too, on whatever
    // small, sequentially-allocated ids its own Scene happens to assign.
    // Distinctive, out-of-range ids here avoid the exact class of collision
    // `live_data_test` hit earlier (see its callback-id comment).

    #[test]
    fn fetches_and_decodes_a_real_image() {
        let id: NodeId = 200_001;
        super::image_cache::request(id, "https://picsum.photos/id/237/200/200.jpg".to_string());
        let image = wait_for_result(id).expect("a real, reachable JPEG URL should decode successfully");
        assert!(image.width() > 0 && image.height() > 0, "decoded image should have real dimensions");
    }

    #[test]
    fn a_bad_url_resolves_to_no_image_rather_than_hanging_or_panicking() {
        let id: NodeId = 200_002;
        super::image_cache::request(id, "https://picsum.photos/id/237/200/200.jpg/this-path-does-not-exist-404".to_string());
        assert!(wait_for_result(id).is_none(), "a 404 should decode to nothing, not panic the render loop");
    }

    #[test]
    fn requesting_the_same_url_twice_for_the_same_node_is_a_no_op() {
        // Only meaningfully testable by absence of a crash/hang — `request`
        // is fire-and-forget, so a duplicate call spawning a second fetch
        // wouldn't be directly observable here either way, but it
        // shouldn't panic (e.g. on a poisoned lock from reentrant use).
        let id: NodeId = 200_003;
        let url = "https://picsum.photos/id/1/100/100.jpg".to_string();
        super::image_cache::request(id, url.clone());
        super::image_cache::request(id, url);
        assert!(wait_for_result(id).is_some());
    }
}
