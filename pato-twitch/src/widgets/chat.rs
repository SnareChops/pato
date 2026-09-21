//! Twitch chat custom widget: a channel picker that hands off to a live
//! chat view (scrollable message list + a pinned send box) once connected.
//! IRC protocol details live in `crate::irc`; this file only owns widget
//! state and the node tree.

use std::collections::VecDeque;

use crate::irc::{self, IrcConnection, IrcEvent};
use crate::pato::plugin::{widget_dom as dom, widgets};

use super::builder::{blank_style, plain, tiles, Builder};

const WIDGET_ID: &str = "chat";
/// How many chat lines are kept (and rendered) at once.
const MAX_MESSAGES: usize = 100;

pub const HANDLER_CHANNEL_INPUT: &str = "channel-input";
pub const HANDLER_CHANNEL_SUBMIT: &str = "channel-submit";
pub const HANDLER_MESSAGE_INPUT: &str = "message-input";
pub const HANDLER_MESSAGE_SEND: &str = "message-send";
pub const HANDLER_OPTIONS_TOGGLE: &str = "options-toggle";
pub const HANDLER_DISCONNECT: &str = "disconnect";

struct ChatMessage {
    id: u64,
    username: String,
    text: String,
}

enum State {
    Disconnected {
        channel_input: String,
        error: Option<String>,
    },
    Connected {
        conn: IrcConnection,
        messages: VecDeque<ChatMessage>,
        next_message_id: u64,
        message_input: String,
        options_open: bool,
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
            state: State::Disconnected {
                channel_input: String::new(),
                error: None,
            },
        };
        widget.render();
        widget
    }

    // --- user interactions (from `widget-events.on-event`) ----------------

    pub fn on_input(&mut self, node_key: &str, text: String) {
        match (&mut self.state, node_key) {
            (State::Disconnected { channel_input, .. }, HANDLER_CHANNEL_INPUT) => {
                *channel_input = text;
            }
            (State::Connected { message_input, .. }, HANDLER_MESSAGE_INPUT) => {
                *message_input = text;
            }
            _ => {}
        }
    }

    pub fn on_click(&mut self, node_key: &str, login: Option<&str>, token: Option<&str>) {
        match node_key {
            HANDLER_CHANNEL_SUBMIT => self.submit_channel(login, token),
            HANDLER_MESSAGE_SEND => self.send_pending_message(),
            HANDLER_OPTIONS_TOGGLE => {
                if let State::Connected { options_open, .. } = &mut self.state {
                    *options_open = !*options_open;
                    self.render();
                }
            }
            HANDLER_DISCONNECT => self.force_disconnect(),
            _ => {}
        }
    }

    fn submit_channel(&mut self, login: Option<&str>, token: Option<&str>) {
        let State::Disconnected { channel_input, .. } = &self.state else {
            return;
        };
        let channel = channel_input.trim().to_string();
        if channel.is_empty() {
            return;
        }
        let (Some(login), Some(token)) = (login, token) else {
            self.state = State::Disconnected {
                channel_input: channel,
                error: Some("Connect to Twitch first (see the status widget)".to_string()),
            };
            self.render();
            return;
        };
        self.state = match irc::connect(login, token, &channel) {
            Ok(conn) => State::Connected {
                conn,
                messages: VecDeque::new(),
                next_message_id: 0,
                message_input: String::new(),
                options_open: false,
            },
            Err(e) => State::Disconnected {
                channel_input: channel,
                error: Some(e),
            },
        };
        self.render();
    }

    fn send_pending_message(&mut self) {
        let State::Connected {
            conn,
            message_input,
            messages,
            next_message_id,
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

    /// Reset to the disconnected picker view, closing any open IRC
    /// connection. Used both by the widget's own "Disconnect" option and by
    /// a full Twitch account sign-out (`PatoTwitch::logout`), which affects
    /// every widget that depends on the connection.
    pub fn force_disconnect(&mut self) {
        if let State::Connected { conn, .. } = &self.state {
            irc::disconnect(conn);
        }
        self.state = State::Disconnected {
            channel_input: String::new(),
            error: None,
        };
        self.render();
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
                IrcEvent::Message { username, text } => {
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
                IrcEvent::AuthFailed(reason) => {
                    self.state = State::Disconnected {
                        channel_input: String::new(),
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
                    channel_input: String::new(),
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
            State::Disconnected {
                channel_input,
                error,
            } => render_picker(&mut b, channel_input, error.as_deref()),
            State::Connected {
                conn,
                messages,
                message_input,
                options_open,
                ..
            } => render_chat(&mut b, &conn.channel, messages, message_input, *options_open),
        }
        widgets::update(&widgets::Widget::Custom(widgets::CustomWidget {
            id: WIDGET_ID.to_string(),
            nodes: b.nodes,
        }));
    }
}

fn render_picker(b: &mut Builder, channel_input: &str, error: Option<&str>) {
    let root = b.element(
        None,
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexCol),
                gap: Some(dom::Space::Sm),
                padding: Some(dom::Space::Md),
                align: Some(dom::Align::Stretch),
                justify: Some(dom::Justify::Center),
                height: Some(dom::Size::Fill),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );

    let heading = b.element(Some(root), plain(dom::Tag::H3));
    b.text(heading, "Connect to Twitch Chat");

    if let Some(message) = error {
        let err = b.element(
            Some(root),
            dom::Element {
                style: Some(dom::Style {
                    color: Some(dom::ThemeColor::Danger),
                    font_size: Some(dom::TextSize::Sm),
                    ..blank_style()
                }),
                ..plain(dom::Tag::P)
            },
        );
        b.text(err, message);
    }

    b.element(
        Some(root),
        dom::Element {
            key: Some(HANDLER_CHANNEL_INPUT.into()),
            attrs: vec![
                dom::Attr::Placeholder("channel name".into()),
                dom::Attr::Value(channel_input.to_string()),
            ],
            events: vec![
                dom::Binding {
                    kind: dom::EventKind::Input,
                    handler: HANDLER_CHANNEL_INPUT.into(),
                },
                dom::Binding {
                    kind: dom::EventKind::EnterKey,
                    handler: HANDLER_CHANNEL_SUBMIT.into(),
                },
            ],
            ..plain(dom::Tag::Input)
        },
    );

    let button = b.element(
        Some(root),
        dom::Element {
            key: Some(HANDLER_CHANNEL_SUBMIT.into()),
            events: vec![dom::Binding {
                kind: dom::EventKind::Click,
                handler: HANDLER_CHANNEL_SUBMIT.into(),
            }],
            ..plain(dom::Tag::Button)
        },
    );
    b.text(button, "Join");
}

fn render_chat(
    b: &mut Builder,
    channel: &str,
    messages: &VecDeque<ChatMessage>,
    message_input: &str,
    options_open: bool,
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

    // header: channel name + options button, dropdown opens below it
    let header = b.element(
        Some(root),
        dom::Element {
            style: Some(dom::Style {
                layout: Some(dom::Layout::FlexRow),
                justify: Some(dom::Justify::Between),
                align: Some(dom::Align::Center),
                ..blank_style()
            }),
            ..plain(dom::Tag::Div)
        },
    );
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
    let options_btn = b.element(
        Some(header),
        dom::Element {
            key: Some(HANDLER_OPTIONS_TOGGLE.into()),
            events: vec![dom::Binding {
                kind: dom::EventKind::Click,
                handler: HANDLER_OPTIONS_TOGGLE.into(),
            }],
            ..plain(dom::Tag::Button)
        },
    );
    b.text(options_btn, "Options");

    if options_open {
        let dropdown = b.element(
            Some(root),
            dom::Element {
                style: Some(dom::Style {
                    layout: Some(dom::Layout::FlexCol),
                    padding: Some(dom::Space::Sm),
                    gap: Some(dom::Space::Xs),
                    background: Some(dom::ThemeColor::Bg),
                    radius: Some(dom::Space::Sm),
                    align: Some(dom::Align::Stretch),
                    ..blank_style()
                }),
                ..plain(dom::Tag::Div)
            },
        );
        let disconnect = b.element(
            Some(dropdown),
            dom::Element {
                key: Some(HANDLER_DISCONNECT.into()),
                events: vec![dom::Binding {
                    kind: dom::EventKind::Click,
                    handler: HANDLER_DISCONNECT.into(),
                }],
                ..plain(dom::Tag::Button)
            },
        );
        b.text(disconnect, "Disconnect");
    }

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
