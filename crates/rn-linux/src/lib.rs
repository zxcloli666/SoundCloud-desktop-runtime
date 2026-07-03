//! The Linux desktop-RN runner: winit window + event loop, glued to
//! `js-host`'s Hermes/Yoga/Skia `Scene` (spike 4b, DESKTOP_RUNTIME_TZ.md) —
//! this is the "Fabric" layer, minus Meta's Fabric C++ (Desktop-Runtime/
//! CLAUDE.md explains why).
//!
//! Everything in this file is generic — it has no idea `sc-rn`/`@sc/ui`
//! exist. A consumer that wants SoundCloud-specific host ops (live auth/
//! home/wave/etc.) plugs them in from the outside via `RunConfig::
//! before_bundle_eval`, exactly like any third-party consumer of this
//! engine would register their own ops — see `examples/soundcloud/crates/
//! sc-desktop-example` for the real example.

use std::path::PathBuf;
use std::time::Instant;

use js_host::Runtime;
use skia_desktop::GlWindowSurface;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{WindowAttributes, WindowId};

const TICK_JS: &str = "if (typeof __reanimatedTick === 'function') __reanimatedTick();";
const DRAIN_TIMERS_JS: &str = "if (typeof __scDrainTimers === 'function') __scDrainTimers();";
const DISPATCH_LAYOUT_JS: &str = "if (typeof __scDispatchLayoutChanges === 'function') __scDispatchLayoutChanges();";

/// Everything a consumer needs to hand this engine to run their own bundle.
pub struct RunConfig {
    /// Built by `pnpm build` (esbuild, IIFE — Hermes has no module loader).
    /// Read from disk at `run()` time, not `include_str!`'d, so JS iteration
    /// doesn't require recompiling Rust.
    pub bundle_path: PathBuf,
    pub window_title: String,
    pub initial_size: (f64, f64),
    /// Runs once, right after the engine's own generic host ops are
    /// registered but before `bundle_path` is read/eval'd — the seam for a
    /// consumer to register additional `js_host::hermes_op` plugins (e.g.
    /// SoundCloud-specific ones) and run whatever one-time init they need
    /// against the same `Runtime` before any JS runs at all.
    pub before_bundle_eval: Option<Box<dyn FnOnce(&Runtime) -> Result<(), String>>>,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            bundle_path: PathBuf::new(),
            window_title: "rn-linux — Hermes+Yoga+Skia engine".to_string(),
            initial_size: (1024.0, 640.0),
            before_bundle_eval: None,
        }
    }
}

struct App {
    gpu: Option<GlWindowSurface>,
    hermes: Runtime,
    start: Instant,
    snapshot_path: Option<PathBuf>,
    snapshot_delay_ms: u64,
    /// Window-physical-pixel coordinates, same space `compute_layout` uses —
    /// updated on every `CursorMoved`, read back on `MouseInput`.
    cursor_pos: (f32, f32),
    /// The pressable node hit on the last `MouseInput` press-down, if any —
    /// `onPressOut`/`onPress` target this same node on release, not
    /// whatever's under the cursor at that later moment (matches real touch
    /// semantics: a press-and-drag-off still ends the SAME touch target).
    pressed_node: Option<u32>,
    focused: bool,
    occluded: bool,
}

/// `Occluded` isn't delivered on Wayland/Windows (winit's own docs) —
/// `focused` alone already covers alt-tab/minimize on every backend;
/// `occluded` only adds precision on X11 (visible-but-covered). Free
/// functions, not `App` methods: called from spots that already hold
/// `self.gpu.as_mut()`, which a `&self` method would conflict with.
fn should_render(focused: bool, occluded: bool) -> bool {
    focused && !occluded
}

