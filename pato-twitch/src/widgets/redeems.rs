//! Channel point redeem manager: a list of the channel's custom rewards with
//! quick enabled / paused switches, plus a modal form to create or edit one
//! and a core-drawn confirm prompt before deleting.
//!
//! Immediate mode like the other widgets: all state lives here, and every
//! change rebuilds the whole tree. Rewards this app didn't create show in the
//! list but are read-only, because Helix only lets an app change its own.

use crate::exports::pato::plugin::widget_events::{EventKind, Payload, WidgetEvent};
use crate::helix::{Reward, RewardInput, TwitchApi, TwitchUser};
use crate::pato::plugin::{modals, widget_dom as dom, widgets};

use super::builder::{blank_style, plain, tiles, Builder};

pub const WIDGET_ID: &str = "redeems";
pub const MODAL_FORM: &str = "redeem-form";
pub const MODAL_DELETE: &str = "redeem-delete";

const HANDLER_NEW: &str = "redeems-new";
const HANDLER_REFRESH: &str = "redeems-refresh";
const PREFIX_ENABLED: &str = "enabled:";
const PREFIX_PAUSED: &str = "paused:";
const PREFIX_EDIT: &str = "edit:";
const PREFIX_DELETE: &str = "delete:";
const HANDLER_FORM_SUBMIT: &str = "form-submit";
const HANDLER_FORM_CANCEL: &str = "form-cancel";

// Twitch's own limits, checked up front so the user gets a specific message.
const MAX_TITLE: usize = 45;
const MAX_PROMPT: usize = 200;

/// What the form's controls hold. Kept as the raw strings the user typed, so a
/// failed save re-renders exactly what they entered.
#[derive(Default, Clone, Debug, PartialEq)]
struct Draft {
    title: String,
    cost: String,
    prompt: String,
    user_input: bool,
    skip_queue: bool,
    max_stream_on: bool,
    max_stream: String,
    max_user_on: bool,
    max_user: String,
    cooldown_on: bool,
    cooldown: String,
}

impl Draft {
    fn from_reward(r: &Reward) -> Self {
        let num = |on: bool, n: u32| if on { n.to_string() } else { String::new() };
        Draft {
            title: r.title.clone(),
            cost: r.cost.to_string(),
            prompt: r.prompt.clone(),
            user_input: r.is_user_input_required,
            skip_queue: r.should_redemptions_skip_request_queue,
            max_stream_on: r.max_per_stream_setting.is_enabled,
            max_stream: num(
                r.max_per_stream_setting.is_enabled,
                r.max_per_stream_setting.max_per_stream,
            ),
            max_user_on: r.max_per_user_per_stream_setting.is_enabled,
            max_user: num(
                r.max_per_user_per_stream_setting.is_enabled,
                r.max_per_user_per_stream_setting.max_per_user_per_stream,
            ),
            cooldown_on: r.global_cooldown_setting.is_enabled,
            cooldown: num(
                r.global_cooldown_setting.is_enabled,
                r.global_cooldown_setting.global_cooldown_seconds,
            ),
        }
    }

    /// Read the submitted form fields by name.
    fn from_fields(fields: &[(String, String)]) -> Self {
        let get = |name: &str| {
            fields
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
                .unwrap_or("")
        };
        let flag = |name: &str| get(name) == "true";
        Draft {
            title: get("title").to_string(),
            cost: get("cost").to_string(),
            prompt: get("prompt").to_string(),
            user_input: flag("user_input"),
            skip_queue: flag("skip_queue"),
            max_stream_on: flag("max_stream_on"),
            max_stream: get("max_stream").to_string(),
            max_user_on: flag("max_user_on"),
            max_user: get("max_user").to_string(),
            cooldown_on: flag("cooldown_on"),
            cooldown: get("cooldown").to_string(),
        }
    }

