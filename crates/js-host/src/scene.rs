//! The mounted tree behind a `react-reconciler` host-config. Two kinds of
//! node share one tree:
//! - **layout nodes** (View/Text/Canvas) — real Yoga geometry, flexbox props.
//! - **Skia draw nodes** (Circle/RoundedRect/Group/Blur/gradients/Box/
//!   BoxShadow) — no Yoga; like real react-native-skia, they're positioned in
//!   raw pixel coordinates within their nearest ancestor Canvas, mirroring
//!   `@shopify/react-native-skia`'s JSX surface (see Desktop-Runtime/CLAUDE.md)
//!   without replicating its internal two-reconciler/SkPicture architecture —
//!   we own the whole pipeline, so one tree is enough.
//!
//! Style/color coverage is intentionally wide (percent dimensions, absolute
//! positioning, per-corner radii, CSS color strings) rather than the minimum
//! `@sc/ui` happens to use today — the point is not having to keep coming
//! back here every time a new screen touches one more RN style prop.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use ordered_float::OrderedFloat;
use serde::Deserialize;
use serde_json::Value as Json;
use skia_safe::{
    BlendMode, BlurStyle, Canvas, Color4f, Font, FontMgr, FontStyle, MaskFilter, Paint, PaintStyle,
    Point, RRect, Rect, Shader, TileMode, Typeface, Vector, canvas::SaveLayerRec, gradient, image_filters,
};
use yoga::{Align, Direction, Edge, FlexDirection, Justify, MeasureMode, Node as YogaNode, PositionType, Size as YogaSize, StyleUnit, Wrap};

thread_local! {
    // The system default typeface is somewhat expensive to resolve (font
    // manager lookup) — cache it once per thread, cheap to `Typeface::clone`
    // (Skia ref-counted handle) for a differently-sized `Font` each time.
    static TYPEFACE_CACHE: RefCell<Option<Typeface>> = const { RefCell::new(None) };
    // Set only for the duration of `Scene::compute_layout`'s `calculate_layout`
    // call — Yoga invokes `measure_text` synchronously and reentrantly from
    // there for any dirty text node, and that's the ONLY time it's non-null.
    // A raw pointer, not a normal reference, because `extern "C" fn` measure
    // callbacks can't capture Rust closure state — Yoga's C API only passes
    // back whatever opaque `NodeRef`/context we attached per-node (see
    // `SceneNode::text`/`Scene::alloc`), not an arbitrary payload.
    static CURRENT_SCENE: Cell<*const Scene> = const { Cell::new(std::ptr::null()) };
}

fn cached_typeface() -> Typeface {
    TYPEFACE_CACHE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(
                FontMgr::default()
                    .legacy_make_typeface(None, FontStyle::default())
                    .expect("no system default typeface available"),
            );
        }
        slot.as_ref().expect("just initialized").clone()
    })
}

fn sized_font(size: f32) -> Font {
    Font::from_typeface(cached_typeface(), size)
}

/// Longest prefix of `text` (by character count) that, with "…" appended,
/// measures within `max_width` — binary search since `Font::measure_str`
/// isn't linear-cost-free to call per character on longer strings.
fn truncate_with_ellipsis(text: &str, font: &Font, max_width: f32) -> String {
    let (full_width, _) = font.measure_str(text, None);
    if full_width <= max_width {
        return text.to_string();
    }
    let ellipsis = "\u{2026}";
    let (ellipsis_width, _) = font.measure_str(ellipsis, None);
    if ellipsis_width > max_width {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        let candidate: String = chars[..mid].iter().collect::<String>() + ellipsis;
        let (width, _) = font.measure_str(&candidate, None);
        if width <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo].iter().collect::<String>() + ellipsis
}

/// Yoga's measure-function hook (real single-line text measurement, replacing
/// a `chars().count() * 8.0` guess) — reports the text's natural, unwrapped
/// size regardless of the width/height constraint Yoga passes in, same as a
/// real non-wrapping single-line Text component. If the node's *final*
/// layout width ends up smaller (Yoga's flex-shrink honoring that natural
/// size as a hint, same as real Text), `draw_layout_node` truncates with an
/// ellipsis at draw time — this function only ever reports the untruncated
/// size.
extern "C" fn measure_text(node_ref: yoga::NodeRef, _width: f32, _width_mode: MeasureMode, _height: f32, _height_mode: MeasureMode) -> YogaSize {
    let empty = YogaSize { width: 0.0, height: 0.0 };
    let Some(id) = yoga::get_node_ref_context(&node_ref).and_then(|ctx| ctx.downcast_ref::<NodeId>()).copied() else {
        return empty;
    };
    let scene_ptr = CURRENT_SCENE.with(|c| c.get());
    if scene_ptr.is_null() {
        return empty;
    }
    // SAFETY: only ever non-null for the duration of the `calculate_layout`
    // call inside `Scene::compute_layout`, which holds `self: &Scene` (this
    // exact pointer) on the stack for that whole call — Yoga only invokes
    // measure functions synchronously from within it, never after it returns.
    let scene = unsafe { &*scene_ptr };
    scene.measure_text_node(id).unwrap_or(empty)
}

pub type NodeId = u32;

pub enum NodeKind {
    View,
    Text(String),
    Canvas,
    Circle,
    Rect,
    RoundedRect,
    SkPath,
    SkText,
    /// No asset-decoding pipeline yet — renders as a placeholder box the
    /// requested size, same honest-stub approach as `react-native`'s `Image`.
    SkImage,
    Group,
    Blur,
    RadialGradient,
    LinearGradient,
    /// Configures the parent shape's paint directly (color/opacity/blendMode)
    /// — same "child configures parent" pattern as Blur/gradients.
    Paint,
    Box,
    BoxShadow,
}

/// Radii for the four corners, clockwise from top-left — matches CSS/RN's
/// per-corner `border*Radius` props exactly.
#[derive(Clone, Copy)]
struct CornerRadii {
    top_left: f32,
    top_right: f32,
    bottom_right: f32,
    bottom_left: f32,
}

impl CornerRadii {
    fn uniform(r: f32) -> Self {
        Self { top_left: r, top_right: r, bottom_right: r, bottom_left: r }
    }

    fn is_uniform(&self) -> bool {
        self.top_left == self.top_right && self.top_right == self.bottom_right && self.bottom_right == self.bottom_left
    }

    fn rrect(&self, rect: Rect) -> RRect {
        if self.is_uniform() {
            return RRect::new_rect_xy(rect, self.top_left, self.top_left);
        }
        RRect::new_rect_radii(
            rect,
            &[
                Vector::new(self.top_left, self.top_left),
                Vector::new(self.top_right, self.top_right),
                Vector::new(self.bottom_right, self.bottom_right),
                Vector::new(self.bottom_left, self.bottom_left),
            ],
        )
    }
}

