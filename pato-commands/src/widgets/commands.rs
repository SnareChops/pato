//! The command manager widget: a list of commands with an enabled switch, a
//! modal form to create or edit one, a modal for per-platform prefixes, and a
//! core-drawn confirm prompt before deleting.
//!
//! Immediate mode like the other plugins' widgets: all UI state lives here,
//! every change rebuilds the whole tree. The command data itself lives in
//! `model::State`, owned by the plugin and passed in; handlers return whether
//! it changed so the caller can persist it. Commands a plugin registered are
//! listed too, but only their permissions, cooldowns, aliases and enabled
//! switch are editable, and they can't be deleted from here.

use crate::draft::{apply_prefixes, CommandDraft};
use crate::exports::pato::plugin::widget_events::{EventKind, Payload, WidgetEvent};
use crate::model::{Command, Kind, Owner, Role, State, MAX_NAME_LEN, MAX_RESPONSE_LEN};
use crate::pato::plugin::{modals, widget_dom as dom, widgets};

use super::builder::{blank_style, plain, tiles, Builder};

pub const WIDGET_ID: &str = "commands";
pub const MODAL_FORM: &str = "command-form";
pub const MODAL_PREFIXES: &str = "command-prefixes";
pub const MODAL_DELETE: &str = "command-delete";

const HANDLER_NEW: &str = "commands-new";
const HANDLER_MASTER: &str = "commands-master";
const HANDLER_PREFIXES: &str = "commands-prefixes";
const PREFIX_ENABLED: &str = "enabled:";
const PREFIX_EDIT: &str = "edit:";
const PREFIX_DELETE: &str = "delete:";
const HANDLER_FORM_SUBMIT: &str = "form-submit";
const HANDLER_FORM_CANCEL: &str = "form-cancel";
const HANDLER_PREFIXES_SUBMIT: &str = "prefixes-submit";
const HANDLER_PREFIXES_CANCEL: &str = "prefixes-cancel";

struct Form {
    /// Canonical name of the command being edited; `None` when creating.
    editing: Option<String>,
    /// The plugin that owns the command being edited, if any (locks the
    /// identity fields).
    locked_to: Option<String>,
    draft: CommandDraft,
    error: Option<String>,
}

struct PrefixForm {
    /// The fields as last submitted, so a rejected save keeps what was typed.
    fields: Vec<(String, String)>,
    error: Option<String>,
}

pub struct CommandsWidget {
    /// The last action failed; shown above the list.
    notice: Option<String>,
    form: Option<Form>,
    prefix_form: Option<PrefixForm>,
    /// The command a delete confirmation is currently asking about.
    pending_delete: Option<String>,
}

impl CommandsWidget {
    pub fn new(state: &State) -> Self {
        widgets::register(&widgets::WidgetSpec {
            id: WIDGET_ID.to_string(),
            size: tiles(10, 10),
            min: Some(tiles(5, 6)),
            max: Some(tiles(20, 20)),
        });
        let widget = CommandsWidget {
            notice: None,
            form: None,
            prefix_form: None,
            pending_delete: None,
        };
        widget.render(state);
        widget
    }

    // --- interactions -------------------------------------------------------

    /// Returns true when `state` changed and needs saving.
    pub fn on_event(&mut self, event: &WidgetEvent, state: &mut State) -> bool {
        match event.widget_id.as_str() {
            WIDGET_ID => self.on_list_event(event, state),
            MODAL_FORM => self.on_form_event(event, state),
            MODAL_PREFIXES => self.on_prefix_event(event, state),
            _ => false,
        }
    }