    /// Validate into the request body Helix wants. A disabled limit is sent as
    /// disabled with no number, whatever is still typed in its box.
    fn to_input(&self) -> Result<RewardInput, String> {
        let title = self.title.trim();
        if title.is_empty() {
            return Err("Give the redeem a title.".into());
        }
        if title.chars().count() > MAX_TITLE {
            return Err(format!("The title can be at most {MAX_TITLE} characters."));
        }
        let cost = whole_number(&self.cost, "Cost")?;
        let prompt = self.prompt.trim();
        if prompt.chars().count() > MAX_PROMPT {
            return Err(format!("The prompt can be at most {MAX_PROMPT} characters."));
        }
        if self.user_input && prompt.is_empty() {
            return Err("Add a prompt telling viewers what to enter.".into());
        }
        let limit = |on: bool, raw: &str, what: &str| -> Result<Option<u32>, String> {
            if on {
                whole_number(raw, what).map(Some)
            } else {
                Ok(None)
            }
        };
        let max_stream = limit(self.max_stream_on, &self.max_stream, "Limit per stream")?;
        let max_user = limit(self.max_user_on, &self.max_user, "Limit per viewer")?;
        let cooldown = limit(self.cooldown_on, &self.cooldown, "Cooldown")?;
        Ok(RewardInput {
            title: Some(title.to_string()),
            cost: Some(cost),
            prompt: Some(prompt.to_string()),
            is_user_input_required: Some(self.user_input),
            should_redemptions_skip_request_queue: Some(self.skip_queue),
            is_max_per_stream_enabled: Some(self.max_stream_on),
            max_per_stream: max_stream,
            is_max_per_user_per_stream_enabled: Some(self.max_user_on),
            max_per_user_per_stream: max_user,
            is_global_cooldown_enabled: Some(self.cooldown_on),
            global_cooldown_seconds: cooldown,
            ..Default::default()
        })
    }
}

fn whole_number(raw: &str, what: &str) -> Result<u32, String> {
    match raw.trim().parse::<u32>() {
        Ok(n) if n >= 1 => Ok(n),
        _ => Err(format!("{what} must be a whole number of at least 1.")),
    }
}

struct Form {
    /// The reward being edited; `None` when creating a new one.
    editing: Option<String>,
    draft: Draft,
    error: Option<String>,
}

pub struct TwitchRedeemsWidget {
    signed_in: bool,
    /// Signed in, but not as the primary channel's own account, so redeems
    /// can't be managed; shown in place of the list.
    conflict: Option<String>,
    rewards: Vec<Reward>,
    /// The list couldn't be loaded; shown in place of the list.
    load_error: Option<String>,
    /// The last action (toggle, save, delete) failed; shown above the list.
    notice: Option<String>,
    form: Option<Form>,
    /// The reward a delete confirmation is currently asking about.
    pending_delete: Option<String>,
}

impl TwitchRedeemsWidget {
    pub fn new() -> Self {
        widgets::register(&widgets::WidgetSpec {
            id: WIDGET_ID.to_string(),
            size: tiles(10, 10),
            min: Some(tiles(5, 6)),
            max: Some(tiles(20, 20)),
        });
        let widget = TwitchRedeemsWidget {
            signed_in: false,
            conflict: None,
            rewards: vec![],
            load_error: None,
            notice: None,
            form: None,
            pending_delete: None,
        };
        widget.render();
        widget
    }

    /// The Twitch connection or primary channel changed. Signing in loads the
    /// list; signing out - or a `conflict` (the signed-in account doesn't own
    /// the primary channel, which Twitch requires for managing rewards) -
    /// clears it and dismisses anything still open on top of it.
    pub fn set_connection(
        &mut self,
        api: Option<&TwitchApi>,
        user: Option<&TwitchUser>,
        conflict: Option<String>,
    ) {
        self.notice = None;
        self.conflict = conflict;
        match (api, user) {
            (Some(api), Some(user)) if self.conflict.is_none() => {
                self.signed_in = true;
                self.load(api, user);
            }
            _ => {
                self.signed_in = false;
                self.rewards.clear();
                self.load_error = None;
                self.form = None;
                self.pending_delete = None;
                modals::close(MODAL_FORM);
                modals::close(MODAL_DELETE);
            }
        }
        self.render();
    }

