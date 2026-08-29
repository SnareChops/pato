use crate::{js::call_js, wasm};

wasmtime::component::bindgen!({
  world: "logger",
  imports: { default: async | trappable},
  exports: { default: async},
});

impl pato::plugin::log::Host for wasm::HostData {
    async fn info(&mut self, message: String) -> Result<(), wasmtime::Error> {
        call_js::<String, ()>("logInfo", message)
            .await
            .map_err(|e| wasmtime::Error::msg(e))
    }

    async fn warn(&mut self, message: String) -> Result<(), wasmtime::Error> {
        call_js::<String, ()>("logWarn", message)
            .await
            .map_err(|e| wasmtime::Error::msg(e))
    }

    async fn error(&mut self, message: String) -> Result<(), wasmtime::Error> {
        call_js::<String, ()>("logError", message)
            .await
            .map_err(|e| wasmtime::Error::msg(e))
    }
}
