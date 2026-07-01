//! GPU-Skia surface for a native window. Shared between the Linux host (`rn-linux`)
//! and, eventually, the Windows backend — the point is one Skia GPU surface, not two.

mod gl_surface;

pub use gl_surface::GlWindowSurface;
pub use skia_safe as skia;