#[derive(Default)]
struct LayoutPaint {
    background: Option<[f32; 4]>,
    opacity: f32,
    overflow_hidden: bool,
    radii: CornerRadii,
    border_width: f32,
    border_color: Option<[f32; 4]>,
    shadow: Option<ViewShadow>,
    /// `react-native.tsx`'s `Text` always renders `<View style={{fontSize,
    /// color, ...}}>{string}</View>` — these live on the wrapping View (this
    /// node), not the `NodeKind::Text` child itself, so drawing that child
    /// looks them up via `SceneNode::parent`.
    font_size: f32,
    text_color: [f32; 4],
}

/// RN's iOS-style View shadow (`shadowColor`/`shadowOpacity`/`shadowRadius`/
/// `shadowOffset` — `@sc/ui`'s `Avatar` ring glow and `Button`'s primary-variant
/// glow both depend on this). `elevation` (Android) isn't handled separately:
/// `@sc/ui` always sets both together, and the shadow* props already carry
/// everything needed to draw one.
#[derive(Clone, Copy)]
struct ViewShadow {
    color: [f32; 4],
    radius: f32,
    offset: (f32, f32),
}

impl Default for CornerRadii {
    fn default() -> Self {
        Self::uniform(0.0)
    }
}

const DEFAULT_FONT_SIZE: f32 = 16.0;
const DEFAULT_TEXT_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

pub struct SceneNode {
    kind: NodeKind,
    /// `None` for Skia draw nodes — they don't participate in flexbox.
    yoga: Option<YogaNode>,
    paint: LayoutPaint,
    /// Raw props for Skia draw nodes, parsed against their kind at draw time
    /// (the prop shapes are too varied per type to justify one big struct).
    props: Json,
    children: Vec<NodeId>,
    /// Set by `append_child` — a `NodeKind::Text` child looks its own
    /// `fontSize`/`color` up through this (see `LayoutPaint::font_size`).
    parent: Option<NodeId>,
}

impl SceneNode {
    fn layout(kind: NodeKind) -> Self {
        Self {
            kind,
            yoga: Some(YogaNode::new()),
            paint: LayoutPaint { opacity: 1.0, font_size: DEFAULT_FONT_SIZE, text_color: DEFAULT_TEXT_COLOR, ..Default::default() },
            props: Json::Null,
            children: Vec::new(),
            parent: None,
        }
    }

    fn sk(kind: NodeKind) -> Self {
        Self {
            kind,
            yoga: None,
            paint: LayoutPaint { font_size: DEFAULT_FONT_SIZE, text_color: DEFAULT_TEXT_COLOR, ..Default::default() },
            props: Json::Null,
            children: Vec::new(),
            parent: None,
        }
    }

    fn text(text: String) -> Self {
        let mut yoga = YogaNode::new();
        // No explicit width/height: `measure_text` (Yoga's measure-function
        // hook, wired up in `Scene::alloc` once this node's id is known)
        // reports real measured size instead. `flex_shrink` lets a text node
        // actually shrink below that natural size when its flex container
        // doesn't have room — `draw_layout_node` compares final vs. natural
        // width to decide whether to truncate with an ellipsis.
        yoga.set_flex_shrink(1.0);
        Self {
            kind: NodeKind::Text(text),
            yoga: Some(yoga),
            paint: LayoutPaint { opacity: 1.0, font_size: DEFAULT_FONT_SIZE, text_color: DEFAULT_TEXT_COLOR, ..Default::default() },
            props: Json::Null,
            children: Vec::new(),
            parent: None,
        }
    }
}

fn pt(v: f32) -> StyleUnit {
    StyleUnit::Point(OrderedFloat(v))
}

/// A style dimension: points (`120`) or a percentage string (`"50%"`) — RN
/// accepts both everywhere width/height/position/margin/padding are used.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Dimension {
    Point(f32),
    Percent(String),
}

impl Dimension {
    fn to_style_unit(&self) -> StyleUnit {
        match self {
            Dimension::Point(p) => pt(*p),
            Dimension::Percent(s) => {
                let pct = s.trim_end_matches('%').trim().parse::<f32>().unwrap_or(0.0);
                StyleUnit::Percent(OrderedFloat(pct))
            }
        }
    }
}

/// Wide coverage of RN's `StyleSheet` surface — percent dimensions, absolute
/// positioning, gaps, per-corner radii, border, opacity, overflow — not just
/// what `@sc/ui` happens to use today.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleInput {
    pub width: Option<Dimension>,
    pub height: Option<Dimension>,
    pub min_width: Option<Dimension>,
    pub max_width: Option<Dimension>,
    pub min_height: Option<Dimension>,
    pub max_height: Option<Dimension>,

    pub flex: Option<f32>,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<Dimension>,
    pub flex_direction: Option<String>,
    pub flex_wrap: Option<String>,
    pub justify_content: Option<String>,
    pub align_items: Option<String>,
    pub align_self: Option<String>,
    pub align_content: Option<String>,
    pub aspect_ratio: Option<f32>,
    pub display: Option<String>,

    pub position: Option<String>,
    pub left: Option<Dimension>,
    pub right: Option<Dimension>,
    pub top: Option<Dimension>,
    pub bottom: Option<Dimension>,

    pub padding: Option<Dimension>,
    pub padding_horizontal: Option<Dimension>,
    pub padding_vertical: Option<Dimension>,
    pub padding_left: Option<Dimension>,
    pub padding_right: Option<Dimension>,
    pub padding_top: Option<Dimension>,
    pub padding_bottom: Option<Dimension>,

    pub margin: Option<Dimension>,
    pub margin_horizontal: Option<Dimension>,
    pub margin_vertical: Option<Dimension>,
    pub margin_left: Option<Dimension>,
    pub margin_right: Option<Dimension>,
    pub margin_top: Option<Dimension>,
    pub margin_bottom: Option<Dimension>,

    pub gap: Option<f32>,
    pub row_gap: Option<f32>,
    pub column_gap: Option<f32>,

    pub opacity: Option<f32>,
    pub overflow: Option<String>,
    pub background_color: Option<Json>,

    pub border_radius: Option<f32>,
    pub border_top_left_radius: Option<f32>,
    pub border_top_right_radius: Option<f32>,
    pub border_bottom_left_radius: Option<f32>,
    pub border_bottom_right_radius: Option<f32>,
    pub border_width: Option<f32>,
    pub border_color: Option<Json>,

    pub shadow_color: Option<Json>,
    pub shadow_opacity: Option<f32>,
    pub shadow_radius: Option<f32>,
    pub shadow_offset: Option<ShadowOffset>,

    /// Lives on the wrapping View (`react-native.tsx`'s `Text` component),
    /// not the `NodeKind::Text` child — see `LayoutPaint::font_size`.
    pub color: Option<Json>,
    pub font_size: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct ShadowOffset {
    pub width: f32,
    pub height: f32,
}

