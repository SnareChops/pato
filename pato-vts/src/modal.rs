//! The save/edit position form. One modal for both: saving a new position
//! asks for a name and move duration; editing an existing one shows the same
//! fields plus a Delete button.

use crate::pato::plugin::{modals, widget_dom as dom};
use crate::positions::{DEFAULT_DURATION, MAX_DURATION, MAX_NAME_LEN};
use crate::widgets::{blank_style, plain, Builder};

pub const MODAL_ID: &str = "vts-position";
pub const CONFIRM_DELETE_ID: &str = "vts-position-delete";
pub const HANDLER_SUBMIT: &str = "pos-submit";
pub const HANDLER_CANCEL: &str = "pos-cancel";
pub const HANDLER_DELETE: &str = "pos-delete";

pub struct Submitted {
    pub name: String,
    pub duration: String,
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
        Submitted { name: get("name"), duration: get("duration") }
    }

    /// `(name, duration seconds)`, or what to tell the user is wrong.
    pub fn validate(&self) -> Result<(String, f64), String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Enter a name.".to_string());
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(format!("Names are at most {MAX_NAME_LEN} characters."));
        }
        let duration = self
            .duration
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|d| (0.0..=MAX_DURATION).contains(d))
            .ok_or_else(|| format!("Duration must be a number from 0 to {MAX_DURATION} seconds."))?;
        Ok((name.to_string(), duration))
    }
}

/// (Re)opens the form. `editing` adds the Delete button and the "Edit"
/// title; `error` shows a problem with the previous submit.
pub fn open(editing: bool, name: &str, duration: &str, error: Option<&str>) -> bool {
    let mut b = Builder::default();
    let root = b.element(
        None,
        dom::Element {
            style: Some(dom::Style { layout: Some(dom::Layout::FlexCol), ..blank_style() }),
            ..plain(dom::Tag::Div)
        },
    );
    let form = b.element(
        Some(root),
        dom::Element {
            key: Some("form".into()),
            events: vec![dom::Binding { kind: dom::EventKind::Submit, handler: HANDLER_SUBMIT.into() }],
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

    field(
        &mut b,
        form,
        "f-name",
        "name",
        "Name",
        dom::InputType::Text,
        name,
        vec![dom::Attr::Required(true), dom::Attr::MaxLength(MAX_NAME_LEN as u32)],
    );
    field(
        &mut b,
        form,
        "f-duration",
        "duration",
        "Move duration (seconds)",
        dom::InputType::Number,
        duration,
        vec![dom::Attr::Required(true), dom::Attr::Min(0.0), dom::Attr::Max(MAX_DURATION), dom::Attr::Step(0.05)],
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
    if editing {
        let delete = b.element(
            Some(actions),
            dom::Element {
                key: Some("f-delete".into()),
                events: vec![dom::Binding { kind: dom::EventKind::Click, handler: HANDLER_DELETE.into() }],
                style: Some(dom::Style { color: Some(dom::ThemeColor::Danger), ..blank_style() }),
                ..plain(dom::Tag::Button)
            },
        );
        b.text(delete, "Delete");
    }
    let cancel = b.element(
        Some(actions),
        dom::Element {
            key: Some("f-cancel".into()),
            events: vec![dom::Binding { kind: dom::EventKind::Click, handler: HANDLER_CANCEL.into() }],
            ..plain(dom::Tag::Button)
        },
    );
    b.text(cancel, "Cancel");
    let save = b.element(
        Some(actions),
        dom::Element {
            key: Some("f-save".into()),
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
        id: MODAL_ID.to_string(),
        title: if editing { "Edit position" } else { "Save position" }.to_string(),
        size: modals::ModalSize::Sm,
        nodes: b.nodes,
    })
}

pub fn default_duration() -> String {
    DEFAULT_DURATION.to_string()
}

pub fn close() {
    modals::close(MODAL_ID);
}

pub fn confirm_delete(name: &str) -> bool {
    modals::confirm(&modals::ConfirmPrompt {
        id: CONFIRM_DELETE_ID.to_string(),
        title: "Delete position".to_string(),
        message: format!("Delete the position \"{name}\"?"),
        confirm_label: "Delete".to_string(),
        tone: modals::ConfirmTone::Danger,
    })
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
        dom::Attr::Value(value.to_string()),
    ];
    attrs.append(&mut extra);
    b.element(
        Some(field),
        dom::Element { key: Some(key.to_string()), attrs, ..plain(dom::Tag::Input) },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn submitted(name: &str, duration: &str) -> Submitted {
        Submitted { name: name.into(), duration: duration.into() }
    }

    #[test]
    fn accepts_a_trimmed_name_and_valid_duration() {
        assert_eq!(submitted("  Gaming ", "0.5").validate(), Ok(("Gaming".to_string(), 0.5)));
        assert_eq!(submitted("x", "0").validate(), Ok(("x".to_string(), 0.0)));
    }

    #[test]
    fn rejects_blank_name_and_bad_durations() {
        assert!(submitted("  ", "0.5").validate().is_err());
        assert!(submitted("a", "").validate().is_err());
        assert!(submitted("a", "abc").validate().is_err());
        assert!(submitted("a", "2.5").validate().is_err());
        assert!(submitted("a", "-1").validate().is_err());
    }

    #[test]
    fn rejects_overlong_names() {
        assert!(submitted(&"a".repeat(MAX_NAME_LEN + 1), "1").validate().is_err());
    }
}