/// While hidden, `gpu.present()` (a real GPU swap) can block forever —
/// compositors commonly stop delivering vsync to backgrounded windows,
/// and this is the one thread handling every OS event, so a blocked swap
/// freezes the whole app, not just rendering. Stop requesting redraws and
/// let the event loop sleep until something (most likely regaining focus)
/// wakes it back up.
fn sync_control_flow(event_loop: &ActiveEventLoop, gpu: &GlWindowSurface, focused: bool, occluded: bool) {
    if should_render(focused, occluded) {
        event_loop.set_control_flow(ControlFlow::Poll);
        gpu.window.request_redraw();
    } else {
        event_loop.set_control_flow(ControlFlow::Wait);
    }
}

impl App {
    /// `js/src/hostConfig.ts`'s `__scDispatchPress` — a plain `eval` (not
    /// `Function::call`+`create_value_from_json`, like the async bridge
    /// uses for arbitrary backend JSON) is fine here: every argument is a
    /// number or one of three fixed literal strings we control ourselves,
    /// nothing to escape.
    fn dispatch_press(&self, id: u32, phase: &str, local_x: f32, local_y: f32, page_x: f32, page_y: f32) {
        self.hermes
            .eval(&format!(
                "if (typeof __scDispatchPress === 'function') __scDispatchPress({id}, {phase:?}, {local_x}, {local_y}, {page_x}, {page_y});"
            ))
            .expect("dispatch press failed");
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                sync_control_flow(event_loop, gpu, self.focused, self.occluded);
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                sync_control_flow(event_loop, gpu, self.focused, self.occluded);
            }
            WindowEvent::Resized(size) => {
                gpu.resize(size.width, size.height);
                // useWindowDimensions (js/src/react-native.tsx) needs to know
                // without the app having to poll every frame.
                self.hermes
                    .eval(&format!(
                        "if (typeof __scNotifyResize === 'function') __scNotifyResize({}, {});",
                        size.width, size.height
                    ))
                    .expect("resize notify failed");
                gpu.window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let (cx, cy) = self.cursor_pos;
                match state {
                    ElementState::Pressed => {
                        let hit = js_host::host::with_scene(|scene| scene.hit_test(cx, cy));
                        if let Some((id, local_x, local_y)) = hit {
                            self.pressed_node = Some(id);
                            self.dispatch_press(id, "pressIn", local_x, local_y, cx, cy);
                        }
                    }
                    ElementState::Released => {
                        if let Some(id) = self.pressed_node.take() {
                            // Same node the press-down landed on, not
                            // whatever's under the cursor now — a
                            // press-and-drag-off still ends *that* touch.
                            // `absolute_origin` re-derives local coordinates
                            // for it at the *release* position, since
                            // `hit_test`'s original press-down result isn't
                            // kept around.
                            let still_over = js_host::host::with_scene(|scene| scene.hit_test(cx, cy)).is_some_and(|(hit_id, _, _)| hit_id == id);
                            let origin = js_host::host::with_scene(|scene| scene.absolute_origin(id));
                            let (local_x, local_y) = origin.map(|(ox, oy)| (cx - ox, cy - oy)).unwrap_or((cx, cy));
                            self.dispatch_press(id, "pressOut", local_x, local_y, cx, cy);
                            if still_over {
                                self.dispatch_press(id, "press", local_x, local_y, cx, cy);
                            }
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // `LineDelta` (most mice) is in "lines", not pixels — an
                // arbitrary but reasonable-feeling multiplier, same idea as
                // a browser's default wheel step. `PixelDelta` (trackpads)
                // is already precise.
                const LINE_HEIGHT_PX: f32 = 40.0;
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * LINE_HEIGHT_PX, y * LINE_HEIGHT_PX),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                let (cx, cy) = self.cursor_pos;
                let hit = js_host::host::with_scene(|scene| scene.hit_test_scrollable(cx, cy));
                if let Some(id) = hit {
                    js_host::host::with_scene(|scene| scene.scroll_by(id, dx, dy));
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                // react-reconciler schedules its commit through a microtask
                // (our `queueMicrotask` shim) — Hermes only runs those when
                // told to, there's no event loop doing it implicitly.
                //
                // Due timers first (our `setTimeout`/`setImmediate` shim,
                // js-host/src/host.rs's PRELUDE_JS — this is how the
                // `scheduler` package that react-reconciler depends on
                // re-enters *after* a commit, e.g. a `useEffect` calling
                // `setState`, without hitting React's "Should not already be
                // working" reentrancy guard), then whatever microtasks that
                // produced, mirroring a real event loop's task ordering.
                self.hermes.eval(DRAIN_TIMERS_JS).expect("drain timers failed");
                self.hermes.drain_microtasks().expect("drain microtasks");
                // Reanimated worklets (spike 6) — our own per-frame tick,
                // not a second UI-runtime thread. See js/src/reanimated.tsx.
                self.hermes.eval(TICK_JS).expect("reanimated tick failed");
                // Spike 7b: resolve/reject whatever async host-fn calls
                // finished on the background tokio runtime since last frame
                // — generic machinery, works whether or not a consumer even
                // registered any async ops. See js-host/src/async_bridge.rs.
                js_host::async_bridge::deliver(&self.hermes);

                let (width, height): (u32, u32) = gpu.window.inner_size().into();
                js_host::host::with_scene(|scene| {
                    scene.compute_layout(width as f32, height as f32);
                });
                // Separate `with_scene` call, not nested in the one above:
                // dispatching onLayout may run JS that re-enters the Scene
                // (e.g. `__scSetStyle` from a `setState` in an `onLayout`
                // handler), which would double-borrow the same thread-local
                // RefCell if done from inside `compute_layout`'s closure.
                // Whatever it changes takes effect next frame, same as any
                // other reactive update in this render loop.
                self.hermes.eval(DISPATCH_LAYOUT_JS).expect("dispatch layout changes failed");

                // `<Image>` fetch+decode (js-host/src/image_cache.rs) —
                // same per-frame drain shape as async_bridge::deliver, but
                // pure Rust: no JS involved, so no reentrancy concern
                // nesting it with the draw call below.
                for (id, image) in js_host::image_cache::drain_ready() {
                    js_host::host::with_scene(|scene| scene.set_image(id, image));
                }

                js_host::host::with_scene(|scene| {
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
                    // Guarded, not unconditional: a focus/occlusion change
                    // mid-frame shouldn't re-arm a redraw sync_control_flow
                    // just switched off.
                    if should_render(self.focused, self.occluded) {
                        gpu.window.request_redraw();
                    }
                }
            }
            _ => {}
        }
    }
}

/// Runs the engine against `config.bundle_path` until the window closes (or,
/// with `RN_LINUX_SNAPSHOT` set, until one frame is captured). Never
/// returns normally — same shape `main()` had before this was library-ified.
pub fn run(config: RunConfig) -> ! {
    let hermes = Runtime::new().expect("failed to create Hermes runtime");
    js_host::host::install(&hermes).expect("failed to install host functions");

    if let Some(before_bundle_eval) = config.before_bundle_eval {
        before_bundle_eval(&hermes).expect("before_bundle_eval hook failed");
    }

    let bundle = std::fs::read_to_string(&config.bundle_path).unwrap_or_else(|e| {
        panic!(
            "failed to read {}: {e} (run `pnpm build` in the JS package that produces it)",
            config.bundle_path.display()
        )
    });
    hermes.eval(&bundle).expect("bundle JS failed");

    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let (width, height) = config.initial_size;
    let attrs = WindowAttributes::default()
        .with_title(config.window_title)
        .with_inner_size(winit::dpi::LogicalSize::new(width, height));
    let gpu = GlWindowSurface::new(&event_loop, attrs);
    gpu.window.request_redraw();
    let (initial_width, initial_height): (u32, u32) = gpu.window.inner_size().into();
    hermes
        .eval(&format!("if (typeof __scNotifyResize === 'function') __scNotifyResize({initial_width}, {initial_height});"))
        .expect("resize notify failed");

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
        cursor_pos: (0.0, 0.0),
        pressed_node: None,
        focused: true,
        occluded: false,
    };
    event_loop.run_app(&mut app).expect("event loop run_app failed");
    std::process::exit(0);
}