    fn on_list_event(&mut self, event: &WidgetEvent, state: &mut State) -> bool {
        let handler = event.node_key.as_str();
        match (&event.kind, &event.payload) {
            (EventKind::Click, _) if handler == HANDLER_NEW => self.open_form(None, state),
            (EventKind::Click, _) if handler == HANDLER_PREFIXES => self.open_prefix_form(state),
            (EventKind::Change, Payload::Toggled(on)) if handler == HANDLER_MASTER => {
                state.enabled = *on;
                self.render(state);
                return true;
            }
            (EventKind::Change, Payload::Toggled(on)) => {
                if let Some(name) = handler.strip_prefix(PREFIX_ENABLED) {
                    if let Some(command) = state.commands.iter_mut().find(|c| c.name == name) {
                        command.enabled = *on;
                        self.notice = None;
                        self.render(state);
                        return true;
                    }
                }
            }
            (EventKind::Click, _) => {
                if let Some(name) = handler.strip_prefix(PREFIX_EDIT) {
                    self.open_form(Some(name.to_string()), state);
                } else if let Some(name) = handler.strip_prefix(PREFIX_DELETE) {
                    self.ask_delete(name.to_string(), state);
                }
            }
            _ => {}
        }
        false
    }

    fn open_form(&mut self, editing: Option<String>, state: &State) {
        let (draft, locked_to) = match &editing {
            Some(name) => match state.commands.iter().find(|c| &c.name == name) {
                Some(c) => (
                    CommandDraft::from_command(c),
                    match &c.owner {
                        Owner::Plugin(p) => Some(p.clone()),
                        Owner::User => None,
                    },
                ),
                None => return,
            },
            None => (CommandDraft::default(), None),
        };
        self.form = Some(Form {
            editing,
            locked_to,
            draft,
            error: None,
        });
        self.render_form(state);
    }

    fn on_form_event(&mut self, event: &WidgetEvent, state: &mut State) -> bool {
        match (&event.kind, &event.payload) {
            (EventKind::Click, _) if event.node_key == HANDLER_FORM_CANCEL => {
                self.form = None;
                modals::close(MODAL_FORM);
                false
            }
            (EventKind::Submit, Payload::Fields(fields)) if event.node_key == HANDLER_FORM_SUBMIT => {
                self.submit_form(CommandDraft::from_fields(fields), state)
            }
            _ => false,
        }
    }

    fn submit_form(&mut self, draft: CommandDraft, state: &mut State) -> bool {
        let Some(form) = self.form.take() else {
            return false;
        };
        let existing = form
            .editing
            .as_ref()
            .and_then(|name| state.commands.iter().find(|c| &c.name == name))
            .cloned();
        let saved = draft.to_command(existing.as_ref()).and_then(|command| match &form.editing {
            Some(old) => state.replace(old, command),
            None => state.add(command),
        });
        match saved {
            Ok(()) => {
                self.notice = None;
                modals::close(MODAL_FORM);
                self.render(state);
                true
            }
            Err(error) => {
                self.form = Some(Form {
                    draft,
                    error: Some(error),
                    ..form
                });
                self.render_form(state);
                false
            }
        }
    }

    fn open_prefix_form(&mut self, state: &State) {
        let mut fields: Vec<(String, String)> = state
            .prefixes
            .iter()
            .map(|(platform, prefix)| (format!("prefix:{platform}"), prefix.clone()))
            .collect();
        fields.sort();
        self.prefix_form = Some(PrefixForm { fields, error: None });
        self.render_prefix_form();
    }

    fn on_prefix_event(&mut self, event: &WidgetEvent, state: &mut State) -> bool {
        match (&event.kind, &event.payload) {
            (EventKind::Click, _) if event.node_key == HANDLER_PREFIXES_CANCEL => {
                self.prefix_form = None;
                modals::close(MODAL_PREFIXES);
                false
            }
            (EventKind::Submit, Payload::Fields(fields)) if event.node_key == HANDLER_PREFIXES_SUBMIT => {
                match apply_prefixes(state, fields) {
                    Ok(next) => {
                        *state = next;
                        self.prefix_form = None;
                        self.notice = None;
                        modals::close(MODAL_PREFIXES);
                        self.render(state);
                        true
                    }
                    Err(error) => {
                        self.prefix_form = Some(PrefixForm {
                            fields: fields.clone(),
                            error: Some(error),
                        });
                        self.render_prefix_form();
                        false
                    }
                }
            }
            _ => false,
        }
    }

