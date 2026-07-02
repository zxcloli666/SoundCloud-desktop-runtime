//! Tiling window managers hand out whatever aspect ratio fits their layout,
//! ignoring the app's requested size (found while chasing a screenshot that
//! looked cut off at the requested 1024x640 — the actual window was
//! 847x1388). This renders offscreen (no GPU window needed) at that same
//! unusual aspect ratio and checks the root's background genuinely covers
//! it, guarding against the root/Canvas silently falling back to a fixed size.
//!
//! `rt.eval(...)` below only ever runs our own locally built bundle JS,
//! never external input — Hermes' ordinary script-execution entry point,
//! not a code-injection risk.

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