    fn load(&mut self, api: &TwitchApi, user: &TwitchUser) {
        match api.list_rewards(&user.id) {
            Ok(mut rewards) => {
                rewards.sort_by_key(|r| r.title.to_lowercase());
                self.rewards = rewards;
                self.load_error = None;
            }
            Err(e) => {
                self.rewards.clear();
                self.load_error = Some(e);
            }
        }
    }

    // --- interactions -------------------------------------------------------

    pub fn on_event(&mut self, event: &WidgetEvent, api: Option<&TwitchApi>, user: Option<&TwitchUser>) {
        let (Some(api), Some(user)) = (api, user) else {
            return;
        };
        match event.widget_id.as_str() {
            WIDGET_ID => self.on_list_event(event, api, user),
            MODAL_FORM => self.on_form_event(event, api, user),
            _ => {}
        }
    }

    fn on_list_event(&mut self, event: &WidgetEvent, api: &TwitchApi, user: &TwitchUser) {
        let handler = event.node_key.as_str();
        match (&event.kind, &event.payload) {
            (EventKind::Click, _) if handler == HANDLER_NEW => self.open_form(None),
            (EventKind::Click, _) if handler == HANDLER_REFRESH => {
                self.notice = None;
                self.load(api, user);
                self.render();
            }
            (EventKind::Change, Payload::Toggled(on)) => {
                if let Some(id) = handler.strip_prefix(PREFIX_ENABLED) {
                    let input = RewardInput {
                        is_enabled: Some(*on),
                        ..Default::default()
                    };
                    self.patch(api, user, id, input);
                } else if let Some(id) = handler.strip_prefix(PREFIX_PAUSED) {
                    let input = RewardInput {
                        is_paused: Some(*on),
                        ..Default::default()
                    };
                    self.patch(api, user, id, input);
                }
            }
            (EventKind::Click, _) => {
                if let Some(id) = handler.strip_prefix(PREFIX_EDIT) {
                    self.open_form(Some(id.to_string()));
                } else if let Some(id) = handler.strip_prefix(PREFIX_DELETE) {
                    self.ask_delete(id.to_string());
                }
            }
            _ => {}
        }
    }

    /// A quick switch: change one field and swap the returned reward in. On
    /// failure the list re-renders from unchanged state, which puts the switch
    /// back where it was.
    fn patch(&mut self, api: &TwitchApi, user: &TwitchUser, id: &str, input: RewardInput) {
        match api.update_reward(&user.id, id, &input) {
            Ok(updated) => {
                self.notice = None;
                self.replace(updated);
            }
            Err(e) => self.notice = Some(e),
        }
        self.render();
    }

    fn replace(&mut self, updated: Reward) {
        match self.rewards.iter_mut().find(|r| r.id == updated.id) {
            Some(slot) => *slot = updated,
            None => {
                self.rewards.push(updated);
                self.rewards.sort_by_key(|r| r.title.to_lowercase());
            }
        }
    }

    fn open_form(&mut self, editing: Option<String>) {
        let draft = match &editing {
            Some(id) => match self.rewards.iter().find(|r| &r.id == id && r.manageable) {
                Some(reward) => Draft::from_reward(reward),
                None => return,
            },
            None => Draft::default(),
        };
        self.form = Some(Form {
            editing,
            draft,
            error: None,
        });
        self.render_form();
    }

    fn on_form_event(&mut self, event: &WidgetEvent, api: &TwitchApi, user: &TwitchUser) {
        match (&event.kind, &event.payload) {
            (EventKind::Click, _) if event.node_key == HANDLER_FORM_CANCEL => {
                self.form = None;
                modals::close(MODAL_FORM);
            }
            (EventKind::Submit, Payload::Fields(fields)) if event.node_key == HANDLER_FORM_SUBMIT => {
                self.submit_form(Draft::from_fields(fields), api, user);
            }
            _ => {}
        }
    }

