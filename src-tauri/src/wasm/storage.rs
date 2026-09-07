use crate::{js::call_js_or_default, wasm};
use serde_json::json;

wasmtime::component::bindgen!({
    world: "store",
    imports: { default: async },
    exports: { default: async },
});

pub use pato::plugin::storage;
use wasmtime::component::HasSelf;

impl storage::Host for wasm::HostData {
    async fn get(&mut self, key: String) -> Option<String> {
        call_js_or_default(
            "storeGet",
            json!({
              "plugin": self.name,
              "key": key,
            }),
        )
        .await
    }

    async fn set(&mut self, key: String, value: String) -> bool {
        call_js_or_default(
            "storeSet",
            json!({
              "plugin": self.name,
              "key": key,
              "value": value,
            }),
        )
        .await
    }

    async fn del(&mut self, key: String) -> bool {
        call_js_or_default(
            "storeDel",
            json!({
              "plugin": self.name,
              "key": key,
            }),
        )
        .await
    }
}

pub fn init() -> Result<(), String> {
    println!("Initializing Storage module...");
    wasm::link(|linker| pato::plugin::storage::add_to_linker::<_, HasSelf<_>>(linker, |host| host))
}
