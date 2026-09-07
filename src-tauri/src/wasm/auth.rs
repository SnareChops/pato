use crate::wasm;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use wasmtime::component::{HasSelf, Val};

wasmtime::component::bindgen!({
  world: "oauth",
  imports: { default: async },
  exports: {default: async},
});

impl pato::plugin::auth::Host for wasm::HostData {
    async fn open_auth_window(&mut self, title: String, url: String) -> bool {
        match self.try_open_auth_window(title, url) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("open_auth_window failed: {e}");
                false
            }
        }
    }
}

impl wasm::HostData {
    fn try_open_auth_window(&self, title: String, url: String) -> Result<(), wasmtime::Error> {
        let pid = self.pid.clone();
        let parsed_url = url.parse().map_err(wasmtime::Error::msg)?;
        let app = crate::APP_HANDLE
            .get()
            .ok_or(wasmtime::Error::msg("Missing app handle"))?;
        // Unique per request: two concurrent auth flows (even from the same
        // plugin) must not collide on one window label.
        let window_label = format!("oauth_window_{}", uuid::Uuid::new_v4());
        WebviewWindowBuilder::new(app, window_label.clone(), WebviewUrl::External(parsed_url))
            .title(title)
            .inner_size(500.0, 700.0)
            .center()
            .on_navigation(move |u| {
                if u.domain() == Some("snarechops.net") {
                    let url = u.to_string();
                    let pid = pid.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = wasm::plugin_call(
                            &pid,
                            "pato:plugin/auth-events",
                            "on-auth",
                            vec![
                                Val::Bool(true),
                                Val::Option(Some(Box::new(Val::String(url)))),
                            ],
                            vec![Val::Result(Ok(None))],
                        )
                        .await
                        {
                            eprintln!("on-auth dispatch failed: {e}");
                        }
                    });
                    // Close the WebviewWindow
                    if let Some(window) = app.get_webview_window(&window_label) {
                        let _ = window.close();
                    }

                    return false;
                }
                true
            })
            .build()
            .map_err(wasmtime::Error::msg)?;
        Ok(())
    }
}

pub fn init() -> Result<(), String> {
    println!("Initializing Auth module...");
    wasm::link(|linker| pato::plugin::auth::add_to_linker::<_, HasSelf<_>>(linker, |host| host))
}