    fn ask_delete(&mut self, name: String, state: &State) {
        // Plugin-registered commands can't be deleted here; disable them.
        let Some(command) = state.commands.iter().find(|c| c.name == name && c.owner == Owner::User) else {
            return;
        };
        let opened = modals::confirm(&modals::ConfirmPrompt {
            id: MODAL_DELETE.to_string(),
            title: "Delete command?".to_string(),
            message: format!("\u{201c}{}\u{201d} will be permanently deleted.", command.name),
            confirm_label: "Delete".to_string(),
            tone: modals::ConfirmTone::Danger,
        });
        if opened {
            self.pending_delete = Some(name);
        } else {
            self.notice = Some("Couldn't open the confirmation.".into());
            self.render(state);
        }
    }

    pub fn on_confirm(&mut self, id: &str, confirmed: bool, state: &mut State) -> bool {
        if id != MODAL_DELETE {
            return false;
        }
        let Some(name) = self.pending_delete.take() else {
            return false;
        };
        if !confirmed {
            return false;
        }
        let removed = state.remove(&name);
        self.notice = None;
        self.render(state);
        removed
    }

    /// The user dismissed a modal without saving.
    pub fn on_modal_closed(&mut self, id: &str) {
        match id {
            MODAL_FORM => self.form = None,
            MODAL_PREFIXES => self.prefix_form = None,
            _ => {}
        }
    }

    // --- rendering ----------------------------------------------------------

    fn render(&self, state: &State) {
        let mut b = Builder::default();
        let root = b.element(
            None,
            dom::Element {
                style: Some(dom::Style {
                    layout: Some(dom::Layout::FlexCol),
                    gap: Some(dom::Space::Md),
                    padding: Some(dom::Space::Md),
                    color: Some(dom::ThemeColor::Fg),
                    ..blank_style()
                }),
                ..plain(dom::Tag::Div)
            },
        );

        let header = b.element(
            Some(root),
            dom::Element {
                style: Some(dom::Style {
                    layout: Some(dom::Layout::FlexRow),
                    gap: Some(dom::Space::Md),
                    align: Some(dom::Align::Center),
                    ..blank_style()
                }),
                ..plain(dom::Tag::Div)
            },
        );
        let heading = b.element(
            Some(header),
            dom::Element {
                style: Some(dom::Style {
                    grow: Some(true),
                    ..blank_style()
                }),
                ..plain(dom::Tag::H3)
            },
        );
        b.text(heading, "Chat commands");
        let master = b.element(Some(header), switch_label());
        b.element(Some(master), switch("master", None, state.enabled, Some(HANDLER_MASTER)));
        b.text(master, if state.enabled { "On" } else { "Paused" });
        let prefixes = b.element(Some(header), click_button("prefixes", HANDLER_PREFIXES));
        b.text(prefixes, "Prefixes");
        let new = b.element(Some(header), click_button("new", HANDLER_NEW));
        b.text(new, "New command");

        if let Some(notice) = &self.notice {
            let p = b.element(Some(root), message_style(dom::ThemeColor::Danger));
            b.text(p, notice);
        }

        if !state.enabled {
            let p = b.element(Some(root), message_style(dom::ThemeColor::Warning));
            b.text(p, "Commands are paused. Nothing in chat will trigger them until you switch this back on.");
        }

        if state.commands.is_empty() {
            let p = b.element(Some(root), message_style(dom::ThemeColor::FgMuted));
            b.text(p, "No commands yet. Create one with \u{201c}New command\u{201d}.");
        } else {
            let list = b.element(
                Some(root),
                dom::Element {
                    style: Some(dom::Style {
                        layout: Some(dom::Layout::FlexCol),
                        gap: Some(dom::Space::Sm),
                        grow: Some(true),
                        overflow: Some(dom::Overflow::Scroll),
                        ..blank_style()
                    }),
                    ..plain(dom::Tag::Div)
                },
            );
            let mut commands: Vec<&Command> = state.commands.iter().collect();
            commands.sort_by(|a, b| a.name.cmp(&b.name));
            for command in commands {
                render_row(&mut b, list, command);
            }
        }

        widgets::update(&widgets::Widget::Custom(widgets::CustomWidget {
            id: WIDGET_ID.to_string(),
            nodes: b.nodes,
        }));
    }

