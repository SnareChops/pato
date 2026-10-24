//! The "Model" dashboard widget: which model is loaded, where it is, whether
//! tracking sees a face, and a button per expression and hotkey.

use crate::model::Model;
use crate::positions::Preset;
use crate::pato::plugin::{widget_dom as dom, widgets};

use super::builder::{blank_style, plain, tiles, Builder};

pub const WIDGET_ID: &str = "vts_model";

/// Handler-id prefixes; the rest is the expression file / hotkey id.
pub const PREFIX_EXPRESSION: &str = "exp:";
pub const PREFIX_HOTKEY: &str = "hk:";
pub const PREFIX_POSITION: &str = "pos:";
/// Right-click on a saved position.
pub const PREFIX_POSITION_MENU: &str = "posmenu:";
pub const HANDLER_ADD_POSITION: &str = "pos-add";

/// One button in a `section`.
struct Item<'a> {
    handler: String,
    label: &'a str,
    active: bool,
    tooltip: Option<&'a str>,
    /// Handler for a right-click on the button.
    context_menu: Option<String>,
}

pub struct VtsModelWidget;

impl VtsModelWidget {
    pub fn new() -> Self {
        widgets::register(&widgets::WidgetSpec {
            id: WIDGET_ID.to_string(),
            size: tiles(8, 8),
            min: Some(tiles(4, 4)),
            max: Some(tiles(20, 20)),
        });
        VtsModelWidget
    }

    pub fn render(&self, connected: bool, model: &Model, presets: &[Preset]) {
        let mut b = Builder::default();
        let root = b.element(
            None,
            dom::Element {
                style: Some(dom::Style {
                    layout: Some(dom::Layout::FlexCol),
                    gap: Some(dom::Space::Md),
                    padding: Some(dom::Space::Md),
                    height: Some(dom::Size::Fill),
                    overflow: Some(dom::Overflow::Scroll),
                    ..blank_style()
                }),
                ..plain(dom::Tag::Div)
            },
        );

        if !connected {
            muted(&mut b, root, "Not connected to VTube Studio");
        } else if !model.loaded {
            muted(&mut b, root, "No model loaded");
        } else {
            header(&mut b, root, model);
            if let Some(p) = &model.position {
                let line = b.element(Some(root), text_style(dom::ThemeColor::FgMuted, dom::TextSize::Sm));
                b.text(
                    line,
                    &format!("X {:.2}  Y {:.2}  Rotation {:.0}\u{b0}  Size {:.1}", p.x, p.y, p.rotation, p.size),
                );
            }
            section(&mut b, root, "Expressions", "No expressions", model.expressions.iter().map(|e| Item {
                handler: format!("{PREFIX_EXPRESSION}{}", e.file),
                label: e.name.as_str(),
                active: e.active,
                tooltip: None,
                context_menu: None,
            }));
            let hotkeys: Vec<_> = model.extra_hotkeys().collect();
            if !hotkeys.is_empty() {
                section(&mut b, root, "Hotkeys", "", hotkeys.into_iter().map(|h| Item {
                    handler: format!("{PREFIX_HOTKEY}{}", h.id),
                    label: if h.name.is_empty() { h.id.as_str() } else { h.name.as_str() },
                    active: false,
                    tooltip: Some(h.kind.as_str()),
                    context_menu: None,
                }));
            }
            let saved = presets.iter().map(|p| Item {
                handler: format!("{PREFIX_POSITION}{}", p.id),
                label: p.name.as_str(),
                active: model.position.as_ref().is_some_and(|at| p.matches(at)),
                tooltip: Some("Right-click to edit"),
                context_menu: Some(format!("{PREFIX_POSITION_MENU}{}", p.id)),
            });
            let add = Item {
                handler: HANDLER_ADD_POSITION.to_string(),
                label: "+",
                active: false,
                tooltip: Some("Save Position"),
                context_menu: None,
            };
            section(&mut b, root, "Positions", "", saved.chain(std::iter::once(add)));
        }

        widgets::update(&widgets::Widget::Custom(widgets::CustomWidget {
            id: WIDGET_ID.to_string(),
            nodes: b.nodes,
        }));
    }
}

fn header(b: &mut Builder, root: u32, model: &Model) {
    let row = b.element(
        Some(root),
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexRow),
                align: Some(dom::Align::Center),
                justify: Some(dom::Justify::Between),
                gap: Some(dom::Space::Md),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );
    let name = b.element(Some(row), plain(dom::Tag::Strong));
    b.text(name, &model.name);
    if let Some(found) = model.face_found {
        let (color, text) = if found {
            (dom::ThemeColor::Success, "\u{25cf} Tracking")
        } else {
            (dom::ThemeColor::Danger, "\u{25cb} No face")
        };
        let tag = b.element(Some(row), text_style(color, dom::TextSize::Sm));
        b.text(tag, text);
    }
}

/// A titled, wrapping row of buttons.
fn section<'a>(b: &mut Builder, root: u32, title: &str, empty: &str, items: impl Iterator<Item = Item<'a>>) {
    let section = b.element(
        Some(root),
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexCol),
                gap: Some(dom::Space::Sm),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );
    let heading = b.element(Some(section), text_style(dom::ThemeColor::FgMuted, dom::TextSize::Sm));
    b.text(heading, title);
    let row = b.element(
        Some(section),
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexRow),
                gap: Some(dom::Space::Sm),
                wrap: Some(true),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );
    let mut any = false;
    for Item { handler, label, active, tooltip, context_menu } in items {
        any = true;
        let (background, color) = if active {
            (dom::ThemeColor::Primary, dom::ThemeColor::Bg)
        } else {
            (dom::ThemeColor::BgRaised, dom::ThemeColor::Fg)
        };
        let button = b.element(
            Some(row),
            dom::Element {
                key: Some(handler.clone()),
                attrs: tooltip.map(|t| vec![dom::Attr::Title(t.to_string())]).unwrap_or_default(),
                events: std::iter::once(dom::Binding {
                    kind: dom::EventKind::Click,
                    handler,
                })
                .chain(context_menu.map(|handler| dom::Binding {
                    kind: dom::EventKind::ContextMenu,
                    handler,
                }))
                .collect(),
                style: Some(dom::Style {
                    padding: Some(dom::Space::Sm),
                    radius: Some(dom::Space::Sm),
                    background: Some(background),
                    color: Some(color),
                    ..blank_style()
                }),
                ..plain(dom::Tag::Button)
            },
        );
        b.text(button, label);
    }
    if !any && !empty.is_empty() {
        muted(b, section, empty);
    }
}

fn muted(b: &mut Builder, parent: u32, text: &str) {
    let p = b.element(Some(parent), text_style(dom::ThemeColor::FgMuted, dom::TextSize::Md));
    b.text(p, text);
}

fn text_style(color: dom::ThemeColor, size: dom::TextSize) -> dom::Element {
    dom::Element {
        style: Some(dom::Style {
            color: Some(color),
            font_size: Some(size),
            ..blank_style()
        }),
        ..plain(dom::Tag::P)
    }
}