    fn submit_form(&mut self, draft: Draft, api: &TwitchApi, user: &TwitchUser) {
        let Some(editing) = self.form.as_ref().map(|f| f.editing.clone()) else {
            return;
        };
        let saved = draft.to_input().and_then(|input| match &editing {
            Some(id) => api.update_reward(&user.id, id, &input),
            None => api.create_reward(&user.id, &input),
        });
        match saved {
            Ok(reward) => {
                self.replace(reward);
                self.form = None;
                self.notice = None;
                modals::close(MODAL_FORM);
                self.render();
            }
            Err(e) => {
                self.form = Some(Form {
                    editing,
                    draft,
                    error: Some(e),
                });
                self.render_form();
            }
        }
    }

    fn ask_delete(&mut self, id: String) {
        let Some(reward) = self.rewards.iter().find(|r| r.id == id && r.manageable) else {
            return;
        };
        let opened = modals::confirm(&modals::ConfirmPrompt {
            id: MODAL_DELETE.to_string(),
            title: "Delete redeem?".to_string(),
            message: format!(
                "\u{201c}{}\u{201d} will be permanently deleted from your channel. This can't be undone.",
                reward.title
            ),
            confirm_label: "Delete".to_string(),
            tone: modals::ConfirmTone::Danger,
        });
        if opened {
            self.pending_delete = Some(id);
        } else {
            self.notice = Some("Couldn't open the confirmation.".into());
            self.render();
        }
    }

    pub fn on_confirm(&mut self, id: &str, confirmed: bool, api: Option<&TwitchApi>, user: Option<&TwitchUser>) {
        if id != MODAL_DELETE {
            return;
        }
        let Some(reward_id) = self.pending_delete.take() else {
            return;
        };
        let (true, Some(api), Some(user)) = (confirmed, api, user) else {
            return;
        };
        match api.delete_reward(&user.id, &reward_id) {
            Ok(()) => {
                self.notice = None;
                self.rewards.retain(|r| r.id != reward_id);
            }
            Err(e) => self.notice = Some(e),
        }
        self.render();
    }

    /// The user dismissed the form without saving.
    pub fn on_modal_closed(&mut self, id: &str) {
        if id == MODAL_FORM {
            self.form = None;
        }
    }

    // --- rendering ----------------------------------------------------------

    fn render(&self) {
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
        b.element(Some(header), super::platform_badge());
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
        b.text(heading, "Channel point redeems");
        if self.signed_in {
            let refresh = b.element(Some(header), click_button("refresh", HANDLER_REFRESH));
            b.text(refresh, "Refresh");
            let new = b.element(Some(header), click_button("new", HANDLER_NEW));
            b.text(new, "New redeem");
        }

        if let Some(notice) = &self.notice {
            let p = b.element(Some(root), message_style(dom::ThemeColor::Danger));
            b.text(p, notice);
        }

        if let Some(conflict) = &self.conflict {
            let p = b.element(Some(root), message_style(dom::ThemeColor::Warning));
            b.text(p, conflict);
        } else if !self.signed_in {
            let p = b.element(Some(root), message_style(dom::ThemeColor::FgMuted));
            b.text(p, "Connect Twitch from the status widget to manage your channel point redeems.");
        } else if let Some(error) = &self.load_error {
            let p = b.element(Some(root), message_style(dom::ThemeColor::Danger));
            b.text(p, error);
        } else if self.rewards.is_empty() {
            let p = b.element(Some(root), message_style(dom::ThemeColor::FgMuted));
            b.text(p, "No redeems yet. Create one with \u{201c}New redeem\u{201d}.");
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
            for reward in &self.rewards {
                self.render_row(&mut b, list, reward);
            }
        }

        widgets::update(&widgets::Widget::Custom(widgets::CustomWidget {
            id: WIDGET_ID.to_string(),
            nodes: b.nodes,
        }));
    }