    /// (Re)open the command form from the current form state; opening an id
    /// that is already showing updates it in place.
    fn render_form(&mut self, state: &State) {
        let Some(form) = &self.form else {
            return;
        };
        let d = &form.draft;
        let mut b = Builder::default();
        let root = b.element(None, column(None));
        let f = b.element(
            Some(root),
            dom::Element {
                key: Some("form".into()),
                events: vec![dom::Binding {
                    kind: dom::EventKind::Submit,
                    handler: HANDLER_FORM_SUBMIT.into(),
                }],
                style: Some(dom::Style {
                    layout: Some(dom::Layout::FlexCol),
                    gap: Some(dom::Space::Lg),
                    ..blank_style()
                }),
                ..plain(dom::Tag::Form)
            },
        );

        if let Some(error) = &form.error {
            let p = b.element(Some(f), message_style(dom::ThemeColor::Danger));
            b.text(p, error);
        }

        match &form.locked_to {
            Some(owner) => {
                // Identity of a plugin's command is fixed.
                let p = b.element(Some(f), message_style(dom::ThemeColor::FgMuted));
                b.text(
                    p,
                    &format!(
                        "\u{201c}{}\u{201d} is provided by the {owner} plugin. You can change who may use it, its cooldowns and its aliases.",
                        d.name
                    ),
                );
            }
            None => {
                let field = b.element(Some(f), field_label());
                b.text(field, "Name (what chat types after the prefix)");
                b.element(
                    Some(field),
                    input(
                        "f-name",
                        "name",
                        dom::InputType::Text,
                        &d.name,
                        vec![dom::Attr::Required(true), dom::Attr::MaxLength(MAX_NAME_LEN as u32)],
                    ),
                );
            }
        }

        let field = b.element(Some(f), field_label());
        b.text(field, "Aliases (optional, separated by commas or spaces)");
        b.element(
            Some(field),
            input("f-aliases", "aliases", dom::InputType::Text, &d.aliases, vec![dom::Attr::MaxLength(200)]),
        );

        if form.locked_to.is_none() {
            let field = b.element(Some(f), field_label());
            b.text(field, "Arguments (optional), e.g. <user:string> [amount:number]");
            b.element(
                Some(field),
                input("f-args", "args", dom::InputType::Text, &d.args, vec![dom::Attr::MaxLength(200)]),
            );

            let field = b.element(Some(f), field_label());
            b.text(field, "Response. Use {user} and {argument name} to fill in values");
            b.element(
                Some(field),
                dom::Element {
                    key: Some("f-response".into()),
                    attrs: vec![
                        dom::Attr::Name("response".into()),
                        dom::Attr::Value(d.response.clone()),
                        dom::Attr::Rows(3),
                        dom::Attr::Required(true),
                        dom::Attr::MaxLength(MAX_RESPONSE_LEN as u32),
                    ],
                    ..plain(dom::Tag::Textarea)
                },
            );
        }

        // Who may use it: a radio group, since the role is a single choice.
        let group = b.element(Some(f), column(Some(dom::Space::Xs)));
        let label = b.element(Some(group), plain(dom::Tag::Span));
        b.text(label, "Who can use it");
        let options = b.element(
            Some(group),
            dom::Element {
                style: Some(dom::Style {
                    layout: Some(dom::Layout::FlexRow),
                    gap: Some(dom::Space::Lg),
                    wrap: Some(true),
                    ..blank_style()
                }),
                ..plain(dom::Tag::Div)
            },
        );
        for role in Role::ALL {
            let row = b.element(Some(options), switch_label());
            b.element(
                Some(row),
                dom::Element {
                    key: Some(format!("f-role-{}", role.as_str())),
                    attrs: vec![
                        dom::Attr::Kind(dom::InputType::Radio),
                        dom::Attr::Name("min_role".into()),
                        dom::Attr::Value(role.as_str().into()),
                        dom::Attr::Checked(d.min_role == role),
                    ],
                    ..plain(dom::Tag::Input)
                },
            );
            b.text(row, role.label());
        }

        // Cooldowns
        for (key, name, text, value) in [
            ("f-global-cd", "global_cooldown", "Cooldown for everyone (seconds)", &d.global_cooldown),
            ("f-user-cd", "user_cooldown", "Cooldown per person (seconds)", &d.user_cooldown),
        ] {
            let row = b.element(Some(f), switch_label());
            let label = b.element(
                Some(row),
                dom::Element {
                    style: Some(dom::Style {
                        grow: Some(true),
                        ..blank_style()
                    }),
                    ..plain(dom::Tag::Span)
                },
            );
            b.text(label, text);
            let mut number = input(key, name, dom::InputType::Number, value, vec![dom::Attr::Min(0.0), dom::Attr::Step(1.0)]);
            number.style = Some(dom::Style {
                width: Some(dom::Size::Percent(25)),
                ..blank_style()
            });
            b.element(Some(row), number);
        }

        for (key, name, on, text) in [
            ("f-bypass", "bypass_cooldown", d.bypass_cooldown, "Moderators and the streamer ignore cooldowns"),
            ("f-enabled", "enabled", d.enabled, "Enabled"),
        ] {
            let row = b.element(Some(f), switch_label());
            b.element(Some(row), switch(key, Some(name), on, None));
            b.text(row, text);
        }

        form_actions(&mut b, f, HANDLER_FORM_CANCEL, if form.editing.is_some() { "Save" } else { "Create" });

        let opened = modals::open(&modals::Modal {
            id: MODAL_FORM.to_string(),
            title: if form.editing.is_some() { "Edit command" } else { "New command" }.to_string(),
            size: modals::ModalSize::Md,
            nodes: b.nodes,
        });
        if !opened {
            self.form = None;
            self.notice = Some("Couldn't open the command form.".into());
            self.render(state);
        }
    }

