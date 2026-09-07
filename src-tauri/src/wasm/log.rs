use crate::{js::call_js_or_default, wasm};

wasmtime::component::bindgen!({
  world: "logger",
  imports: { default: async },
  exports: { default: async},
});

impl pato::plugin::log::Host for wasm::HostData {
    async fn info(&mut self, message: String) {
        call_js_or_default::<_, ()>("logInfo", message).await
    }

    async fn warn(&mut self, message: String) {
        call_js_or_default::<_, ()>("logWarn", message).await
    }

    async fn error(&mut self, message: String) {
        call_js_or_default::<_, ()>("logError", message).await
    }
}
