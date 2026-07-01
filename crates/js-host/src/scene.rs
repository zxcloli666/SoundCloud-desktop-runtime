//! The mounted tree behind a `react-reconciler` host-config. Two kinds of
//! node share one tree:
//! - **layout nodes** (View/Text/Canvas) — real Yoga geometry, flexbox props.
//! - **Skia draw nodes** (Circle/RoundedRect/Group/Blur/gradients/Box/
//!   BoxShadow) — no Yoga; like real react-native-skia, they're positioned in
//!   raw pixel coordinates within their nearest ancestor Canvas, mirroring
//!   `@shopify/react-native-skia`'s JSX surface (see Desktop-Runtime/CLAUDE.md)
//!   without replicating its internal two-reconciler/SkPicture architecture —
//!   we own the whole pipeline, so one tree is enough.

use std::cell::RefCell;
use std::collections::HashMap;

use ordered_float::OrderedFloat;
use serde::Deserialize;
use serde_json::Value as Json;
use skia_safe::{
    BlendMode, BlurStyle, Canvas, Color4f, Font, FontMgr, FontStyle, MaskFilter, Paint, PaintStyle,
    Point, RRect, Rect, Shader, TileMode, gradient, image_filters,
};
use yoga::{Align, Direction, Edge, FlexDirection, Justify, Node as YogaNode, StyleUnit};

pub type NodeId = u32;

pub enum NodeKind {
    View,
    Text(String),
    Canvas,
    Circle,
    Rect,
    RoundedRect,
    Group,
    Blur,
    RadialGradient,
    LinearGradient,
    Box,
    BoxShadow,
}

pub struct SceneNode {
    kind: NodeKind,
    /// `None` for Skia draw nodes — they don't participate in flexbox.
    yoga: Option<YogaNode>,
    background: Option<[f32; 4]>,
    /// Raw props for Skia draw nodes, parsed against their kind at draw time
    /// (the prop shapes are too varied per type to justify one big struct).
    props: Json,
    children: Vec<NodeId>,
}

impl SceneNode {
    fn layout(kind: NodeKind) -> Self {
        Self { kind, yoga: Some(YogaNode::new()), background: None, props: Json::Null, children: Vec::new() }
    }

    fn sk(kind: NodeKind) -> Self {
        Self { kind, yoga: None, background: None, props: Json::Null, children: Vec::new() }
    }

    fn text(text: String) -> Self {
        let mut yoga = YogaNode::new();
        // Placeholder metrics — real font measurement is a Skia concern that
        // lands with real text support, not here.
        yoga.set_width(pt(text.chars().count() as f32 * 8.0 + 4.0));
        yoga.set_height(pt(20.0));
        Self { kind: NodeKind::Text(text), yoga: Some(yoga), background: None, props: Json::Null, children: Vec::new() }
    }
}

fn pt(v: f32) -> StyleUnit {
    StyleUnit::Point(OrderedFloat(v))
}

/// Mirrors the subset of `@sc/ui` style props this spike proves out: box
/// layout (flex/padding/margin) and a background fill. Grows as later spikes
/// need more of RN's style surface.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleInput {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub flex_grow: Option<f32>,
    pub flex_direction: Option<String>,
    pub justify_content: Option<String>,
    pub align_items: Option<String>,
    pub padding: Option<f32>,
    pub margin: Option<f32>,
    pub background_color: Option<[f32; 4]>,
}

#[derive(Default)]
pub struct Scene {
    nodes: HashMap<NodeId, RefCell<SceneNode>>,
    next_id: NodeId,
    pub root: Option<NodeId>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    fn alloc(&mut self, node: SceneNode) -> NodeId {
        self.next_id += 1;
        let id = self.next_id;
        self.nodes.insert(id, RefCell::new(node));
        id
    }

    pub fn create_view(&mut self) -> NodeId {
        self.alloc(SceneNode::layout(NodeKind::View))
    }

    pub fn create_text(&mut self, text: String) -> NodeId {
        self.alloc(SceneNode::text(text))
    }

