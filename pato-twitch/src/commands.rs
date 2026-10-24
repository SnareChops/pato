//! Bridge from Twitch chat into the `pato-commands` plugin.
//!
//! `pato:pato-commands/platform` is brokered by the host (src-tauri), which
//! forwards the call to the pato-commands plugin if it is installed and
//! silently drops it otherwise - so this is always safe to call.

use crate::irc::Role;
use crate::pato::pato_commands::platform;

pub const PLATFORM: &str = "twitch";

pub fn ingest(username: &str, display_name: &str, user_id: &str, role: Role, text: &str) {
    platform::ingest(&platform::ChatMessage {
        platform: PLATFORM.to_string(),
        user_id: user_id.to_string(),
        username: username.to_string(),
        display_name: display_name.to_string(),
        role: to_wit_role(role),
        text: text.to_string(),
    });
}

fn to_wit_role(role: Role) -> platform::Role {
    match role {
        Role::Everyone => platform::Role::Everyone,
        Role::Subscriber => platform::Role::Subscriber,
        Role::Vip => platform::Role::Vip,
        Role::Moderator => platform::Role::Moderator,
        Role::Broadcaster => platform::Role::Broadcaster,
    }
}
