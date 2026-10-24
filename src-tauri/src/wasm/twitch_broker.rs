//! Host broker for pato-twitch's public `pato:pato-twitch/chat` API. Same
//! mechanism as `obs_broker.rs`: implements the interface for the host and
//! forwards each call to the real `pato_twitch` plugin instance.

use wasmtime::component::{HasSelf, Val};

use crate::wasm;

mod plugin {
    wasmtime::component::bindgen!({
        path: "wit-broker",
        world: "twitch-broker",
        imports: { default: async },
    });
}
use plugin::pato::pato_twitch::chat;

const PATO_TWITCH: &str = "pato_twitch";
const CHAT: &str = "pato:pato-twitch/chat@0.1.0";

impl chat::Host for wasm::HostData {
    async fn send_message(&mut self, text: String) -> bool {
        let result = wasm::plugin_call_by_name(
            PATO_TWITCH,
            CHAT,
            "send-message",
            vec![Val::String(text)],
            vec![Val::Bool(false)],
        )
        .await
        .unwrap_or_default();
        matches!(result.into_iter().next(), Some(Val::Bool(true)))
    }

    async fn channel(&mut self) -> Option<String> {
        let result = wasm::plugin_call_by_name(PATO_TWITCH, CHAT, "channel", vec![], vec![Val::Option(None)])
            .await
            .unwrap_or_default();
        match result.into_iter().next() {
            Some(Val::Option(Some(boxed))) => match *boxed {
                Val::String(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }
}

pub fn init() -> Result<(), String> {
    println!("Initializing Twitch broker module...");
    wasm::link(|linker| chat::add_to_linker::<_, HasSelf<_>>(linker, |host| host))
}
