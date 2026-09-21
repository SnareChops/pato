//! Small shared helpers for building a custom-widget flat node table
//! (see `custom-widgets.md`). One `Builder` per `render()` call.

use crate::pato::plugin::widget_dom as dom;

pub fn tiles(w: u32, h: u32) -> dom::TileSize {
    dom::TileSize { w, h }
}

/// An element with no key, attrs, style or events - fill in what's needed
/// with `..plain(tag)`.
pub fn plain(tag: dom::Tag) -> dom::Element {
    dom::Element {
        key: None,
        tag,
        attrs: vec![],
        style: None,
        events: vec![],
    }
}

/// A fully-`None` style record - fill in what's needed with `..blank_style()`.
pub fn blank_style() -> dom::Style {
    dom::Style {
        layout: None,
        gap: None,
        padding: None,
        margin: None,
        radius: None,
        color: None,
        background: None,
        font_size: None,
        align: None,
        justify: None,
        grow: None,
        wrap: None,
        width: None,
        height: None,
        overflow: None,
        sticky: None,
    }
}

/// Builds the flat node table: `element`/`text`/`slot` append a node and
/// return its index for use as a parent.
#[derive(Default)]
pub struct Builder {
    pub nodes: Vec<dom::Node>,
}
impl Builder {
    fn push(&mut self, parent: Option<u32>, kind: dom::NodeKind) -> u32 {
        let index = self.nodes.len() as u32;
        self.nodes.push(dom::Node { parent, kind });
        index
    }
    pub fn element(&mut self, parent: Option<u32>, el: dom::Element) -> u32 {
        self.push(parent, dom::NodeKind::Element(el))
    }
    pub fn text(&mut self, parent: u32, value: &str) -> u32 {
        self.push(Some(parent), dom::NodeKind::Text(value.to_string()))
    }
    pub fn slot(&mut self, parent: u32, name: &str) -> u32 {
        self.push(Some(parent), dom::NodeKind::ValueSlot(name.to_string()))
    }
}