#[derive(Default)]
pub struct Scene {
    nodes: HashMap<NodeId, RefCell<SceneNode>>,
    next_id: NodeId,
    pub root: Option<NodeId>,
    /// Nodes JS registered an `onLayout` listener for, with the last
    /// geometry reported — `NAN` sentinel forces the first report.
    watched_layouts: HashMap<NodeId, (f32, f32, f32, f32)>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    fn alloc(&mut self, mut node: SceneNode) -> NodeId {
        self.next_id += 1;
        let id = self.next_id;
        // Only known once allocated (the node itself doesn't know its own id
        // yet when `SceneNode::text()` constructs it) — `measure_text` reads
        // this back via `yoga::get_node_ref_context` to know which node it's
        // being asked to measure.
        if let (NodeKind::Text(_), Some(yoga)) = (&node.kind, node.yoga.as_mut()) {
            yoga.set_context(Some(yoga::Context::new(id)));
            yoga.set_measure_func(Some(measure_text));
        }
        self.nodes.insert(id, RefCell::new(node));
        id
    }

    pub fn create_view(&mut self) -> NodeId {
        self.alloc(SceneNode::layout(NodeKind::View))
    }

    pub fn create_text(&mut self, text: String) -> NodeId {
        self.alloc(SceneNode::text(text))
    }

    /// Content-only update for an existing text node (react-reconciler's
    /// `commitTextUpdate`, called whenever a `<Text>` child string changes
    /// between renders — every live-data-driven label needs this, not just
    /// static copy). `mark_dirty` invalidates Yoga's cached measurement so
    /// `measure_text` actually re-runs for the new string on the next layout
    /// (Yoga otherwise assumes an unchanged node's size is still valid).
    pub fn set_text(&mut self, id: NodeId, text: String) {
        let cell = self.nodes.get(&id).expect("unknown text node id");
        let mut node = cell.borrow_mut();
        node.kind = NodeKind::Text(text);
        if let Some(yoga) = node.yoga.as_mut() {
            yoga.mark_dirty();
        }
    }