    fn render_prefix_form(&mut self) {
        let Some(form) = &self.prefix_form else {
            return;
        };
        let get = |name: &str| {
            form.fields
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
                .unwrap_or("")
        };
        let mut b = Builder::default();
        let root = b.element(None, column(None));
        let f = b.element(
            Some(root),
            dom::Element {
                key: Some("form".into()),
                events: vec![dom::Binding {
                    kind: dom::EventKind::Submit,
                    handler: HANDLER_PREFIXES_SUBMIT.into(),
                }],
                style: Some(dom::Style {
                    layout: Some(dom::Layout::FlexCol),
                    gap: Some(dom::Space::Lg),
                    ..blank_style()
                }),
                ..plain(dom::Tag::Form)
            },
        );

        if let Some(error) = &form.error {
            let p = b.element(Some(f), message_style(dom::ThemeColor::Danger));
            b.text(p, error);
        }
        let p = b.element(Some(f), message_style(dom::ThemeColor::FgMuted));
        b.text(
            p,
            "The text a chat message starts with to be treated as a command on each platform. Clear a prefix to stop commands on that platform.",
        );

        let mut existing: Vec<&(String, String)> = form.fields.iter().filter(|(k, _)| k.starts_with("prefix:")).collect();
        existing.sort();
        for (key, value) in existing {
            let platform = key.trim_start_matches("prefix:");
            let field = b.element(Some(f), field_label());
            b.text(field, platform);
            b.element(
                Some(field),
                input(&format!("f-{key}"), key, dom::InputType::Text, value, vec![dom::Attr::MaxLength(8)]),
            );
        }

        let field = b.element(Some(f), field_label());
        b.text(field, "Add a platform (optional)");
        let row = b.element(
            Some(field),
            dom::Element {
                style: Some(dom::Style {
                    layout: Some(dom::Layout::FlexRow),
                    gap: Some(dom::Space::Md),
                    ..blank_style()
                }),
                ..plain(dom::Tag::Div)
            },
        );
        b.element(
            Some(row),
            input(
                "f-new-platform",
                "new_platform",
                dom::InputType::Text,
                get("new_platform"),
                vec![dom::Attr::Placeholder("platform, e.g. discord".into()), dom::Attr::MaxLength(32)],
            ),
        );
        b.element(
            Some(row),
            input(
                "f-new-prefix",
                "new_prefix",
                dom::InputType::Text,
                get("new_prefix"),
                vec![dom::Attr::Placeholder("prefix".into()), dom::Attr::MaxLength(8)],
            ),
        );

        form_actions(&mut b, f, HANDLER_PREFIXES_CANCEL, "Save");

        let opened = modals::open(&modals::Modal {
            id: MODAL_PREFIXES.to_string(),
            title: "Command prefixes".to_string(),
            size: modals::ModalSize::Sm,
            nodes: b.nodes,
        });
        if !opened {
            self.prefix_form = None;
            self.notice = Some("Couldn't open the prefix settings.".into());
        }
    }
}

