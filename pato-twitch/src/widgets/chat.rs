//! Twitch chat custom widget: a live chat view (scrollable message list + a
//! pinned send box) of the *primary channel*.
//!
//! The widget has no channel picker. Its one IRC connection is also the feed
//! for chat commands, so it is locked to the primary channel and driven by
//! `connect` / `force_disconnect` from the plugin (see the primary channel
//! setting in `lib.rs`). Viewing additional channels is a later addition and
//! will be display-only. IRC protocol details live in `crate::irc`; this file
//! only owns widget state and the node tree.

use std::collections::VecDeque;

use crate::irc::{self, IrcConnection, IrcEvent, Role};
use crate::pato::plugin::{widget_dom as dom, widgets};

use super::builder::{blank_style, plain, tiles, Builder};

const WIDGET_ID: &str = "chat";
/// How many chat lines are kept (and rendered) at once.
const MAX_MESSAGES: usize = 100;

pub const HANDLER_MESSAGE_INPUT: &str = "message-input";
pub const HANDLER_MESSAGE_SEND: &str = "message-send";

struct ChatMessage {
    id: u64,
    username: String,
    text: String,
}

enum State {
    Disconnected {
        error: Option<String>,
    },
    Connected {
        conn: IrcConnection,
        messages: VecDeque<ChatMessage>,
        next_message_id: u64,
        message_input: String,
        // Our own role/display name in this channel. Seeded from whether we
        // own the channel, then kept current by Twitch's USERSTATE.
        self_role: Role,
        self_display_name: String,
    },
}

pub struct TwitchChatWidget {
    state: State,
}

impl TwitchChatWidget {
    pub fn new() -> Self {
        widgets::register(&widgets::WidgetSpec {
            id: WIDGET_ID.to_string(),
            size: tiles(6, 8),
            min: Some(tiles(4, 5)),
            max: Some(tiles(10, 14)),
        });
        let widget = TwitchChatWidget {
            state: State::Disconnected { error: None },
        };
        widget.render();
        widget
    }

    // --- user interactions (from `widget-events.on-event`) ----------------

    pub fn on_input(&mut self, node_key: &str, text: String) {
        if let (State::Connected { message_input, .. }, HANDLER_MESSAGE_INPUT) =
            (&mut self.state, node_key)
        {
            *message_input = text;
        }
    }

    pub fn on_click(&mut self, node_key: &str) {
        if node_key == HANDLER_MESSAGE_SEND {
            self.send_pending_message();
        }
    }

    /// (Re)connect to `channel` as `login`, replacing any existing
    /// connection. The history of the previous channel is dropped.
    pub fn connect(&mut self, channel: &str, login: &str, token: &str) {
        self.close_connection();
        self.state = match irc::connect(login, token, channel) {
            Ok(conn) => State::Connected {
                self_role: if conn.self_login == conn.channel {
                    Role::Broadcaster
                } else {
                    Role::Everyone
                },
                self_display_name: conn.self_login.clone(),
                conn,
                messages: VecDeque::new(),
                next_message_id: 0,
                message_input: String::new(),
            },
            Err(e) => State::Disconnected { error: Some(e) },
        };
        self.render();
    }

    fn send_pending_message(&mut self) {
        let State::Connected {
            conn,
            message_input,
            messages,
            next_message_id,
            self_role,
            self_display_name,
            ..
        } = &mut self.state
        else {
            return;
        };
        let text = message_input.trim().to_string();
        if text.is_empty() {
            return;
        }
        irc::send_message(conn, &text);
        // A message the user types here is a candidate command like any
        // other chat line. (Messages sent by *other plugins* through
        // `send_message` below deliberately are not - that is what stops a
        // command's reply from re-triggering commands.) Twitch doesn't
        // return our user id for our own messages; the login is the stable
        // key instead.
        crate::commands::ingest(&conn.self_login, self_display_name, "", *self_role, &text);
        // Twitch's IRC doesn't echo a client's own PRIVMSGs back (no
        // echo-message capability support), so show it locally ourselves.
        messages.push_back(ChatMessage {
            id: *next_message_id,
            username: conn.self_login.clone(),
            text,
        });
        *next_message_id += 1;
        while messages.len() > MAX_MESSAGES {
            messages.pop_front();
        }
        message_input.clear();
        self.render();
    }