    /// `kind_name` matches the host-config `type` string from `js/src/rnskia`
    /// (e.g. "Canvas", "Circle", "Group", "RadialGradient", "BoxShadow"...).
    pub fn create_sk_node(&mut self, kind_name: &str) -> NodeId {
        let kind = match kind_name {
            "Canvas" => return self.alloc(SceneNode::layout(NodeKind::Canvas)),
            "Circle" => NodeKind::Circle,
            "Rect" => NodeKind::Rect,
            "RoundedRect" => NodeKind::RoundedRect,
            "Path" => NodeKind::SkPath,
            "Text" => NodeKind::SkText,
            "Image" => NodeKind::SkImage,
            "Group" | "BackdropBlur" | "BackdropFilter" | "Mask" => NodeKind::Group,
            "Blur" | "ColorMatrix" | "Shader" => NodeKind::Blur,
            "RadialGradient" => NodeKind::RadialGradient,
            "LinearGradient" => NodeKind::LinearGradient,
            "Paint" => NodeKind::Paint,
            "Box" => NodeKind::Box,
            "BoxShadow" => NodeKind::BoxShadow,
            other => panic!("unknown Skia node kind: {other}"),
        };
        self.alloc(SceneNode::sk(kind))
    }

    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        let parent_cell = self.nodes.get(&parent).expect("unknown parent id");
        let child_cell = self.nodes.get(&child).expect("unknown child id");
        // Two RefCells, not one — borrow_mut on each is independently checked,
        // so this doesn't panic even though both live in the same HashMap.
        let mut parent_node = parent_cell.borrow_mut();
        let mut child_node = child_cell.borrow_mut();
        let index = parent_node.children.len();
        if let (Some(py), Some(cy)) = (parent_node.yoga.as_mut(), child_node.yoga.as_mut()) {
            py.insert_child(cy, index);
        }
        parent_node.children.push(child);
        child_node.parent = Some(parent);
    }

    pub fn remove_child(&mut self, parent: NodeId, child: NodeId) {
        let parent_cell = self.nodes.get(&parent).expect("unknown parent id");
        let child_cell = self.nodes.get(&child).expect("unknown child id");
        let mut parent_node = parent_cell.borrow_mut();
        let mut child_node = child_cell.borrow_mut();
        if let (Some(py), Some(cy)) = (parent_node.yoga.as_mut(), child_node.yoga.as_mut()) {
            py.remove_child(cy);
        }
        parent_node.children.retain(|id| *id != child);
        self.watched_layouts.remove(&child);
    }

    /// Registers `id` for `onLayout` reporting — `drain_layout_changes()`
    /// (called once per frame from rn-linux) reports it the first time and
    /// again whenever its Yoga-computed geometry actually changes.
    pub fn watch_layout(&mut self, id: NodeId) {
        self.watched_layouts.entry(id).or_insert((f32::NAN, f32::NAN, f32::NAN, f32::NAN));
    }

    pub fn unwatch_layout(&mut self, id: NodeId) {
        self.watched_layouts.remove(&id);
    }

    /// `(id, left, top, width, height)` for every watched node whose layout
    /// changed since the last call — relative to its parent, matching real
    /// RN's `onLayout` (`x`/`y` aren't meant for absolute positioning there
    /// either).
    pub fn drain_layout_changes(&mut self) -> Vec<(NodeId, f32, f32, f32, f32)> {
        let mut changes = Vec::new();
        for (&id, last) in self.watched_layouts.iter_mut() {
            let Some(node_cell) = self.nodes.get(&id) else { continue };
            let node = node_cell.borrow();
            let Some(yoga) = node.yoga.as_ref() else { continue };
            let current = (yoga.get_layout_left(), yoga.get_layout_top(), yoga.get_layout_width(), yoga.get_layout_height());
            if current != *last {
                *last = current;
                changes.push((id, current.0, current.1, current.2, current.3));
            }
        }
        changes
    }

    pub fn set_style(&mut self, id: NodeId, style: StyleInput) {
        let cell = self.nodes.get(&id).expect("unknown node id");
        let mut node = cell.borrow_mut();

        let uniform_radius = style.border_radius.unwrap_or(0.0);
        node.paint.radii = CornerRadii {
            top_left: style.border_top_left_radius.unwrap_or(uniform_radius),
            top_right: style.border_top_right_radius.unwrap_or(uniform_radius),
            bottom_right: style.border_bottom_right_radius.unwrap_or(uniform_radius),
            bottom_left: style.border_bottom_left_radius.unwrap_or(uniform_radius),
        };
        if let Some(opacity) = style.opacity {
            node.paint.opacity = opacity;
        }
        if let Some(overflow) = style.overflow.as_deref() {
            node.paint.overflow_hidden = overflow == "hidden";
        }
        if let Some(bg) = &style.background_color {
            node.paint.background = parse_color(bg);
        }
        if let Some(bw) = style.border_width {
            node.paint.border_width = bw;
        }
        if let Some(bc) = &style.border_color {
            node.paint.border_color = parse_color(bc);
        }
        if style.shadow_color.is_some() || style.shadow_opacity.is_some() || style.shadow_radius.is_some() || style.shadow_offset.is_some() {
            let mut color = style.shadow_color.as_ref().and_then(parse_color).unwrap_or([0.0, 0.0, 0.0, 1.0]);
            // RN multiplies shadowColor's own alpha by shadowOpacity, rather
            // than one replacing the other.
            color[3] *= style.shadow_opacity.unwrap_or(1.0);
            let radius = style.shadow_radius.unwrap_or(0.0);
            let offset = style.shadow_offset.as_ref().map(|o| (o.width, o.height)).unwrap_or((0.0, 0.0));
            node.paint.shadow = Some(ViewShadow { color, radius, offset });
        }
        if let Some(color) = &style.color {
            if let Some(c) = parse_color(color) {
                node.paint.text_color = c;
            }
        }
        if let Some(size) = style.font_size {
            node.paint.font_size = size;
        }

        let Some(yoga) = node.yoga.as_mut() else {
            return;
        };

        if let Some(w) = &style.width {
            yoga.set_width(w.to_style_unit());
        }
        if let Some(h) = &style.height {
            yoga.set_height(h.to_style_unit());
        }
        if let Some(v) = &style.min_width {
            yoga.set_min_width(v.to_style_unit());
        }
        if let Some(v) = &style.max_width {
            yoga.set_max_width(v.to_style_unit());
        }
        if let Some(v) = &style.min_height {
            yoga.set_min_height(v.to_style_unit());
        }
        if let Some(v) = &style.max_height {
            yoga.set_max_height(v.to_style_unit());
        }
        if let Some(flex) = style.flex {
            yoga.set_flex(flex);
        }
        if let Some(fg) = style.flex_grow {
            yoga.set_flex_grow(fg);
        }
        if let Some(fs) = style.flex_shrink {
            yoga.set_flex_shrink(fs);
        }
        if let Some(fb) = &style.flex_basis {
            yoga.set_flex_basis(fb.to_style_unit());
        }
        if let Some(dir) = style.flex_direction.as_deref() {
            let dir = match dir {
                "column" => FlexDirection::Column,
                "row-reverse" => FlexDirection::RowReverse,
                "column-reverse" => FlexDirection::ColumnReverse,
                _ => FlexDirection::Row,
            };
            yoga.set_flex_direction(dir);
        }
        if let Some(wrap) = style.flex_wrap.as_deref() {
            let wrap = match wrap {
                "wrap" => Wrap::Wrap,
                "wrap-reverse" => Wrap::WrapReverse,
                _ => Wrap::NoWrap,
            };
            yoga.set_flex_wrap(wrap);
        }
        if let Some(justify) = style.justify_content.as_deref() {
            let justify = match justify {
                "center" => Justify::Center,
                "flex-end" => Justify::FlexEnd,
                "space-between" => Justify::SpaceBetween,
                "space-around" => Justify::SpaceAround,
                "space-evenly" => Justify::SpaceEvenly,
                _ => Justify::FlexStart,
            };
            yoga.set_justify_content(justify);
        }
        if let Some(align) = style.align_items.as_deref() {
            yoga.set_align_items(parse_align(align));
        }
        if let Some(align) = style.align_self.as_deref() {
            yoga.set_align_self(parse_align(align));
        }
        if let Some(align) = style.align_content.as_deref() {
            yoga.set_align_content(parse_align(align));
        }
        if let Some(ratio) = style.aspect_ratio {
            yoga.set_aspect_ratio(ratio);
        }
        if let Some(display) = style.display.as_deref() {
            yoga.set_display(if display == "none" { yoga::Display::None } else { yoga::Display::Flex });
        }
        if let Some(position) = style.position.as_deref() {
            yoga.set_position_type(if position == "absolute" { PositionType::Absolute } else { PositionType::Relative });
        }
        if let Some(v) = &style.left {
            yoga.set_position(Edge::Left, v.to_style_unit());
        }
        if let Some(v) = &style.right {
            yoga.set_position(Edge::Right, v.to_style_unit());
        }
        if let Some(v) = &style.top {
            yoga.set_position(Edge::Top, v.to_style_unit());
        }
        if let Some(v) = &style.bottom {
            yoga.set_position(Edge::Bottom, v.to_style_unit());
        }

        if let Some(p) = &style.padding {
            for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
                yoga.set_padding(edge, p.to_style_unit());
            }
        }
        if let Some(p) = &style.padding_horizontal {
            for edge in [Edge::Left, Edge::Right] {
                yoga.set_padding(edge, p.to_style_unit());
            }
        }
        if let Some(p) = &style.padding_vertical {
            for edge in [Edge::Top, Edge::Bottom] {
                yoga.set_padding(edge, p.to_style_unit());
            }
        }
        if let Some(p) = &style.padding_left {
            yoga.set_padding(Edge::Left, p.to_style_unit());
        }
        if let Some(p) = &style.padding_right {
            yoga.set_padding(Edge::Right, p.to_style_unit());
        }
        if let Some(p) = &style.padding_top {
            yoga.set_padding(Edge::Top, p.to_style_unit());
        }
        if let Some(p) = &style.padding_bottom {
            yoga.set_padding(Edge::Bottom, p.to_style_unit());
        }

        if let Some(m) = &style.margin {
            for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
                yoga.set_margin(edge, m.to_style_unit());
            }
        }
        if let Some(m) = &style.margin_horizontal {
            for edge in [Edge::Left, Edge::Right] {
                yoga.set_margin(edge, m.to_style_unit());
            }
        }
        if let Some(m) = &style.margin_vertical {
            for edge in [Edge::Top, Edge::Bottom] {
                yoga.set_margin(edge, m.to_style_unit());
            }
        }
        if let Some(m) = &style.margin_left {
            yoga.set_margin(Edge::Left, m.to_style_unit());
        }
        if let Some(m) = &style.margin_right {
            yoga.set_margin(Edge::Right, m.to_style_unit());
        }
        if let Some(m) = &style.margin_top {
            yoga.set_margin(Edge::Top, m.to_style_unit());
        }
        if let Some(m) = &style.margin_bottom {
            yoga.set_margin(Edge::Bottom, m.to_style_unit());
        }

        if let Some(g) = style.gap {
            yoga.set_gap(yoga::Gutter::All, pt(g));
        }
        if let Some(g) = style.row_gap {
            yoga.set_gap(yoga::Gutter::Row, pt(g));
        }
        if let Some(g) = style.column_gap {
            yoga.set_gap(yoga::Gutter::Column, pt(g));
        }
    }

    /// Raw prop bag for Skia draw nodes (Circle/RoundedRect/Group/...) — kept
    /// as JSON and interpreted per-kind in `draw_sk_node`, since the shapes
    /// vary too much (center+radius vs xywh vs gradient stops) for one struct.
    pub fn set_sk_props(&mut self, id: NodeId, props: Json) {
        self.nodes.get(&id).expect("unknown node id").borrow_mut().props = props;
    }

    pub fn set_root(&mut self, id: NodeId) {
        self.root = Some(id);
    }

    /// `&self`, not `&mut self` — deliberately, even though nothing else
    /// needs it shared: `measure_text` needs a way to read node data back
    /// out of this exact `Scene` while Yoga is calling it reentrantly from
    /// inside `calculate_layout` below, and the only way to give it one
    /// without an actual second, conflicting borrow of the same thread-local
    /// `RefCell<Scene>` (`js-host/src/host.rs`'s `with_scene`) is a raw
    /// pointer to this call's own `&self`, valid for exactly its duration.
    pub fn compute_layout(&self, width: f32, height: f32) {
        let Some(root) = self.root else { return };
        CURRENT_SCENE.with(|c| c.set(self as *const Scene));
        {
            let cell = self.nodes.get(&root).expect("unknown root id");
            let mut node = cell.borrow_mut();
            let yoga = node.yoga.as_mut().expect("root must be a layout node");
            yoga.calculate_layout(width, height, Direction::LTR);
        }
        CURRENT_SCENE.with(|c| c.set(std::ptr::null()));
    }

    /// `measure_text`'s actual body — split out so it's an ordinary method
    /// (borrow-checked normally) rather than living inside the `unsafe`
    /// pointer-dereferencing `extern "C" fn` itself.
    fn measure_text_node(&self, id: NodeId) -> Option<YogaSize> {
        let node = self.nodes.get(&id)?.borrow();
        let NodeKind::Text(text) = &node.kind else { return None };
        let font_size = node
            .parent
            .and_then(|p| self.nodes.get(&p))
            .map(|p| p.borrow().paint.font_size)
            .unwrap_or(DEFAULT_FONT_SIZE);
        let font = sized_font(font_size);
        let (width, _bounds) = font.measure_str(text, None);
        let (_, metrics) = font.metrics();
        let height = metrics.descent - metrics.ascent + metrics.leading;
        Some(YogaSize { width, height })
    }

    pub fn children_of(&self, id: NodeId) -> Vec<NodeId> {
        self.nodes.get(&id).expect("unknown node id").borrow().children.clone()
    }

    /// `(left, top, width, height)` relative to this node's parent — for
    /// tests/introspection; drawing walks the tree itself via `draw()`.
    pub fn layout_of(&self, id: NodeId) -> (f32, f32, f32, f32) {
        let node = self.nodes.get(&id).expect("unknown node id").borrow();
        let yoga = node.yoga.as_ref().expect("layout_of on a non-layout node");
        (yoga.get_layout_left(), yoga.get_layout_top(), yoga.get_layout_width(), yoga.get_layout_height())
    }

    pub fn draw(&self, canvas: &Canvas) {
        let Some(root) = self.root else { return };
        self.draw_layout_node(root, 0.0, 0.0, canvas);
    }

    fn draw_layout_node(&self, id: NodeId, parent_x: f32, parent_y: f32, canvas: &Canvas) {
        let (x, y, w, h, text, is_canvas, children, background, opacity, overflow_hidden, radii, border_width, border_color, shadow, parent) = {
            let node = self.nodes.get(&id).expect("unknown node id").borrow();
            let yoga = node.yoga.as_ref().expect("draw_layout_node on a non-layout node");
            let x = parent_x + yoga.get_layout_left();
            let y = parent_y + yoga.get_layout_top();
            let w = yoga.get_layout_width();
            let h = yoga.get_layout_height();
            let text = match &node.kind {
                NodeKind::Text(t) => Some(t.clone()),
                _ => None,
            };
            let is_canvas = matches!(node.kind, NodeKind::Canvas);
            (
                x,
                y,
                w,
                h,
                text,
                is_canvas,
                node.children.clone(),
                node.paint.background,
                node.paint.opacity,
                node.paint.overflow_hidden,
                node.paint.radii,
                node.paint.border_width,
                node.paint.border_color,
                node.paint.shadow,
                node.parent,
            )
        };

        let rect = Rect::from_xywh(x, y, w, h);
        let needs_layer = opacity < 1.0;
        if needs_layer {
            let mut layer_paint = Paint::default();
            layer_paint.set_alpha_f(opacity.clamp(0.0, 1.0));
            canvas.save_layer(&SaveLayerRec::default().paint(&layer_paint));
        } else {
            canvas.save();
        }

        // Drawn before the fill, same as Skia `BoxShadow` (`draw_box_shadow`):
        // `drop_shadow`'s filtered draw includes a copy of the source shape
        // itself, which the real fill right below then exactly covers,
        // leaving only the blurred halo visible past the shape's edges.
        if let Some(shadow) = shadow {
            let sk_color = Color4f::new(shadow.color[0], shadow.color[1], shadow.color[2], shadow.color[3]).to_color();
            if let Some(filter) = image_filters::drop_shadow(shadow.offset, (shadow.radius, shadow.radius), sk_color, None, None, None) {
                let mut paint = Paint::new(Color4f::new(shadow.color[0], shadow.color[1], shadow.color[2], shadow.color[3]), None);
                paint.set_anti_alias(true);
                paint.set_image_filter(filter);
                canvas.draw_rrect(radii.rrect(rect), &paint);
            }
        }

        if let Some([r, g, b, a]) = background {
            let mut paint = Paint::new(Color4f::new(r, g, b, a), None);
            paint.set_anti_alias(true);
            canvas.draw_rrect(radii.rrect(rect), &paint);
        }

        if border_width > 0.0 {
            if let Some([r, g, b, a]) = border_color {
                let mut paint = Paint::new(Color4f::new(r, g, b, a), None);
                paint.set_anti_alias(true);
                paint.set_style(PaintStyle::Stroke);
                paint.set_stroke_width(border_width);
                canvas.draw_rrect(radii.rrect(rect), &paint);
            }
        }

        if overflow_hidden {
            canvas.clip_rrect(radii.rrect(rect), None, Some(true));
        }

        if let Some(text) = text {
            // `fontSize`/`color` live on the wrapping View (this node's
            // parent — see `LayoutPaint::font_size`), not this Text node.
            let (font_size, color) = parent
                .and_then(|p| self.nodes.get(&p))
                .map(|p| {
                    let p = p.borrow();
                    (p.paint.font_size, p.paint.text_color)
                })
                .unwrap_or((DEFAULT_FONT_SIZE, DEFAULT_TEXT_COLOR));
            let font = sized_font(font_size);
            // No line-wrapping support — a single line that doesn't fit its
            // final (possibly flex-shrunk) width truncates with an ellipsis,
            // the only sensible rendering for a non-wrapping Text.
            let displayed = truncate_with_ellipsis(&text, &font, w);
            let (_, metrics) = font.metrics();
            let baseline_y = y + (h - (metrics.descent - metrics.ascent)) * 0.5 - metrics.ascent;
            let mut paint = Paint::new(Color4f::new(color[0], color[1], color[2], color[3]), None);
            paint.set_anti_alias(true);
            canvas.draw_str(&displayed, (x, baseline_y), &font, &paint);
        }

        if is_canvas {
            canvas.save();
            canvas.clip_rect(rect, None, Some(true));
            for child in children {
                self.draw_sk_node(child, x, y, canvas);
            }
            canvas.restore();
        } else {
            for child in children {
                self.draw_layout_node(child, x, y, canvas);
            }
        }

        canvas.restore();
    }

    /// Skia draw nodes: no Yoga, raw pixel coordinates from `props`, offset by
    /// `(ox, oy)` — their Canvas's layout origin, or a parent Group's origin
    /// once we support nested translation.
    fn draw_sk_node(&self, id: NodeId, ox: f32, oy: f32, canvas: &Canvas) {
        let node = self.nodes.get(&id).expect("unknown node id").borrow();
        match node.kind {
            NodeKind::Circle => self.draw_circle(&node, ox, oy, canvas),
            NodeKind::Rect => self.draw_rect_shape(&node, ox, oy, canvas),
            NodeKind::RoundedRect => self.draw_rounded_rect(&node, ox, oy, canvas),
            NodeKind::SkPath => self.draw_sk_path(&node, ox, oy, canvas),
            NodeKind::SkText => self.draw_sk_text(&node, ox, oy, canvas),
            NodeKind::SkImage => self.draw_sk_image_placeholder(&node, ox, oy, canvas),
            NodeKind::Group => self.draw_group(&node, ox, oy, canvas),
            NodeKind::Box => self.draw_box(&node, ox, oy, canvas),
            // Configuration-only nodes: meaningful as a child of Circle/RRect/
            // Box/Group, not as something independently drawn.
            NodeKind::Blur | NodeKind::RadialGradient | NodeKind::LinearGradient | NodeKind::Paint | NodeKind::BoxShadow => {}
            NodeKind::View | NodeKind::Text(_) | NodeKind::Canvas => {
                unreachable!("layout node encountered in the Skia subtree")
            }
        }
    }

    fn shape_paint(&self, node: &SceneNode, cx: f32, cy: f32, radius: f32) -> Paint {
        let color = node.props.get("color").and_then(parse_color).unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let mut paint = Paint::new(Color4f::new(color[0], color[1], color[2], color[3]), None);
        paint.set_anti_alias(true);
        match node.props.get("style").and_then(Json::as_str) {
            Some("stroke") => {
                paint.set_style(PaintStyle::Stroke);
                let width = node.props.get("strokeWidth").and_then(Json::as_f64).unwrap_or(1.0);
                paint.set_stroke_width(width as f32);
            }
            _ => {
                paint.set_style(PaintStyle::Fill);
            }
        };
        if let Some(opacity) = node.props.get("opacity").and_then(Json::as_f64) {
            paint.set_alpha_f(opacity as f32);
        }

        for &child_id in &node.children {
            let child = self.nodes.get(&child_id).expect("unknown child id").borrow();
            match child.kind {
                NodeKind::RadialGradient => {
                    if let Some(shader) = radial_gradient_shader(&child.props, cx, cy, radius) {
                        paint.set_shader(shader);
                    }
                }
                NodeKind::LinearGradient => {
                    if let Some(shader) = linear_gradient_shader(&child.props) {
                        paint.set_shader(shader);
                    }
                }
                NodeKind::Blur => {
                    let sigma = child.props.get("blur").and_then(Json::as_f64).unwrap_or(0.0) as f32;
                    if sigma > 0.0 {
                        paint.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, sigma, false));
                    }
                }
                NodeKind::Paint => {
                    if let Some(color) = child.props.get("color").and_then(parse_color) {
                        paint.set_color4f(Color4f::new(color[0], color[1], color[2], color[3]), None);
                    }
                    if let Some(opacity) = child.props.get("opacity").and_then(Json::as_f64) {
                        paint.set_alpha_f(opacity as f32);
                    }
                    if let Some(mode) = child.props.get("blendMode").and_then(Json::as_str).and_then(json_blend_mode) {
                        paint.set_blend_mode(mode);
                    }
                }
                _ => {}
            }
        }
        paint
    }

    fn draw_sk_path(&self, node: &SceneNode, ox: f32, oy: f32, canvas: &Canvas) {
        let Some(svg) = node.props.get("path").and_then(Json::as_str) else { return };
        let Some(path) = skia_safe::Path::from_svg(svg) else { return };
        let path = path.make_offset((ox, oy));
        let bounds = *path.bounds();
        let paint = self.shape_paint(node, bounds.center_x(), bounds.center_y(), bounds.width().max(bounds.height()) * 0.5);
        canvas.draw_path(&path, &paint);
    }

    fn draw_sk_text(&self, node: &SceneNode, ox: f32, oy: f32, canvas: &Canvas) {
        let Some(text) = node.props.get("text").and_then(Json::as_str) else { return };
        let x = node.props.get("x").and_then(Json::as_f64).unwrap_or(0.0) as f32;
        let y = node.props.get("y").and_then(Json::as_f64).unwrap_or(0.0) as f32;
        let size = node.props.get("size").and_then(Json::as_f64).unwrap_or(16.0) as f32;
        let color = node.props.get("color").and_then(parse_color).unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let font = sized_font(size);
        let mut paint = Paint::new(Color4f::new(color[0], color[1], color[2], color[3]), None);
        paint.set_anti_alias(true);
        canvas.draw_str(text, (ox + x, oy + y), &font, &paint);
    }

    /// No asset-decoding pipeline yet — draws the requested rect as a flat
    /// placeholder so layouts using `<Image>` inside a Canvas are visible and
    /// correctly sized rather than silently blank.
    fn draw_sk_image_placeholder(&self, node: &SceneNode, ox: f32, oy: f32, canvas: &Canvas) {
        let Some(rect) = json_rect(&node.props) else { return };
        let mut paint = Paint::new(Color4f::new(0.5, 0.5, 0.5, 0.4), None);
        paint.set_anti_alias(true);
        canvas.draw_rect(rect.with_offset((ox, oy)), &paint);
    }

    fn draw_circle(&self, node: &SceneNode, ox: f32, oy: f32, canvas: &Canvas) {
        let (cx, cy) = json_point(node.props.get("c")).unwrap_or((0.0, 0.0));
        let r = node.props.get("r").and_then(Json::as_f64).unwrap_or(0.0) as f32;
        let paint = self.shape_paint(node, ox + cx, oy + cy, r);
        canvas.draw_circle((ox + cx, oy + cy), r, &paint);
    }

    fn draw_rect_shape(&self, node: &SceneNode, ox: f32, oy: f32, canvas: &Canvas) {
        let rect = json_rect(&node.props).unwrap_or(Rect::from_xywh(0.0, 0.0, 0.0, 0.0));
        let rect = rect.with_offset((ox, oy));
        let paint = self.shape_paint(node, rect.center_x(), rect.center_y(), rect.width().max(rect.height()) * 0.5);
        canvas.draw_rect(rect, &paint);
    }

    fn draw_rounded_rect(&self, node: &SceneNode, ox: f32, oy: f32, canvas: &Canvas) {
        let rect = json_rect(&node.props).unwrap_or(Rect::from_xywh(0.0, 0.0, 0.0, 0.0)).with_offset((ox, oy));
        let r = node.props.get("r").and_then(Json::as_f64).unwrap_or(0.0) as f32;
        let paint = self.shape_paint(node, rect.center_x(), rect.center_y(), rect.width().max(rect.height()) * 0.5);
        canvas.draw_rrect(RRect::new_rect_xy(rect, r, r), &paint);
    }

    fn draw_group(&self, node: &SceneNode, ox: f32, oy: f32, canvas: &Canvas) {
        canvas.save();

        let (dx, dy) = json_translate(node.props.get("transform"));
        let opacity = node.props.get("opacity").and_then(Json::as_f64).unwrap_or(1.0) as f32;
        let blend_mode = node.props.get("blendMode").and_then(Json::as_str).and_then(json_blend_mode);

        if let Some(clip) = node.props.get("clip") {
            if let Some(rect) = json_rect(clip) {
                let r = clip.get("rx").and_then(Json::as_f64).unwrap_or(0.0) as f32;
                canvas.clip_rrect(RRect::new_rect_xy(rect.with_offset((ox + dx, oy + dy)), r, r), None, Some(true));
            }
        }

        if opacity < 1.0 || blend_mode.is_some() {
            let mut layer_paint = Paint::default();
            layer_paint.set_alpha_f(opacity);
            if let Some(mode) = blend_mode {
                layer_paint.set_blend_mode(mode);
            }
            canvas.save_layer(&SaveLayerRec::default().paint(&layer_paint));
        }

        for &child in &node.children {
            self.draw_sk_node(child, ox + dx, oy + dy, canvas);
        }
        canvas.restore();
    }

    fn draw_box(&self, node: &SceneNode, ox: f32, oy: f32, canvas: &Canvas) {
        let Some(box_val) = node.props.get("box") else { return };
        let Some(rect) = json_rect(box_val) else { return };
        let rect = rect.with_offset((ox, oy));
        let r = box_val.get("rx").and_then(Json::as_f64).unwrap_or(0.0) as f32;
        let rrect = RRect::new_rect_xy(rect, r, r);

        for &child_id in &node.children {
            let child = self.nodes.get(&child_id).expect("unknown child id").borrow();
            if let NodeKind::BoxShadow = child.kind {
                self.draw_box_shadow(&child, &rrect, canvas);
            }
        }

        let mut fill = Paint::new(Color4f::new(1.0, 1.0, 1.0, 1.0), None);
        fill.set_anti_alias(true);
        let mut has_fill = false;
        for &child_id in &node.children {
            let child = self.nodes.get(&child_id).expect("unknown child id").borrow();
            if let NodeKind::LinearGradient = child.kind {
                if let Some(shader) = linear_gradient_shader(&child.props) {
                    fill.set_shader(shader);
                    has_fill = true;
                }
            }
        }
        if has_fill {
            canvas.draw_rrect(rrect, &fill);
        }
    }

    fn draw_box_shadow(&self, shadow: &SceneNode, rrect: &RRect, canvas: &Canvas) {
        let dx = shadow.props.get("dx").and_then(Json::as_f64).unwrap_or(0.0) as f32;
        let dy = shadow.props.get("dy").and_then(Json::as_f64).unwrap_or(0.0) as f32;
        let blur = shadow.props.get("blur").and_then(Json::as_f64).unwrap_or(0.0) as f32;
        let color = shadow.props.get("color").and_then(parse_color).unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let inner = shadow.props.get("inner").and_then(Json::as_bool).unwrap_or(false);
        let sk_color = Color4f::new(color[0], color[1], color[2], color[3]).to_color();

        let filter = if inner {
            image_filters::drop_shadow_only((dx, dy), (blur, blur), sk_color, None, None, None)
        } else {
            image_filters::drop_shadow((dx, dy), (blur, blur), sk_color, None, None, None)
        };
        let Some(filter) = filter else { return };
        let mut paint = Paint::new(Color4f::new(color[0], color[1], color[2], color[3]), None);
        paint.set_anti_alias(true);
        paint.set_image_filter(filter);
        canvas.draw_rrect(*rrect, &paint);
    }
}

