//! A small custom widget that exercises the full custom-widget path: a styled
//! column with a heading, a value-slot for the signed-in user, and a Refresh
//! button whose click comes back through `widget-events.on-event`.

use crate::helix::TwitchUser;
use crate::pato::plugin::{widget_dom as dom, widgets};

use super::builder::{blank_style, plain, tiles, Builder};

const WIDGET_ID: &str = "panel";
const SLOT_USER: &str = "user";
pub const HANDLER_REFRESH: &str = "refresh";

pub struct TwitchPanel;

impl TwitchPanel {
    pub fn new() -> Self {
        widgets::register(&widgets::WidgetSpec {
            id: WIDGET_ID.to_string(),
            size: tiles(6, 4),
            min: Some(tiles(4, 3)),
            max: Some(tiles(12, 8)),
        });
        let panel = TwitchPanel;
        panel.render(None);
        panel
    }

    /// Rebuild the whole tree (immediate mode) and refresh the user slot.
    pub fn render(&self, user: Option<&TwitchUser>) {
        let mut b = Builder::default();

        let root = b.element(
            None,
            dom::Element {
                key: None,
                tag: dom::Tag::Div,
                attrs: vec![],
                style: Some(dom::Style {
                    layout: Some(dom::Layout::FlexCol),
                    gap: Some(dom::Space::Sm),
                    padding: Some(dom::Space::Md),
                    color: Some(dom::ThemeColor::Fg),
                    ..blank_style()
                }),
                events: vec![],
            },
        );

        let heading = b.element(Some(root), plain(dom::Tag::H3));
        b.text(heading, "Twitch");

        let status = b.element(
            Some(root),
            dom::Element {
                style: Some(dom::Style {
                    color: Some(dom::ThemeColor::FgMuted),
                    font_size: Some(dom::TextSize::Sm),
                    ..blank_style()
                }),
                ..plain(dom::Tag::P)
            },
        );
        b.text(status, "Signed in as ");
        b.slot(status, SLOT_USER);

        let button = b.element(
            Some(root),
            dom::Element {
                key: Some("refresh".into()),
                events: vec![dom::Binding {
                    kind: dom::EventKind::Click,
                    handler: HANDLER_REFRESH.into(),
                }],
                ..plain(dom::Tag::Button)
            },
        );
        b.text(button, "Refresh");

        widgets::update(&widgets::Widget::Custom(widgets::CustomWidget {
            id: WIDGET_ID.to_string(),
            nodes: b.nodes,
        }));
        self.set_user(user);
    }

    /// Cheap update of just the user slot (no re-render).
    pub fn set_user(&self, user: Option<&TwitchUser>) {
        let label = user
            .map(|u| u.display_name.clone())
            .unwrap_or_else(|| "nobody".to_string());
        widgets::set_value(WIDGET_ID, SLOT_USER, &label);
    }
}

