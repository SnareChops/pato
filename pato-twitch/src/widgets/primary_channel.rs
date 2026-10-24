//! The "Primary channel" prompt, opened from the status widget's menu.
//!
//! The primary channel is the one chat, chat commands and channel point
//! redeems are all tied to. It defaults to the signed-in account, but a
//! streamer who runs their bot from a separate account needs to point it at
//! their own channel instead. This is a rarely changed setting, so it is a
//! plain modal form rather than anything on the dashboard.

use crate::exports::pato::plugin::widget_events::{EventKind, Payload, WidgetEvent};
use crate::pato::plugin::{modals, widget_dom as dom};

use super::builder::{blank_style, plain, Builder};

pub const MODAL_PRIMARY_CHANNEL: &str = "primary-channel";

const HANDLER_SUBMIT: &str = "pc-submit";
const HANDLER_CANCEL: &str = "pc-cancel";
const HANDLER_RESET: &str = "pc-reset";

/// What the user did with the prompt.
pub enum Outcome {
    /// Saved this (already normalised) channel name.
    Set(String),
    /// Went back to the signed-in account.
    Reset,
    /// Closed without changing anything (or the event wasn't ours).
    Cancelled,
    /// Typed something that isn't a channel name; the form was re-opened with
    /// the problem shown.
    Invalid,
}

/// Lowercase, no `#`, and a valid Twitch login (4-25 of `a-z 0-9 _`).
pub fn normalize(raw: &str) -> Result<String, String> {
    let name = raw.trim().trim_start_matches('#').to_lowercase();
    let valid = (4..=25).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if valid {
        Ok(name)
    } else {
        Err("Enter a Twitch channel name: 4-25 letters, numbers or underscores.".to_string())
    }
}

/// Show the prompt. `current` is the channel in use; `account` is the
/// signed-in user's own channel (what "Use my account" goes back to).
pub fn open(current: &str, account: &str, error: Option<&str>) {
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

    let help = b.element(Some(form), text_style(dom::ThemeColor::FgMuted));
    b.text(
        help,
        &format!(
            "Chat, chat commands and channel point redeems all use this channel. \
             Leave it as your account (#{account}) unless you run Pato from a separate \
             bot account. Redeems can only be managed when you are signed in as the \
             channel's own account."
        ),
    );
    if let Some(error) = error {
        let p = b.element(Some(form), text_style(dom::ThemeColor::Danger));
        b.text(p, error);
    }

    let label = b.element(
        Some(form),
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexCol),
                gap: Some(dom::Space::Xs),
                ..blank_style()
            }),
            ..plain(dom::Tag::Label)
        },
    );
    b.text(label, "Channel");
    b.element(
        Some(label),
        dom::Element {
            key: Some("pc-channel".into()),
            attrs: vec![
                dom::Attr::Name("channel".into()),
                dom::Attr::Value(current.to_string()),
                dom::Attr::Required(true),
                dom::Attr::MaxLength(26),
            ],
            ..plain(dom::Tag::Input)
        },
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
    let reset = b.element(Some(actions), click_button("pc-reset", HANDLER_RESET));
    b.text(reset, "Use my account");
    let cancel = b.element(Some(actions), click_button("pc-cancel", HANDLER_CANCEL));
    b.text(cancel, "Cancel");
    let save = b.element(
        Some(actions),
        dom::Element {
            key: Some("pc-save".into()),
            attrs: vec![dom::Attr::Submits(true)],
            style: Some(dom::Style {
                background: Some(dom::ThemeColor::Primary),
                color: Some(dom::ThemeColor::Bg),
                ..blank_style()
            }),
            ..plain(dom::Tag::Button)
        },
    );
    b.text(save, "Save");

    modals::open(&modals::Modal {
        id: MODAL_PRIMARY_CHANNEL.to_string(),
        title: "Primary channel".to_string(),
        size: modals::ModalSize::Sm,
        nodes: b.nodes,
    });
}

/// Handle an interaction with the prompt. Closes it on every outcome except
/// `Invalid`.
pub fn on_event(event: &WidgetEvent, account: &str) -> Option<Outcome> {
    match (&event.kind, &event.payload) {
        (EventKind::Click, _) if event.node_key == HANDLER_CANCEL => {
            modals::close(MODAL_PRIMARY_CHANNEL);
            Some(Outcome::Cancelled)
        }
        (EventKind::Click, _) if event.node_key == HANDLER_RESET => {
            modals::close(MODAL_PRIMARY_CHANNEL);
            Some(Outcome::Reset)
        }
        (EventKind::Submit, Payload::Fields(fields)) if event.node_key == HANDLER_SUBMIT => {
            let raw = fields
                .iter()
                .find(|(name, _)| name == "channel")
                .map(|(_, value)| value.as_str())
                .unwrap_or_default();
            match normalize(raw) {
                Ok(name) => {
                    modals::close(MODAL_PRIMARY_CHANNEL);
                    Some(Outcome::Set(name))
                }
                Err(message) => {
                    open(raw, account, Some(&message));
                    Some(Outcome::Invalid)
                }
            }
        }
        _ => None,
    }
}

fn text_style(color: dom::ThemeColor) -> dom::Element {
    dom::Element {
        style: Some(dom::Style {
            color: Some(color),
            font_size: Some(dom::TextSize::Sm),
            ..blank_style()
        }),
        ..plain(dom::Tag::P)
    }
}

fn click_button(key: &str, handler: &str) -> dom::Element {
    dom::Element {
        key: Some(key.into()),
        events: vec![dom::Binding {
            kind: dom::EventKind::Click,
            handler: handler.into(),
        }],
        ..plain(dom::Tag::Button)
    }
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn normalises_names() {
        assert_eq!(normalize("  #SnareChops ").as_deref(), Ok("snarechops"));
    }

    #[test]
    fn rejects_bad_names() {
        assert!(normalize("").is_err());
        assert!(normalize("abc").is_err());
        assert!(normalize("has space").is_err());
        assert!(normalize(&"a".repeat(26)).is_err());
    }
}