fn parse_align(name: &str) -> Align {
    match name {
        "center" => Align::Center,
        "flex-end" => Align::FlexEnd,
        "stretch" => Align::Stretch,
        "baseline" => Align::Baseline,
        "space-between" => Align::SpaceBetween,
        "space-around" => Align::SpaceAround,
        _ => Align::FlexStart,
    }
}

/// Accepts our original `[r, g, b, a]` (0-1 floats) convention *and* real CSS
/// color strings (`@sc/ui`'s theme tokens are plain hex/rgba strings) —
/// hex (#rgb/#rrggbb/#rrggbbaa), rgb()/rgba(), "transparent", and common
/// named colors.
fn parse_color(v: &Json) -> Option<[f32; 4]> {
    if let Some(arr) = v.as_array() {
        return Some([
            arr.first()?.as_f64()? as f32,
            arr.get(1)?.as_f64()? as f32,
            arr.get(2)?.as_f64()? as f32,
            arr.get(3).and_then(Json::as_f64).unwrap_or(1.0) as f32,
        ]);
    }
    parse_css_color(v.as_str()?)
}

fn parse_css_color(s: &str) -> Option<[f32; 4]> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("transparent") {
        return Some([0.0, 0.0, 0.0, 0.0]);
    }
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex_color(hex);
    }
    if let Some(inner) = s.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')) {
        return parse_rgb_components(inner, true);
    }
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        return parse_rgb_components(inner, false);
    }
    named_color(&s.to_ascii_lowercase())
}

