//! The connect/settings modal: host, port and (optional) password, opened
//! from the status widget's "Connect"/"Settings" action. A single-submit
//! form - unlike the redeems form, nothing here needs live per-keystroke
//! state, so only `submit` and the cancel button's `click` are bound, and
//! there's no draft to carry between events (`on_modal_closed` is a no-op).

use crate::pato::plugin::{modals, widget_dom as dom};
use crate::widgets::{blank_style, plain, Builder};

pub const MODAL_ID: &str = "connect";
pub const HANDLER_SUBMIT: &str = "connect-submit";
pub const HANDLER_CANCEL: &str = "connect-cancel";

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: &str = "4455";

/// The submitted form, read into whatever the caller (`lib.rs`) needs to
/// validate and act on.
pub struct Submitted {
    pub host: String,
    pub port: String,
    pub password: String,
}

impl Submitted {
    pub fn from_fields(fields: &[(String, String)]) -> Self {
        let get = |name: &str| {
            fields
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        Submitted {
            host: get("host"),
            port: get("port"),
            password: get("password"),
        }
    }
}

/// (Re)opens the modal prefilled with `host`/`port`/`password` (empty for a
/// fresh connect, the saved values for "Settings"), and `error` from a
/// previous failed attempt. Re-opening an id that's already showing updates
/// it in place, so this doubles as "redraw with an error".
pub fn open(host: &str, port: &str, password: &str, error: Option<&str>) -> bool {
    let mut b = Builder::default();
    let root = b.element(
        None,
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexCol),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );
    let form = b.element(
        Some(root),
        dom::Element {
            key: Some("form".into()),
            events: vec![dom::Binding {
                kind: dom::EventKind::Submit,
                handler: HANDLER_SUBMIT.into(),
            }],
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexCol),
                gap: Some(dom::Space::Lg),
                ..blank_style()
            }),
            ..plain(dom::Tag::Form)
        },
    );

    if let Some(error) = error {
        let p = b.element(
            Some(form),
            dom::Element {
                style: Some(dom::Style {
                    color: Some(dom::ThemeColor::Danger),
                    font_size: Some(dom::TextSize::Sm),
                    ..blank_style()
                }),
                ..plain(dom::Tag::P)
            },
        );
        b.text(p, error);
    }

    field(&mut b, form, "f-host", "host", "Host", dom::InputType::Text, host, vec![dom::Attr::Required(true)]);
    field(
        &mut b,
        form,
        "f-port",
        "port",
        "Port",
        dom::InputType::Number,
        port,
        vec![dom::Attr::Required(true), dom::Attr::Min(1.0), dom::Attr::Max(65535.0), dom::Attr::Step(1.0)],
    );
    field(
        &mut b,
        form,
        "f-password",
        "password",
        "Password (leave blank if OBS has no WebSocket password set)",
        dom::InputType::Password,
        password,
        vec![],
    );

    let actions = b.element(
        Some(form),
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexRow),
                gap: Some(dom::Space::Md),
                justify: Some(dom::Justify::End),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );
    let cancel = b.element(
        Some(actions),
        dom::Element {
            key: Some("f-cancel".into()),
            events: vec![dom::Binding {
                kind: dom::EventKind::Click,
                handler: HANDLER_CANCEL.into(),
            }],
            ..plain(dom::Tag::Button)
        },
    );
    b.text(cancel, "Cancel");
    let connect = b.element(
        Some(actions),
        dom::Element {
            key: Some("f-connect".into()),
            attrs: vec![dom::Attr::Submits(true)],
            style: Some(dom::Style {
                background: Some(dom::ThemeColor::Primary),
                color: Some(dom::ThemeColor::Bg),
                ..blank_style()
            }),
            ..plain(dom::Tag::Button)
        },
    );
    b.text(connect, "Connect");

    modals::open(&modals::Modal {
        id: MODAL_ID.to_string(),
        title: "Connect to OBS".to_string(),
        size: modals::ModalSize::Sm,
        nodes: b.nodes,
    })
}

pub fn close() {
    modals::close(MODAL_ID);
}

#[allow(clippy::too_many_arguments)]
fn field(b: &mut Builder, parent: u32, key: &str, name: &str, label: &str, kind: dom::InputType, value: &str, mut extra: Vec<dom::Attr>) {
    let field = b.element(
        Some(parent),
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexCol),
                gap: Some(dom::Space::Xs),
                ..blank_style()
            }),
            ..plain(dom::Tag::Label)
        },
    );
    b.text(field, label);
    let mut attrs = vec![
        dom::Attr::Kind(kind),
        dom::Attr::Name(name.to_string()),
        // Set as the input's `.value` property by the webview (custom-widget.ts),
        // not an HTML attribute - lets "Settings" prefill the saved password
        // without it showing up in the rendered markup.
        dom::Attr::Value(value.to_string()),
    ];
    attrs.append(&mut extra);
    b.element(
        Some(field),
        dom::Element {
            key: Some(key.to_string()),
            attrs,
            ..plain(dom::Tag::Input)
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_submitted_fields_by_name_and_defaults_missing_ones() {
        let fields = vec![("host".to_string(), "127.0.0.1".to_string()), ("port".to_string(), "4455".to_string())];
        let s = Submitted::from_fields(&fields);
        assert_eq!(s.host, "127.0.0.1");
        assert_eq!(s.port, "4455");
        assert_eq!(s.password, "");
    }
}
