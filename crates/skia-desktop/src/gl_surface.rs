use std::ffi::CString;
use std::num::NonZeroU32;

use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::prelude::{GlSurface, NotCurrentGlContext};
use glutin::surface::{Surface as GlutinSurface, SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use skia_safe::gpu::gl::FramebufferInfo;
use skia_safe::gpu::{self, SurfaceOrigin, backend_render_targets};
use skia_safe::{Canvas, ColorType, EncodedImageFormat, Surface};
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowAttributes};

/// GPU-backed Skia surface bound to a real OS window (OpenGL/EGL/GLX via glutin).
/// Field order matters: Rust drops top-to-bottom, and the GL context must outlive
/// the Skia surface, which must outlive the window (rust-skia#476).
pub struct GlWindowSurface {
    surface: Surface,
    gl_surface: GlutinSurface<WindowSurface>,
    gr_context: gpu::DirectContext,
    gl_context: PossiblyCurrentContext,
    pub window: Window,
    fb_info: FramebufferInfo,
    num_samples: usize,
    stencil_size: usize,
}

impl Drop for GlWindowSurface {
    fn drop(&mut self) {
        // Otherwise some GPU drivers (notably AMD) segfault on teardown.
        self.gr_context.release_resources_and_abandon();
    }
}

impl GlWindowSurface {
    pub fn new(event_loop: &EventLoop<()>, attributes: WindowAttributes) -> Self {
        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_transparency(true);

        let display_builder = DisplayBuilder::new().with_window_attributes(Some(attributes));
        let (window, gl_config) = display_builder
            .build(event_loop, template, |configs| {
                configs
                    .reduce(|accum, config| {
                        let transparency_check = config.supports_transparency().unwrap_or(false)
                            & !accum.supports_transparency().unwrap_or(false);
                        if transparency_check || config.num_samples() < accum.num_samples() {
                            config
                        } else {
                            accum
                        }
                    })
                    .unwrap()
            })
            .expect("could not build GL display/config");
        let window = window.expect("could not create window with GL config");
        let raw_window_handle = window
            .window_handle()
            .expect("window handle")
            .as_raw();

        let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(Some(raw_window_handle));
        let not_current_gl_context = unsafe {
            gl_config
                .display()
                .create_context(&gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    gl_config
                        .display()
                        .create_context(&gl_config, &fallback_context_attributes)
                        .expect("failed to create GL context (core and GLES both failed)")
                })
        };

        let (width, height): (u32, u32) = window.inner_size().into();
        let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw_window_handle,
            NonZeroU32::new(width.max(1)).unwrap(),
            NonZeroU32::new(height.max(1)).unwrap(),
        );
        let gl_surface = unsafe {
            gl_config
                .display()
                .create_window_surface(&gl_config, &surface_attributes)
                .expect("could not create GL window surface")
        };

        let gl_context = not_current_gl_context
            .make_current(&gl_surface)
            .expect("could not make GL context current");

        gl::load_with(|s| {
            gl_config
                .display()
                .get_proc_address(CString::new(s).unwrap().as_c_str())
        });
        let interface = gpu::gl::Interface::new_load_with(|name| {
            if name == "eglGetCurrentDisplay" {
                return std::ptr::null();
            }
            gl_config
                .display()
                .get_proc_address(CString::new(name).unwrap().as_c_str())
        })
        .expect("could not create Skia GL interface");

        let mut gr_context =
            gpu::direct_contexts::make_gl(interface, None).expect("could not create GrContext");

        let fb_info = {
            let mut fboid: gl::types::GLint = 0;
            unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };
            FramebufferInfo {
                fboid: fboid.try_into().unwrap(),
                format: gpu::gl::Format::RGBA8.into(),
                ..Default::default()
            }
        };

        let num_samples = gl_config.num_samples() as usize;
        let stencil_size = gl_config.stencil_size() as usize;
        let surface = Self::create_surface((width, height), fb_info, &mut gr_context, num_samples, stencil_size);

        Self {
            surface,
            gl_surface,
            gr_context,
            gl_context,
            window,
            fb_info,
            num_samples,
            stencil_size,
        }
    }

    /// Takes `(width, height)` explicitly rather than re-querying
    /// `window.inner_size()` — on Wayland compositors (found on Hyprland),
    /// the freshly delivered `WindowEvent::Resized` payload can be ahead of
    /// what `inner_size()` reports at that exact moment (surface resize is a
    /// two-step negotiate, not synchronous), so re-deriving it here produced
    /// a Skia surface still sized to the *previous* dimensions — GPU
    /// snapshots looked cut off at the old size even though the scene's
    /// Yoga layout (computed from the caller's trusted size) was correct.
    fn create_surface(
        (width, height): (u32, u32),
        fb_info: FramebufferInfo,
        gr_context: &mut gpu::DirectContext,
        num_samples: usize,
        stencil_size: usize,
    ) -> Surface {
        let size = (
            width.try_into().expect("width overflow"),
            height.try_into().expect("height overflow"),
        );
        let backend_render_target =
            backend_render_targets::make_gl(size, num_samples, stencil_size, fb_info);
        gpu::surfaces::wrap_backend_render_target(
            gr_context,
            &backend_render_target,
            SurfaceOrigin::BottomLeft,
            ColorType::RGBA8888,
            None,
            None,
        )
        .expect("could not wrap backend render target as Skia surface")
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.gl_surface.resize(
            &self.gl_context,
            NonZeroU32::new(width.max(1)).unwrap(),
            NonZeroU32::new(height.max(1)).unwrap(),
        );
        self.surface = Self::create_surface(
            (width, height),
            self.fb_info,
            &mut self.gr_context,
            self.num_samples,
            self.stencil_size,
        );
    }

    pub fn canvas(&mut self) -> &Canvas {
        self.surface.canvas()
    }

    pub fn present(&mut self) {
        self.gr_context.flush_and_submit();
        self.gl_surface
            .swap_buffers(&self.gl_context)
            .expect("swap_buffers failed");
    }

    /// Reads back the current frame as PNG bytes — for headless/CI verification,
    /// independent of whatever the window manager happens to show on screen.
    pub fn snapshot_png(&mut self) -> Vec<u8> {
        let image = self.surface.image_snapshot();
        let data = image
            .encode(Some(&mut self.gr_context), EncodedImageFormat::PNG, None)
            .expect("encode PNG snapshot");
        data.as_bytes().to_vec()
    }
}