    /// Sends `text` to the connected channel on behalf of another plugin
    /// (`pato:pato-twitch/chat.send-message`). Returns false when there is no
    /// open chat connection. Shown in the widget like a typed message.
    ///
    /// Intentionally NOT forwarded to `pato-commands`: replies sent through
    /// this path must never be parsed as commands, or a reply containing
    /// `!something` would trigger it (and could loop).
    pub fn send_message(&mut self, text: &str) -> bool {
        let State::Connected {
            conn,
            messages,
            next_message_id,
            ..
        } = &mut self.state
        else {
            return false;
        };
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        irc::send_message(conn, text);
        messages.push_back(ChatMessage {
            id: *next_message_id,
            username: conn.self_login.clone(),
            text: text.to_string(),
        });
        *next_message_id += 1;
        while messages.len() > MAX_MESSAGES {
            messages.pop_front();
        }
        self.render();
        true
    }

    /// The channel (lowercase, no `#`) chat is connected to, if any.
    pub fn channel(&self) -> Option<String> {
        match &self.state {
            State::Connected { conn, .. } => Some(conn.channel.clone()),
            State::Disconnected { .. } => None,
        }
    }

    /// Close any open IRC connection and show the disconnected view. Used on
    /// a Twitch account sign-out (`PatoTwitch::logout`).
    pub fn force_disconnect(&mut self) {
        self.close_connection();
        self.state = State::Disconnected { error: None };
        self.render();
    }

    fn close_connection(&mut self) {
        if let State::Connected { conn, .. } = &self.state {
            irc::disconnect(conn);
        }
    }

    // --- websocket events (`crate::pato::plugin::websocket_events::Guest`) -

    pub fn on_message(&mut self, conn_id: &str, data: &str) {
        let State::Connected { conn, .. } = &self.state else {
            return;
        };
        if conn.id != conn_id {
            return;
        }
        let mut changed = false;
        for event in irc::handle_frame(conn_id, data) {
            match event {
                IrcEvent::Message {
                    username,
                    display_name,
                    user_id,
                    role,
                    text,
                } => {
                    // Hand the line to the command system. Fire-and-forget
                    // (the host broker doesn't wait for pato-commands), so
                    // this can't stall chat rendering or deadlock if
                    // pato-commands replies through `send_message`.
                    crate::commands::ingest(&username, &display_name, &user_id, role, &text);
                    let State::Connected {
                        messages,
                        next_message_id,
                        ..
                    } = &mut self.state
                    else {
                        return;
                    };
                    messages.push_back(ChatMessage {
                        id: *next_message_id,
                        username,
                        text,
                    });
                    *next_message_id += 1;
                    while messages.len() > MAX_MESSAGES {
                        messages.pop_front();
                    }
                    changed = true;
                }
                IrcEvent::SelfState { role, display_name } => {
                    if let State::Connected {
                        self_role,
                        self_display_name,
                        ..
                    } = &mut self.state
                    {
                        *self_role = role;
                        if !display_name.is_empty() {
                            *self_display_name = display_name;
                        }
                    }
                }
                IrcEvent::AuthFailed(reason) => {
                    self.state = State::Disconnected {
                        error: Some(format!(
                            "Twitch rejected the chat login: {reason} (try reconnecting to Twitch)"
                        )),
                    };
                    self.render();
                    return;
                }
            }
        }
        if changed {
            self.render();
        }
    }

    pub fn on_closed(&mut self, conn_id: &str, message: String) {
        if let State::Connected { conn, .. } = &self.state {
            if conn.id == conn_id {
                self.state = State::Disconnected {
                    error: Some(message),
                };
                self.render();
            }
        }
    }

    // --- rendering ----------------------------------------------------

    fn render(&self) {
        let mut b = Builder::default();
        match &self.state {
            State::Disconnected { error } => render_disconnected(&mut b, error.as_deref()),
            State::Connected {
                conn,
                messages,
                message_input,
                ..
            } => render_chat(&mut b, &conn.channel, messages, message_input),
        }
        widgets::update(&widgets::Widget::Custom(widgets::CustomWidget {
            id: WIDGET_ID.to_string(),
            nodes: b.nodes,
        }));
    }
}

