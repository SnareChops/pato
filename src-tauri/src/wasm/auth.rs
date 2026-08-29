use crate::wasm;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use wasmtime::component::{HasSelf, Val};

wasmtime::component::bindgen!({
  world: "oauth",
  imports: { default: async | trappable},
  exports: {default: async},
});

impl pato::plugin::auth::Host for wasm::HostData {
    async fn open_auth_window(
        &mut self,
        pid: String,
        title: String,
        url: String,
    ) -> Result<bool, wasmtime::Error> {
        let parsed_url = url.parse().map_err(|e| wasmtime::Error::msg(e))?;
        let app = crate::APP_HANDLE
            .get()
            .ok_or(wasmtime::Error::msg("Missing app handle"))?;
        WebviewWindowBuilder::new(
            app,
            "oauth_window".to_string(),
            WebviewUrl::External(parsed_url),
        )
        .title(title)
        .inner_size(500.0, 700.0)
        .center()
        .on_navigation(move |u| {
            if u.domain() == Some("snarechops.net") {
                let url = u.to_string();
                let pid = pid.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = wasm::plugin_call(
                        &pid,
                        "pato:plugin/auth-events",
                        "on-auth",
                        vec![
                            Val::Bool(true),
                            Val::Option(Some(Box::new(Val::String(url)))),
                        ],
                        vec![Val::Bool(false)],
                    )
                    .await;
                });
                // Close the WebviewWindow
                if let Some(window) = app.get_webview_window("oauth_window") {
                    let _ = window.close();
                }

                return false;
            }
            return true;
        })
        .build()
        .map_err(|e| wasmtime::Error::msg(e))?;
        Ok(true)
    }
}

pub async fn init() -> Result<(), String> {
    println!("Initializing Auth module...");
    wasm::actor()
        .await
        .map_err(|e| e.to_string())?
        .link(|linker| pato::plugin::auth::add_to_linker::<_, HasSelf<_>>(linker, |host| host))
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