fn parse_hex_color(hex: &str) -> Option<[f32; 4]> {
    let nibble = |c: char| -> Option<u8> { c.to_digit(16).map(|d| d as u8) };
    let byte_from_nibble = |c: char| -> Option<f32> { nibble(c).map(|d| (d * 17) as f32 / 255.0) };
    let byte_pair = |s: &str| -> Option<f32> { Some(u8::from_str_radix(s, 16).ok()? as f32 / 255.0) };
    match hex.len() {
        3 => {
            let mut c = hex.chars();
            Some([byte_from_nibble(c.next()?)?, byte_from_nibble(c.next()?)?, byte_from_nibble(c.next()?)?, 1.0])
        }
        4 => {
            let mut c = hex.chars();
            Some([
                byte_from_nibble(c.next()?)?,
                byte_from_nibble(c.next()?)?,
                byte_from_nibble(c.next()?)?,
                byte_from_nibble(c.next()?)?,
            ])
        }
        6 => Some([byte_pair(&hex[0..2])?, byte_pair(&hex[2..4])?, byte_pair(&hex[4..6])?, 1.0]),
        8 => Some([byte_pair(&hex[0..2])?, byte_pair(&hex[2..4])?, byte_pair(&hex[4..6])?, byte_pair(&hex[6..8])?]),
        _ => None,
    }
}

