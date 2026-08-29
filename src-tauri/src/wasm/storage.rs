use crate::{js::call_js, wasm};
use serde_json::json;

wasmtime::component::bindgen!({
    world: "store",
    imports: { default: async | trappable },
    exports: { default: async },
});

pub use pato::plugin::storage;
use wasmtime::component::HasSelf;

impl storage::Host for wasm::HostData {
    async fn get(&mut self, pid: String, key: String) -> Result<Option<String>, wasmtime::Error> {
        call_js::<_, Option<String>>(
            "storeGet",
            json!({
              "plugin": self.name(pid),
              "key": key,
            }),
        )
        .await
        .map_err(|e| wasmtime::Error::msg(e))
    }

    async fn set(
        &mut self,
        pid: String,
        key: String,
        value: String,
    ) -> Result<bool, wasmtime::Error> {
        call_js::<_, bool>(
            "storeSet",
            json!({
              "plugin": self.name(pid),
              "key": key,
              "value": value,
            }),
        )
        .await
        .map_err(|e| wasmtime::Error::msg(e))
    }

    async fn del(&mut self, pid: String, key: String) -> Result<bool, wasmtime::Error> {
        call_js::<_, bool>(
            "storeDel",
            json!({
              "plugin": self.name(pid),
              "key": key,
            }),
        )
        .await
        .map_err(|e| wasmtime::Error::msg(e))
    }
}

pub async fn init() -> Result<(), String> {
    println!("Initializing Storage module...");
    wasm::actor()
        .await
        .map_err(|e| e.to_string())?
        .link(|linker| pato::plugin::storage::add_to_linker::<_, HasSelf<_>>(linker, |host| host))
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
