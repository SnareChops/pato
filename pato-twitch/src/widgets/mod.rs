mod builder;
pub mod primary_channel;
mod chat;
mod redeems;
mod status;

pub use chat::TwitchChatWidget;
pub use redeems::{TwitchRedeemsWidget, MODAL_FORM, WIDGET_ID as REDEEMS_WIDGET_ID};
pub use status::{TwitchStatusWidget, ACTION_DISCONNECT, ACTION_PRIMARY_CHANNEL};

use crate::pato::plugin::widget_dom as dom;

/// The Twitch brand icon, drawn small next to a widget's heading so it is
/// obvious at a glance which platform the widget controls. `platform:twitch`
/// is resolved by the core to a bundled asset.
pub(crate) fn platform_badge() -> dom::Element {
    dom::Element {
        attrs: vec![
            dom::Attr::Src("platform:twitch".into()),
            dom::Attr::Alt("Twitch".into()),
        ],
        style: Some(dom::Style {
            width: Some(dom::Size::Fixed(dom::Space::Md)),
            height: Some(dom::Size::Fixed(dom::Space::Md)),
            ..builder::blank_style()
        }),
        ..builder::plain(dom::Tag::Img)
    }
}
