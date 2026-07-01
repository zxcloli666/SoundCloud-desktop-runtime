//! Spike 4b (DESKTOP_RUNTIME_TZ.md): real `react-reconciler` (bundled from
//! `js/`, see js/src/index.ts) drives the same View/Text → Yoga → Skia
//! pipeline spike 4 proved with hand-written JS — this is the "Fabric" layer,
//! minus Meta's Fabric C++ (see Desktop-Runtime/CLAUDE.md for why).

use std::path::PathBuf;
use std::time::Instant;

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

const TICK_JS: &str = "if (typeof __reanimatedTick === 'function') __reanimatedTick();";

struct App {
    gpu: Option<GlWindowSurface>,
    hermes: Runtime,
    start: Instant,
    snapshot_path: Option<PathBuf>,
    snapshot_delay_ms: u64,
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
                // Reanimated worklets (spike 6) — our own per-frame tick,
                // not a second UI-runtime thread. See js/src/reanimated.tsx.
                self.hermes.eval(TICK_JS).expect("reanimated tick failed");

                let (width, height): (u32, u32) = gpu.window.inner_size().into();
                js_host::host::with_scene(|scene| {
                    scene.compute_layout(width as f32, height as f32);
                    scene.draw(gpu.canvas());
                });

                let elapsed = self.start.elapsed().as_millis() as u64;
                if self.snapshot_path.is_some() && elapsed >= self.snapshot_delay_ms {
                    let path = self.snapshot_path.take().unwrap();
                    std::fs::write(&path, gpu.snapshot_png()).expect("write snapshot");
                    println!("wrote snapshot to {}", path.display());
                    gpu.present();
                    event_loop.exit();
                } else {
                    gpu.present();
                    // Keep animating: reanimated timings/derived values need a
                    // steady stream of frames, not just resize/input events.
                    gpu.window.request_redraw();
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
    event_loop.set_control_flow(ControlFlow::Poll);

    let attrs = WindowAttributes::default()
        .with_title("rn-linux — Hermes+Yoga+Skia spike")
        .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 640.0));
    let gpu = GlWindowSurface::new(&event_loop, attrs);
    gpu.window.request_redraw();

    let snapshot_path = std::env::var_os("RN_LINUX_SNAPSHOT").map(PathBuf::from);
    let snapshot_delay_ms = std::env::var("RN_LINUX_SNAPSHOT_DELAY_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut app = App {
        gpu: Some(gpu),
        hermes,
        start: Instant::now(),
        snapshot_path,
        snapshot_delay_ms,
    };
    event_loop.run_app(&mut app).expect("event loop run_app failed");
}
