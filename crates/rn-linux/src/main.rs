//! Spike 4b (DESKTOP_RUNTIME_TZ.md): real `react-reconciler` (bundled from
//! `js/`, see js/src/index.ts) drives the same View/Text → Yoga → Skia
//! pipeline spike 4 proved with hand-written JS — this is the "Fabric" layer,
//! minus Meta's Fabric C++ (see Desktop-Runtime/CLAUDE.md for why).

use std::path::PathBuf;

use js_host::Runtime;
use skia_desktop::GlWindowSurface;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{WindowAttributes, WindowId};

/// Built by `pnpm build` in `js/` (esbuild, IIFE — Hermes has no module
/// loader). Read from disk, not `include_str!`'d, so JS iteration doesn't
/// require recompiling Rust.
const BUNDLE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../js/dist/bundle.js");

struct App {
    gpu: Option<GlWindowSurface>,
    hermes: Runtime,
    snapshot_path: Option<PathBuf>,
}

impl ApplicationHandler for App {
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
                // react-reconciler schedules its commit through a microtask
                // (our `queueMicrotask` shim) — Hermes only runs those when
                // told to, there's no event loop doing it implicitly.
                self.hermes.drain_microtasks().expect("drain microtasks");
                let (width, height): (u32, u32) = gpu.window.inner_size().into();
                js_host::host::with_scene(|scene| {
                    scene.compute_layout(width as f32, height as f32);
                    scene.draw(gpu.canvas());
                });
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

fn main() {
    let hermes = Runtime::new().expect("failed to create Hermes runtime");
    js_host::host::install(&hermes).expect("failed to install host functions");
    let bundle = std::fs::read_to_string(BUNDLE_PATH)
        .unwrap_or_else(|e| panic!("failed to read {BUNDLE_PATH}: {e} (run `pnpm build` in js/)"));
    hermes.eval(&bundle).expect("bundle JS failed");

    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let attrs = WindowAttributes::default()
        .with_title("rn-linux — Hermes+Yoga+Skia spike")
        .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 640.0));
    let gpu = GlWindowSurface::new(&event_loop, attrs);
    gpu.window.request_redraw();

    let snapshot_path = std::env::var_os("RN_LINUX_SNAPSHOT").map(PathBuf::from);
    let mut app = App { gpu: Some(gpu), hermes, snapshot_path };
    event_loop.run_app(&mut app).expect("event loop run_app failed");
}