fn parse_rgb_components(inner: &str, has_alpha: bool) -> Option<[f32; 4]> {
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    let component = |s: &str| -> Option<f32> { Some(s.parse::<f32>().ok()? / 255.0) };
    let r = component(parts.first()?)?;
    let g = component(parts.get(1)?)?;
    let b = component(parts.get(2)?)?;
    let a = if has_alpha { parts.get(3)?.parse::<f32>().ok()? } else { 1.0 };
    Some([r, g, b, a])
}

fn named_color(name: &str) -> Option<[f32; 4]> {
    Some(match name {
        "white" => [1.0, 1.0, 1.0, 1.0],
        "black" => [0.0, 0.0, 0.0, 1.0],
        "red" => [1.0, 0.0, 0.0, 1.0],
        "green" => [0.0, 0.502, 0.0, 1.0],
        "lime" => [0.0, 1.0, 0.0, 1.0],
        "blue" => [0.0, 0.0, 1.0, 1.0],
        "gray" | "grey" => [0.502, 0.502, 0.502, 1.0],
        "yellow" => [1.0, 1.0, 0.0, 1.0],
        "orange" => [1.0, 0.647, 0.0, 1.0],
        "purple" => [0.502, 0.0, 0.502, 1.0],
        "pink" => [1.0, 0.753, 0.796, 1.0],
        "cyan" | "aqua" => [0.0, 1.0, 1.0, 1.0],
        "magenta" | "fuchsia" => [1.0, 0.0, 1.0, 1.0],
        "navy" => [0.0, 0.0, 0.502, 1.0],
        "teal" => [0.0, 0.502, 0.502, 1.0],
        "indigo" => [0.294, 0.0, 0.510, 1.0],
        "violet" => [0.933, 0.510, 0.933, 1.0],
        "gold" => [1.0, 0.843, 0.0, 1.0],
        "silver" => [0.753, 0.753, 0.753, 1.0],
        "maroon" => [0.502, 0.0, 0.0, 1.0],
        "olive" => [0.502, 0.502, 0.0, 1.0],
        _ => return None,
    })
}