    fn render_row(&self, b: &mut Builder, list: u32, reward: &Reward) {
        let id = &reward.id;
        let row = b.element(
            Some(list),
            dom::Element {
                key: Some(format!("row:{id}")),
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
        b.text(title, &reward.title);
        let detail = b.element(
            Some(info),
            dom::Element {
                style: Some(dom::Style {
                    color: Some(dom::ThemeColor::FgMuted),
                    font_size: Some(dom::TextSize::Sm),
                    ..blank_style()
                }),
                ..plain(dom::Tag::Small)
            },
        );
        b.text(detail, &row_detail(reward));

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
        let locked = !reward.manageable;
        if !locked {
            for (label, checked, handler, key) in [
                ("Enabled", reward.is_enabled, format!("{PREFIX_ENABLED}{id}"), format!("enabled:{id}")),
                ("Paused", reward.is_paused, format!("{PREFIX_PAUSED}{id}"), format!("paused:{id}")),
            ] {
                let field = b.element(Some(controls), switch_label());
                b.element(Some(field), switch(&key, None, checked, Some(&handler)));
                b.text(field, label);
            }
        }
        if !locked {
            let edit = b.element(Some(controls), click_button(&format!("edit:{id}"), &format!("{PREFIX_EDIT}{id}")));
            b.text(edit, "Edit");
            let mut del = click_button(&format!("delete:{id}"), &format!("{PREFIX_DELETE}{id}"));
            del.style = Some(dom::Style {
                color: Some(dom::ThemeColor::Danger),
                ..blank_style()
            });
            let del = b.element(Some(controls), del);
            b.text(del, "Delete");
        }
    }

    /// (Re)open the form modal from the current form state; opening an id
    /// that is already showing updates it in place.
    fn render_form(&mut self) {
        let Some(form) = &self.form else {
            return;
        };
        let d = &form.draft;
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

        // Title
        let field = b.element(Some(f), field_label());
        b.text(field, "Title");
        b.element(
            Some(field),
            input(
                "f-title",
                "title",
                dom::InputType::Text,
                &d.title,
                vec![dom::Attr::Required(true), dom::Attr::MaxLength(MAX_TITLE as u32)],
            ),
        );
        // Cost
        let field = b.element(Some(f), field_label());
        b.text(field, "Cost (channel points)");
        b.element(
            Some(field),
            input(
                "f-cost",
                "cost",
                dom::InputType::Number,
                &d.cost,
                vec![dom::Attr::Required(true), dom::Attr::Min(1.0), dom::Attr::Step(1.0)],
            ),
        );
        // Prompt
        let field = b.element(Some(f), field_label());
        b.text(field, "Prompt shown to viewers");
        b.element(
            Some(field),
            dom::Element {
                key: Some("f-prompt".into()),
                attrs: vec![
                    dom::Attr::Name("prompt".into()),
                    dom::Attr::Value(d.prompt.clone()),
                    dom::Attr::Rows(3),
                    dom::Attr::MaxLength(MAX_PROMPT as u32),
                ],
                ..plain(dom::Tag::Textarea)
            },
        );

        // Behaviour switches
        for (key, name, on, text) in [
            ("f-user-input", "user_input", d.user_input, "Viewers must enter text when redeeming"),
            ("f-skip-queue", "skip_queue", d.skip_queue, "Skip the redemption queue (mark completed automatically)"),
        ] {
            let row = b.element(Some(f), switch_label());
            b.element(Some(row), switch(key, Some(name), on, None));
            b.text(row, text);
        }

        // Limits: a switch and the number it turns on
        for (switch_key, switch_name, on, text, num_key, num_name, value) in [
            ("f-max-stream-on", "max_stream_on", d.max_stream_on, "Limit redemptions per stream", "f-max-stream", "max_stream", &d.max_stream),
            ("f-max-user-on", "max_user_on", d.max_user_on, "Limit redemptions per viewer per stream", "f-max-user", "max_user", &d.max_user),
            ("f-cooldown-on", "cooldown_on", d.cooldown_on, "Cooldown between redemptions (seconds)", "f-cooldown", "cooldown", &d.cooldown),
        ] {
            let row = b.element(Some(f), switch_label());
            b.element(Some(row), switch(switch_key, Some(switch_name), on, None));
            // The text takes the slack so the number box sits at the right edge.
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
            let mut number = input(num_key, num_name, dom::InputType::Number, value, vec![dom::Attr::Min(1.0), dom::Attr::Step(1.0)]);
            number.style = Some(dom::Style {
                width: Some(dom::Size::Percent(25)),
                ..blank_style()
            });
            b.element(Some(row), number);
        }

        // Actions
        let actions = b.element(
            Some(f),
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
        let cancel = b.element(Some(actions), click_button("f-cancel", HANDLER_FORM_CANCEL));
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
        b.text(save, if form.editing.is_some() { "Save" } else { "Create" });

        let opened = modals::open(&modals::Modal {
            id: MODAL_FORM.to_string(),
            title: if form.editing.is_some() { "Edit redeem" } else { "New redeem" }.to_string(),
            size: modals::ModalSize::Md,
            nodes: b.nodes,
        });
        if !opened {
            self.form = None;
            self.notice = Some("Couldn't open the redeem form.".into());
            self.render();
        }
    }
}

/// "500 pts · Paused" style summary line under a reward's title.
fn row_detail(r: &Reward) -> String {
    let mut parts = vec![format!("{} pts", r.cost)];
    if !r.is_enabled {
        parts.push("Disabled".into());
    } else if r.is_paused {
        parts.push("Paused".into());
    }
    if !r.manageable {
        parts.push("Read-only (created outside Pato)".into());
    }
    parts.join(" \u{b7} ")
}

// --- small element helpers -------------------------------------------------

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
    let mut attrs = vec![
        dom::Attr::Kind(dom::InputType::Switch),
        dom::Attr::Checked(checked),
    ];
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

    fn valid() -> Draft {
        Draft {
            title: "  Hydrate  ".into(),
            cost: "500".into(),
            ..Default::default()
        }
    }

    #[test]
    fn minimal_draft_becomes_a_request() {
        let input = valid().to_input().unwrap();
        assert_eq!(input.title.as_deref(), Some("Hydrate"));
        assert_eq!(input.cost, Some(500));
        assert_eq!(input.is_max_per_stream_enabled, Some(false));
        assert_eq!(input.max_per_stream, None);
    }

    #[test]
    fn rejects_bad_input_with_a_specific_message() {
        let mut d = valid();
        d.title = "   ".into();
        assert!(d.to_input().unwrap_err().contains("title"));

        let mut d = valid();
        d.title = "x".repeat(MAX_TITLE + 1);
        assert!(d.to_input().unwrap_err().contains("45"));

        let mut d = valid();
        d.cost = "0".into();
        assert!(d.to_input().unwrap_err().contains("Cost"));
        d.cost = "abc".into();
        assert!(d.to_input().unwrap_err().contains("Cost"));

        let mut d = valid();
        d.user_input = true;
        assert!(d.to_input().unwrap_err().contains("prompt"));
    }

    #[test]
    fn limits_only_count_when_switched_on() {
        let mut d = valid();
        d.max_stream = "junk".into(); // switched off: ignored
        assert!(d.to_input().is_ok());
        d.max_stream_on = true;
        assert!(d.to_input().unwrap_err().contains("Limit per stream"));
        d.max_stream = "10".into();
        assert_eq!(d.to_input().unwrap().max_per_stream, Some(10));
    }

    #[test]
    fn reads_form_fields_by_name() {
        let fields: Vec<(String, String)> = [
            ("title", "Hi"),
            ("cost", "5"),
            ("user_input", "true"),
            ("skip_queue", "false"),
            ("cooldown_on", "true"),
            ("cooldown", "30"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let d = Draft::from_fields(&fields);
        assert_eq!(d.title, "Hi");
        assert!(d.user_input && !d.skip_queue && d.cooldown_on);
        assert_eq!(d.cooldown, "30");
    }
}