fn render_row(b: &mut Builder, list: u32, command: &Command) {
    let name = &command.name;
    let row = b.element(
        Some(list),
        dom::Element {
            key: Some(format!("row:{name}")),
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexRow),
                gap: Some(dom::Space::Md),
                padding: Some(dom::Space::Md),
                align: Some(dom::Align::Center),
                wrap: Some(true),
                background: Some(dom::ThemeColor::Bg),
                radius: Some(dom::Space::Md),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );

    let info = b.element(
        Some(row),
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexCol),
                grow: Some(true),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );
    let title = b.element(Some(info), plain(dom::Tag::Strong));
    b.text(title, name);
    let detail = b.element(Some(info), muted_small());
    b.text(detail, &row_detail(command));
    if let Kind::Info { response } = &command.kind {
        let preview = b.element(Some(info), muted_small());
        b.text(preview, &truncate(response, 100));
    }

    let controls = b.element(
        Some(row),
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexRow),
                gap: Some(dom::Space::Md),
                align: Some(dom::Align::Center),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );
    let field = b.element(Some(controls), switch_label());
    b.element(
        Some(field),
        switch(&format!("enabled:{name}"), None, command.enabled, Some(&format!("{PREFIX_ENABLED}{name}"))),
    );
    b.text(field, "Enabled");
    let edit = b.element(Some(controls), click_button(&format!("edit:{name}"), &format!("{PREFIX_EDIT}{name}")));
    b.text(edit, "Edit");
    if command.owner == Owner::User {
        let mut del = click_button(&format!("delete:{name}"), &format!("{PREFIX_DELETE}{name}"));
        del.style = Some(dom::Style {
            color: Some(dom::ThemeColor::Danger),
            ..blank_style()
        });
        let del = b.element(Some(controls), del);
        b.text(del, "Delete");
    }
}

/// "Info · Moderators+ · 5s cooldown · <who:string> · aka: h, hugs" summary.
fn row_detail(c: &Command) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !c.enabled {
        parts.push("Disabled".into());
    }
    parts.push(match &c.owner {
        Owner::User => "Reply".to_string(),
        Owner::Plugin(p) => format!("Plugin: {p}"),
    });
    parts.push(c.min_role.label().to_string());
    match (c.global_cooldown_secs, c.user_cooldown_secs) {
        (0, 0) => {}
        (g, 0) => parts.push(format!("{g}s cooldown")),
        (0, u) => parts.push(format!("{u}s per-person cooldown")),
        (g, u) => parts.push(format!("{g}s cooldown, {u}s per person")),
    }
    if !c.args.is_empty() {
        parts.push(c.args.clone());
    }
    if !c.aliases.is_empty() {
        parts.push(format!("aka {}", c.aliases.join(", ")));
    }
    parts.join(" \u{b7} ")
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('\u{2026}');
    out
}

// --- small element helpers -------------------------------------------------

