//! Hermes embedded in our own Rust process via `rusty_hermes` (safe wrapper
//! over Hermes' JSI). Owns the mounted scene tree (`scene`) and the host
//! functions (`host`) a JS-side `react-reconciler` host-config calls into —
//! this is where Fabric's job (mounting + Yoga layout) happens for us.

pub mod host;
pub mod scene;

pub use rusty_hermes::Runtime;
pub use scene::Scene;

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

        super::host::with_scene(|scene| {
            let root = scene.root.expect("bundle should have set a scene root");
            scene.compute_layout(1024.0, 640.0);
            let (_, _, w, h) = scene.layout_of(root);
            assert!(w > 0.0 && h > 0.0, "root should have a non-empty layout");
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

        let (width, height) = (847, 1388);
        let image_info = skia_safe::ImageInfo::new_n32_premul((width, height), None);
        let mut surface = skia_safe::surfaces::raster(&image_info, None, None).expect("raster surface");
        super::host::with_scene(|scene| {
            scene.compute_layout(width as f32, height as f32);
            scene.draw(surface.canvas());
        });

        let image = surface.image_snapshot();
        let pixmap = image.peek_pixels().expect("raster surface should be readable");
        // Root's backgroundColor is [0.04, 0.05, 0.08, 1.0] — check well below
        // where the demo's hardcoded-pixel-position orbs/panel stop (~500px),
        // near the bottom of the actual window rather than a fixed old size.
        let color = pixmap.get_color((width / 2, height - 20));
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