fn render_disconnected(b: &mut Builder, error: Option<&str>) {
    let root = b.element(
        None,
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexCol),
                gap: Some(dom::Space::Sm),
                padding: Some(dom::Space::Md),
                align: Some(dom::Align::Center),
                justify: Some(dom::Justify::Center),
                height: Some(dom::Size::Fill),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );

    let heading = b.element(Some(root), plain(dom::Tag::H3));
    b.text(heading, "Twitch Chat");

    let (color, message) = match error {
        Some(message) => (dom::ThemeColor::Danger, message),
        None => (
            dom::ThemeColor::FgMuted,
            "Connect Twitch from the status widget to show your chat.",
        ),
    };
    let p = b.element(
        Some(root),
        dom::Element {
            style: Some(dom::Style {
                color: Some(color),
                font_size: Some(dom::TextSize::Sm),
                ..blank_style()
            }),
            ..plain(dom::Tag::P)
        },
    );
    b.text(p, message);
}

fn render_chat(
    b: &mut Builder,
    channel: &str,
    messages: &VecDeque<ChatMessage>,
    message_input: &str,
) {
    let root = b.element(
        None,
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexCol),
                height: Some(dom::Size::Fill),
                gap: Some(dom::Space::Xs),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );

    // header: platform badge + channel name
    let header = b.element(
        Some(root),
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexRow),
                gap: Some(dom::Space::Xs),
                align: Some(dom::Align::Center),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );
    b.element(Some(header), super::platform_badge());
    let title = b.element(
        Some(header),
        dom::Element {
            style: Some(dom::Style {
                color: Some(dom::ThemeColor::FgMuted),
                font_size: Some(dom::TextSize::Sm),
                ..blank_style()
            }),
            ..plain(dom::Tag::Span)
        },
    );
    b.text(title, &format!("#{channel}"));

    // scrollable message list; keyed so its scroll position and each row's
    // identity survive across re-renders (see custom-widgets.md - keys are
    // required for lists that grow/reorder).
    let list = b.element(
        Some(root),
        dom::Element {
            key: Some("message-list".into()),
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexCol),
                gap: Some(dom::Space::Xs),
                grow: Some(true),
                overflow: Some(dom::Overflow::Scroll),
                // Follow new messages to the bottom unless the viewer has
                // scrolled up to read history.
                sticky: Some(dom::StickyEdge::Bottom),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );
    for msg in messages {
        let line = b.element(
            Some(list),
            dom::Element {
                key: Some(format!("msg-{}", msg.id)),
                ..plain(dom::Tag::P)
            },
        );
        let user = b.element(
            Some(line),
            dom::Element {
                style: Some(dom::Style {
                    color: Some(dom::ThemeColor::Primary),
                    ..blank_style()
                }),
                ..plain(dom::Tag::Strong)
            },
        );
        b.text(user, &msg.username);
        b.text(line, &format!(": {}", msg.text));
    }

    // pinned send row
    let footer = b.element(
        Some(root),
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexRow),
                gap: Some(dom::Space::Xs),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );
    b.element(
        Some(footer),
        dom::Element {
            key: Some(HANDLER_MESSAGE_INPUT.into()),
            attrs: vec![
                dom::Attr::Placeholder("Send a message".into()),
                dom::Attr::Value(message_input.to_string()),
            ],
            style: Some(dom::Style {
                grow: Some(true),
                ..blank_style()
            }),
            events: vec![
                dom::Binding {
                    kind: dom::EventKind::Input,
                    handler: HANDLER_MESSAGE_INPUT.into(),
                },
                dom::Binding {
                    kind: dom::EventKind::EnterKey,
                    handler: HANDLER_MESSAGE_SEND.into(),
                },
            ],
            ..plain(dom::Tag::Input)
        },
    );
    let send = b.element(
        Some(footer),
        dom::Element {
            key: Some(HANDLER_MESSAGE_SEND.into()),
            events: vec![dom::Binding {
                kind: dom::EventKind::Click,
                handler: HANDLER_MESSAGE_SEND.into(),
            }],
            ..plain(dom::Tag::Button)
        },
    );
    b.text(send, "Send");
}