    /// `kind_name` matches the host-config `type` string from `js/src/rnskia`
    /// (e.g. "Canvas", "Circle", "Group", "RadialGradient", "BoxShadow"...).
    pub fn create_sk_node(&mut self, kind_name: &str) -> NodeId {
        let kind = match kind_name {
            "Canvas" => return self.alloc(SceneNode::layout(NodeKind::Canvas)),
            "Circle" => NodeKind::Circle,
            "Rect" => NodeKind::Rect,
            "RoundedRect" => NodeKind::RoundedRect,
            "Group" => NodeKind::Group,
            "Blur" => NodeKind::Blur,
            "RadialGradient" => NodeKind::RadialGradient,
            "LinearGradient" => NodeKind::LinearGradient,
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
    }

    pub fn set_style(&mut self, id: NodeId, style: StyleInput) {
        let cell = self.nodes.get(&id).expect("unknown node id");
        let mut node = cell.borrow_mut();
        let Some(yoga) = node.yoga.as_mut() else {
            return;
        };
        if let Some(w) = style.width {
            yoga.set_width(pt(w));
        }
        if let Some(h) = style.height {
            yoga.set_height(pt(h));
        }
        if let Some(fg) = style.flex_grow {
            yoga.set_flex_grow(fg);
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
        if let Some(justify) = style.justify_content.as_deref() {
            let justify = match justify {
                "center" => Justify::Center,
                "flex-end" => Justify::FlexEnd,
                "space-between" => Justify::SpaceBetween,
                "space-around" => Justify::SpaceAround,
                _ => Justify::FlexStart,
            };
            yoga.set_justify_content(justify);
        }
        if let Some(align) = style.align_items.as_deref() {
            let align = match align {
                "center" => Align::Center,
                "flex-end" => Align::FlexEnd,
                "stretch" => Align::Stretch,
                _ => Align::FlexStart,
            };
            yoga.set_align_items(align);
        }
        if let Some(p) = style.padding {
            for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
                yoga.set_padding(edge, pt(p));
            }
        }
        if let Some(m) = style.margin {
            for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
                yoga.set_margin(edge, pt(m));
            }
        }
        if let Some(bg) = style.background_color {
            node.background = Some(bg);
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

    pub fn compute_layout(&mut self, width: f32, height: f32) {
        let Some(root) = self.root else { return };
        let cell = self.nodes.get(&root).expect("unknown root id");
        let mut node = cell.borrow_mut();
        let yoga = node.yoga.as_mut().expect("root must be a layout node");
        yoga.calculate_layout(width, height, Direction::LTR);
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
        // `Font::default()` carries an empty (0-glyph) typeface — Skia only
        // picks a real one through the font manager. Resolved once per frame,
        // not per node.
        let typeface = FontMgr::default()
            .legacy_make_typeface(None, FontStyle::default())
            .expect("no system default typeface available");
        let font = Font::from_typeface(typeface, 16.0);
        self.draw_layout_node(root, 0.0, 0.0, canvas, &font);
    }

    fn draw_layout_node(&self, id: NodeId, parent_x: f32, parent_y: f32, canvas: &Canvas, font: &Font) {
        let (x, y, w, h, background, text, is_canvas, children) = {
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
            (x, y, w, h, node.background, text, is_canvas, node.children.clone())
        };

        if let Some([r, g, b, a]) = background {
            let mut paint = Paint::new(Color4f::new(r, g, b, a), None);
            paint.set_anti_alias(true);
            canvas.draw_rrect(RRect::new_rect_xy(Rect::from_xywh(x, y, w, h), 8.0, 8.0), &paint);
        }

        if let Some(text) = text {
            let mut paint = Paint::new(Color4f::new(1.0, 1.0, 1.0, 1.0), None);
            paint.set_anti_alias(true);
            canvas.draw_str(&text, (x, y + h * 0.5 + 5.0), font, &paint);
        }

        if is_canvas {
            canvas.save();
            canvas.clip_rect(Rect::from_xywh(x, y, w, h), None, Some(true));
            for child in children {
                self.draw_sk_node(child, x, y, canvas);
            }
            canvas.restore();
            return;
        }

        for child in children {
            self.draw_layout_node(child, x, y, canvas, font);
        }
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
            NodeKind::Group => self.draw_group(&node, ox, oy, canvas),
            NodeKind::Box => self.draw_box(&node, ox, oy, canvas),
            // Configuration-only nodes: meaningful as a child of Circle/RRect/
            // Box/Group, not as something independently drawn.
            NodeKind::Blur | NodeKind::RadialGradient | NodeKind::LinearGradient | NodeKind::BoxShadow => {}
            NodeKind::View | NodeKind::Text(_) | NodeKind::Canvas => {
                unreachable!("layout node encountered in the Skia subtree")
            }
        }
    }

    fn shape_paint(&self, node: &SceneNode, cx: f32, cy: f32, radius: f32) -> Paint {
        let color = node.props.get("color").and_then(json_color).unwrap_or([1.0, 1.0, 1.0, 1.0]);
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
                _ => {}
            }
        }
        paint
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
            canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default().paint(&layer_paint));
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
        let color = shadow.props.get("color").and_then(json_color).unwrap_or([0.0, 0.0, 0.0, 1.0]);
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

fn json_color(v: &Json) -> Option<[f32; 4]> {
    let arr = v.as_array()?;
    Some([
        arr.first()?.as_f64()? as f32,
        arr.get(1)?.as_f64()? as f32,
        arr.get(2)?.as_f64()? as f32,
        arr.get(3).and_then(Json::as_f64).unwrap_or(1.0) as f32,
    ])
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
        "plus" => BlendMode::Plus,
        _ => return None,
    })
}

fn gradient_colors_and_positions(props: &Json) -> Option<(Vec<Color4f>, Option<Vec<f32>>)> {
    let colors: Vec<Color4f> = props
        .get("colors")?
        .as_array()?
        .iter()
        .map(|c| {
            if c.as_str() == Some("transparent") {
                return Color4f::new(0.0, 0.0, 0.0, 0.0);
            }
            json_color(c).map(|[r, g, b, a]| Color4f::new(r, g, b, a)).unwrap_or(Color4f::new(0.0, 0.0, 0.0, 1.0))
        })
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
