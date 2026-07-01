//! Spike 2 (DESKTOP_RUNTIME_TZ.md): proof that Skia GPU rendering works on Linux
//! through a real winit window. Draws a static scene echoing @sc/ui's glass/atmosphere
//! look (blurred rounded rect + glowing orbs) — no JS/Fabric yet, that's spike 3+.

use std::path::PathBuf;

use skia_desktop::GlWindowSurface;
use skia_desktop::skia::{BlurStyle, Canvas, Color4f, MaskFilter, Paint, RRect, Rect};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{WindowAttributes, WindowId};

struct App {
    gpu: Option<GlWindowSurface>,
    /// Set via `RN_LINUX_SNAPSHOT=<path>` — dumps the first rendered frame to
    /// PNG and exits, so rendering can be verified headlessly (CI, or when the
    /// window manager doesn't show the window where a screenshot tool looks).
    snapshot_path: Option<PathBuf>,
}

impl ApplicationHandler for App {
    // Window + GL context are created eagerly in `main()` before `run_app()`
    // (matches rust-skia's own gl-window example); nothing to do on resume.
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                gpu.resize(size.width, size.height);
                gpu.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                draw_scene(gpu.canvas());
                // Snapshot before swap: swap_buffers() may hand the drawn buffer
                // off to the compositor and leave an undefined one current.
                if let Some(path) = self.snapshot_path.take() {
                    std::fs::write(&path, gpu.snapshot_png()).expect("write snapshot");
                    println!("wrote snapshot to {}", path.display());
                    gpu.present();
                    event_loop.exit();
                } else {
                    gpu.present();
                }
            }
            _ => {}
        }
    }
}

fn draw_scene(canvas: &Canvas) {
    canvas.clear(Color4f::new(0.04, 0.05, 0.08, 1.0).to_color());

    let mut glow = Paint::new(Color4f::new(0.35, 0.55, 1.0, 0.55), None);
    glow.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, 60.0, false));
    canvas.draw_circle((260.0, 200.0), 140.0, &glow);

    let mut glow2 = Paint::new(Color4f::new(0.85, 0.4, 0.9, 0.45), None);
    glow2.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, 80.0, false));
    canvas.draw_circle((760.0, 420.0), 180.0, &glow2);

    let mut glass = Paint::new(Color4f::new(1.0, 1.0, 1.0, 0.10), None);
    glass.set_anti_alias(true);
    let rrect = RRect::new_rect_xy(Rect::from_xywh(160.0, 160.0, 700.0, 320.0), 28.0, 28.0);
    canvas.draw_rrect(rrect, &glass);

    let mut glass_edge = Paint::new(Color4f::new(1.0, 1.0, 1.0, 0.28), None);
    glass_edge.set_anti_alias(true);
    glass_edge.set_stroke(true);
    glass_edge.set_stroke_width(1.5);
    canvas.draw_rrect(rrect, &glass_edge);
}

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let attrs = WindowAttributes::default()
        .with_title("rn-linux — skia-desktop spike")
        .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 640.0));
    let gpu = GlWindowSurface::new(&event_loop, attrs);
    gpu.window.request_redraw();

    let snapshot_path = std::env::var_os("RN_LINUX_SNAPSHOT").map(PathBuf::from);
    let mut app = App {
        gpu: Some(gpu),
        snapshot_path,
    };
    event_loop.run_app(&mut app).expect("event loop run_app failed");
}
