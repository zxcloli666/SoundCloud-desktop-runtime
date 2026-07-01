//! Spike 4 (DESKTOP_RUNTIME_TZ.md): the mounted tree behind a `react-reconciler`
//! host-config. Real Yoga computes geometry; paint props (background, text) are
//! ours, since Yoga only knows layout, not drawing.

use std::cell::RefCell;
use std::collections::HashMap;

use ordered_float::OrderedFloat;
use serde::Deserialize;
use skia_safe::{Canvas, Color4f, Font, FontMgr, FontStyle, Paint, RRect, Rect};
use yoga::{Align, Direction, Edge, FlexDirection, Justify, Node as YogaNode, StyleUnit};

pub type NodeId = u32;

pub enum NodeKind {
    View,
    Text(String),
}

pub struct SceneNode {
    kind: NodeKind,
    yoga: YogaNode,
    background: Option<[f32; 4]>,
    children: Vec<NodeId>,
}

impl SceneNode {
    fn view() -> Self {
        Self {
            kind: NodeKind::View,
            yoga: YogaNode::new(),
            background: None,
            children: Vec::new(),
        }
    }

    fn text(text: String) -> Self {
        let mut yoga = YogaNode::new();
        // Placeholder metrics — real font measurement is a Skia concern that
        // lands with the react-native-skia/Text backing (spike 5+), not here.
        yoga.set_width(pt(text.chars().count() as f32 * 8.0 + 4.0));
        yoga.set_height(pt(20.0));
        Self { kind: NodeKind::Text(text), yoga, background: None, children: Vec::new() }
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
        self.alloc(SceneNode::view())
    }

    pub fn create_text(&mut self, text: String) -> NodeId {
        self.alloc(SceneNode::text(text))
    }

    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        let parent_cell = self.nodes.get(&parent).expect("unknown parent id");
        let child_cell = self.nodes.get(&child).expect("unknown child id");
        // Two RefCells, not one — borrow_mut on each is independently checked,
        // so this doesn't panic even though both live in the same HashMap.
        let mut parent_node = parent_cell.borrow_mut();
        let mut child_node = child_cell.borrow_mut();
        let index = parent_node.children.len();
        parent_node.yoga.insert_child(&mut child_node.yoga, index);
        parent_node.children.push(child);
    }

    pub fn remove_child(&mut self, parent: NodeId, child: NodeId) {
        let parent_cell = self.nodes.get(&parent).expect("unknown parent id");
        let child_cell = self.nodes.get(&child).expect("unknown child id");
        let mut parent_node = parent_cell.borrow_mut();
        let mut child_node = child_cell.borrow_mut();
        parent_node.yoga.remove_child(&mut child_node.yoga);
        parent_node.children.retain(|id| *id != child);
    }

    pub fn set_style(&mut self, id: NodeId, style: StyleInput) {
        let cell = self.nodes.get(&id).expect("unknown node id");
        let mut node = cell.borrow_mut();
        if let Some(w) = style.width {
            node.yoga.set_width(pt(w));
        }
        if let Some(h) = style.height {
            node.yoga.set_height(pt(h));
        }
        if let Some(fg) = style.flex_grow {
            node.yoga.set_flex_grow(fg);
        }
        if let Some(dir) = style.flex_direction.as_deref() {
            let dir = match dir {
                "column" => FlexDirection::Column,
                "row-reverse" => FlexDirection::RowReverse,
                "column-reverse" => FlexDirection::ColumnReverse,
                _ => FlexDirection::Row,
            };
            node.yoga.set_flex_direction(dir);
        }
        if let Some(justify) = style.justify_content.as_deref() {
            let justify = match justify {
                "center" => Justify::Center,
                "flex-end" => Justify::FlexEnd,
                "space-between" => Justify::SpaceBetween,
                "space-around" => Justify::SpaceAround,
                _ => Justify::FlexStart,
            };
            node.yoga.set_justify_content(justify);
        }
        if let Some(align) = style.align_items.as_deref() {
            let align = match align {
                "center" => Align::Center,
                "flex-end" => Align::FlexEnd,
                "stretch" => Align::Stretch,
                _ => Align::FlexStart,
            };
            node.yoga.set_align_items(align);
        }
        if let Some(p) = style.padding {
            for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
                node.yoga.set_padding(edge, pt(p));
            }
        }
        if let Some(m) = style.margin {
            for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
                node.yoga.set_margin(edge, pt(m));
            }
        }
        if let Some(bg) = style.background_color {
            node.background = Some(bg);
        }
    }

    pub fn set_root(&mut self, id: NodeId) {
        self.root = Some(id);
    }

    pub fn compute_layout(&mut self, width: f32, height: f32) {
        let Some(root) = self.root else { return };
        let cell = self.nodes.get(&root).expect("unknown root id");
        cell.borrow_mut().yoga.calculate_layout(width, height, Direction::LTR);
    }

    pub fn children_of(&self, id: NodeId) -> Vec<NodeId> {
        self.nodes.get(&id).expect("unknown node id").borrow().children.clone()
    }

    /// `(left, top, width, height)` relative to this node's parent — for
    /// tests/introspection; drawing walks the tree itself via `draw()`.
    pub fn layout_of(&self, id: NodeId) -> (f32, f32, f32, f32) {
        let node = self.nodes.get(&id).expect("unknown node id").borrow();
        (
            node.yoga.get_layout_left(),
            node.yoga.get_layout_top(),
            node.yoga.get_layout_width(),
            node.yoga.get_layout_height(),
        )
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
        self.draw_node(root, 0.0, 0.0, canvas, &font);
    }

    fn draw_node(&self, id: NodeId, parent_x: f32, parent_y: f32, canvas: &Canvas, font: &Font) {
        let (x, y, w, h, background, text, children) = {
            let node = self.nodes.get(&id).expect("unknown node id").borrow();
            let x = parent_x + node.yoga.get_layout_left();
            let y = parent_y + node.yoga.get_layout_top();
            let w = node.yoga.get_layout_width();
            let h = node.yoga.get_layout_height();
            let text = match &node.kind {
                NodeKind::Text(t) => Some(t.clone()),
                NodeKind::View => None,
            };
            (x, y, w, h, node.background, text, node.children.clone())
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

        for child in children {
            self.draw_node(child, x, y, canvas, font);
        }
    }
}
