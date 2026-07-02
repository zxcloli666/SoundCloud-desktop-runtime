//! `draw_resized_image`'s `resizeMode: "repeat"` — pure Rust + real Skia
//! offscreen rendering, no Hermes/JS needed (a synthetic in-memory image,
//! not a real network fetch, keeps this deterministic and fast — real
//! fetch+decode is already covered by `image_cache_test`).

use crate::scene::{Scene, StyleInput};

fn style(json: &str) -> StyleInput {
    serde_json::from_str(json).expect("valid style JSON")
}

/// A 4x4 image, pixel (0,0) red, everything else blue — repeat-tiling a
/// 16x16 destination with this should paint red at every tile's own
/// (0,0) corner (0,0 / 4,0 / 8,0 / 12,0 / 0,4 / ...) and blue everywhere
/// else. Any other resize mode would either scale this beyond
/// recognition or paint a single untiled copy — this pattern is only
/// reproducible by real tiling.
fn checkerboard_corner_image() -> skia_safe::Image {
    let image_info = skia_safe::ImageInfo::new_n32_premul((4, 4), None);
    let mut surface = skia_safe::surfaces::raster(&image_info, None, None).expect("raster surface");
    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::BLUE);
    let mut red = skia_safe::Paint::default();
    red.set_color(skia_safe::Color::RED);
    canvas.draw_rect(skia_safe::Rect::from_xywh(0.0, 0.0, 1.0, 1.0), &red);
    surface.image_snapshot()
}

#[test]
fn repeat_tiles_the_image_at_its_natural_size() {
    let mut scene = Scene::new();
    let root = scene.create_view();
    scene.set_style(root, style(r#"{"width": 16, "height": 16, "imageUri": "test://checkerboard", "imageResizeMode": "repeat"}"#));
    scene.set_root(root);
    scene.set_image(root, Some(checkerboard_corner_image()));
    scene.compute_layout(16.0, 16.0);

    let image_info = skia_safe::ImageInfo::new_n32_premul((16, 16), None);
    let mut surface = skia_safe::surfaces::raster(&image_info, None, None).expect("raster surface");
    scene.draw(surface.canvas());

    let snapshot = surface.image_snapshot();
    let pixmap = snapshot.peek_pixels().expect("raster surface should be readable");

    for tile_x in [0, 4, 8, 12] {
        for tile_y in [0, 4, 8, 12] {
            let corner = pixmap.get_color((tile_x, tile_y));
            assert_eq!((corner.r(), corner.g(), corner.b()), (255, 0, 0), "tile at ({tile_x}, {tile_y})'s own corner should be red");
            let mid = pixmap.get_color((tile_x + 2, tile_y + 2));
            assert_eq!((mid.r(), mid.g(), mid.b()), (0, 0, 255), "tile at ({tile_x}, {tile_y})'s middle should be blue, not stretched red");
        }
    }
}
