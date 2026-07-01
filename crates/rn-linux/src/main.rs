//! Spike 4 (DESKTOP_RUNTIME_TZ.md): `<View><Text>` → Yoga layout → Skia draw,
//! driven by JS running in Hermes through js-host's host functions — no
//! react-reconciler yet (that's the next step once this pipeline is proven).

use std::path::PathBuf;

use js_host::Runtime;
use skia_desktop::GlWindowSurface;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{WindowAttributes, WindowId};

/// Hand-written JS build of a small View/Text tree, calling straight into the
/// host functions js_host::host installs. Stands in for react-reconciler's
/// output until spike 4b wires the real reconciler on top of these same calls.
const SCENE_JS: &str = r#"
const root = __scCreateView();
__scSetStyle(root, JSON.stringify({
    flexDirection: "column",
    padding: 24,
    backgroundColor: [0.04, 0.05, 0.08, 1.0],
}));

const card = __scCreateView();
__scSetStyle(card, JSON.stringify({
    flexDirection: "row",
    padding: 16,
    backgroundColor: [1.0, 1.0, 1.0, 0.10],
}));
__scAppendChild(root, card);

const orb = __scCreateView();
__scSetStyle(orb, JSON.stringify({
    width: 64, height: 64,
    backgroundColor: [0.35, 0.55, 1.0, 0.9],
}));
__scAppendChild(card, orb);

const label = __scCreateText("Hermes + Yoga + Skia, no Fabric C++");
__scSetStyle(label, JSON.stringify({ margin: 16 }));
__scAppendChild(card, label);

__scSetRoot(root);
"#;

struct App {
    gpu: Option<GlWindowSurface>,
    _hermes: Runtime,
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
    hermes.eval(SCENE_JS).expect("scene JS failed");

    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let attrs = WindowAttributes::default()
        .with_title("rn-linux — Hermes+Yoga+Skia spike")
        .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 640.0));
    let gpu = GlWindowSurface::new(&event_loop, attrs);
    gpu.window.request_redraw();

    let snapshot_path = std::env::var_os("RN_LINUX_SNAPSHOT").map(PathBuf::from);
    let mut app = App { gpu: Some(gpu), _hermes: hermes, snapshot_path };
    event_loop.run_app(&mut app).expect("event loop run_app failed");
}