fn json_point(v: Option<&Json>) -> Option<(f32, f32)> {
    let v = v?;
    Some((v.get("x")?.as_f64()? as f32, v.get("y")?.as_f64()? as f32))
}

fn json_rect(v: &Json) -> Option<Rect> {
    let rect_val = v.get("rect").unwrap_or(v);
    let x = rect_val.get("x").and_then(Json::as_f64).unwrap_or(0.0) as f32;
    let y = rect_val.get("y").and_then(Json::as_f64).unwrap_or(0.0) as f32;
    let w = rect_val.get("width").and_then(Json::as_f64)? as f32;
    let h = rect_val.get("height").and_then(Json::as_f64)? as f32;
    Some(Rect::from_xywh(x, y, w, h))
}

/// Only translation, matching `Transforms3d` as `@sc/ui`'s Atmosphere uses it
/// (`[{ translateX }, { translateY }]`) — rotate/scale are unused so far.
fn json_translate(v: Option<&Json>) -> (f32, f32) {
    let Some(arr) = v.and_then(Json::as_array) else {
        return (0.0, 0.0);
    };
    let mut dx = 0.0;
    let mut dy = 0.0;
    for entry in arr {
        if let Some(x) = entry.get("translateX").and_then(Json::as_f64) {
            dx += x as f32;
        }
        if let Some(y) = entry.get("translateY").and_then(Json::as_f64) {
            dy += y as f32;
        }
    }
    (dx, dy)
}

fn json_blend_mode(name: &str) -> Option<BlendMode> {
    Some(match name {
        "screen" => BlendMode::Screen,
        "multiply" => BlendMode::Multiply,
        "overlay" => BlendMode::Overlay,
        "darken" => BlendMode::Darken,
        "lighten" => BlendMode::Lighten,
        "colorDodge" => BlendMode::ColorDodge,
        "colorBurn" => BlendMode::ColorBurn,
        "hardLight" => BlendMode::HardLight,
        "softLight" => BlendMode::SoftLight,
        "difference" => BlendMode::Difference,
        "exclusion" => BlendMode::Exclusion,
        "plus" => BlendMode::Plus,
        "xor" => BlendMode::Xor,
        _ => return None,
    })
}

fn gradient_colors_and_positions(props: &Json) -> Option<(Vec<Color4f>, Option<Vec<f32>>)> {
    let colors: Vec<Color4f> = props
        .get("colors")?
        .as_array()?
        .iter()
        .map(|c| parse_color(c).map(|[r, g, b, a]| Color4f::new(r, g, b, a)).unwrap_or(Color4f::new(0.0, 0.0, 0.0, 1.0)))
        .collect();
    let positions = props
        .get("positions")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|p| p.as_f64().map(|f| f as f32)).collect());
    Some((colors, positions))
}

fn radial_gradient_shader(props: &Json, fallback_cx: f32, fallback_cy: f32, fallback_r: f32) -> Option<Shader> {
    let (cx, cy) = json_point(props.get("c")).unwrap_or((fallback_cx, fallback_cy));
    let r = props.get("r").and_then(Json::as_f64).map(|r| r as f32).unwrap_or(fallback_r);
    let (colors, positions) = gradient_colors_and_positions(props)?;
    let gradient_colors = gradient::Colors::new(&colors, positions.as_deref(), TileMode::Clamp, None);
    let gradient = gradient::Gradient::new(gradient_colors, gradient::Interpolation::default());
    gradient::shaders::radial_gradient((Point::new(cx, cy), r), &gradient, None)
}

fn linear_gradient_shader(props: &Json) -> Option<Shader> {
    let (sx, sy) = json_point(props.get("start"))?;
    let (ex, ey) = json_point(props.get("end"))?;
    let (colors, positions) = gradient_colors_and_positions(props)?;
    let gradient_colors = gradient::Colors::new(&colors, positions.as_deref(), TileMode::Clamp, None);
    let gradient = gradient::Gradient::new(gradient_colors, gradient::Interpolation::default());
    gradient::shaders::linear_gradient((Point::new(sx, sy), Point::new(ex, ey)), &gradient, None)
}