fn column(gap: Option<dom::Space>) -> dom::Element {
    dom::Element {
        style: Some(dom::Style {
            layout: Some(dom::Layout::FlexCol),
            gap,
            ..blank_style()
        }),
        ..plain(dom::Tag::Div)
    }
}

fn muted_small() -> dom::Element {
    dom::Element {
        style: Some(dom::Style {
            color: Some(dom::ThemeColor::FgMuted),
            font_size: Some(dom::TextSize::Sm),
            ..blank_style()
        }),
        ..plain(dom::Tag::Small)
    }
}

fn message_style(color: dom::ThemeColor) -> dom::Element {
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
        key: Some(key.to_string()),
        events: vec![dom::Binding {
            kind: dom::EventKind::Click,
            handler: handler.to_string(),
        }],
        ..plain(dom::Tag::Button)
    }
}

/// Cancel + submit buttons, right-aligned.
fn form_actions(b: &mut Builder, form: u32, cancel_handler: &str, submit_label: &str) {
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
    let cancel = b.element(Some(actions), click_button("f-cancel", cancel_handler));
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
    b.text(save, submit_label);
}

/// A row that pairs a control with its text; clicking the text toggles it.
fn switch_label() -> dom::Element {
    dom::Element {
        style: Some(dom::Style {
            layout: Some(dom::Layout::FlexRow),
            gap: Some(dom::Space::Sm),
            align: Some(dom::Align::Center),
            ..blank_style()
        }),
        ..plain(dom::Tag::Label)
    }
}

/// A label stacked above its text control.
fn field_label() -> dom::Element {
    dom::Element {
        style: Some(dom::Style {
            layout: Some(dom::Layout::FlexCol),
            gap: Some(dom::Space::Xs),
            ..blank_style()
        }),
        ..plain(dom::Tag::Label)
    }
}

fn switch(key: &str, name: Option<&str>, checked: bool, on_change: Option<&str>) -> dom::Element {
    let mut attrs = vec![dom::Attr::Kind(dom::InputType::Switch), dom::Attr::Checked(checked)];
    if let Some(name) = name {
        attrs.push(dom::Attr::Name(name.to_string()));
    }
    dom::Element {
        key: Some(key.to_string()),
        attrs,
        events: on_change
            .map(|handler| {
                vec![dom::Binding {
                    kind: dom::EventKind::Change,
                    handler: handler.to_string(),
                }]
            })
            .unwrap_or_default(),
        ..plain(dom::Tag::Input)
    }
}

fn input(key: &str, name: &str, kind: dom::InputType, value: &str, extra: Vec<dom::Attr>) -> dom::Element {
    let mut attrs = vec![
        dom::Attr::Kind(kind),
        dom::Attr::Name(name.to_string()),
        dom::Attr::Value(value.to_string()),
    ];
    attrs.extend(extra);
    dom::Element {
        key: Some(key.to_string()),
        attrs,
        ..plain(dom::Tag::Input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{info, trigger};

    #[test]
    fn row_detail_summarises_a_reply_command() {
        let mut c = info("hug", "hi");
        c.min_role = Role::Moderator;
        c.global_cooldown_secs = 5;
        c.user_cooldown_secs = 30;
        c.args = "<who:string>".into();
        c.aliases = vec!["h".into(), "hugs".into()];
        assert_eq!(
            row_detail(&c),
            "Reply \u{b7} Moderators+ \u{b7} 5s cooldown, 30s per person \u{b7} <who:string> \u{b7} aka h, hugs"
        );
    }

    #[test]
    fn row_detail_flags_disabled_and_plugin_commands() {
        let mut c = trigger("scene", "pato_obs");
        c.enabled = false;
        c.user_cooldown_secs = 10;
        assert_eq!(
            row_detail(&c),
            "Disabled \u{b7} Plugin: pato_obs \u{b7} Anyone \u{b7} 10s per-person cooldown"
        );
    }

    #[test]
    fn truncate_is_char_safe() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("ééééé", 3), "ééé\u{2026}");
    }
}
