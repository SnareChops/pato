use crate::js::call_js_or_default;
use crate::wasm;
use wasmtime::component::{HasSelf, Val};

mod plugin {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "ui",
        imports: { default: async },
        exports: { default: async },
    });
}
use plugin::pato::plugin::widgets;

/// Wire types for the core -> webview `widgetUpdate` call. These mirror
/// `pato:internal/widget-view` in wit-internal/core-ui.wit (the source jco
/// reads to type the JS side); serde is configured to emit exactly the
/// component-model JSON shape jco generates (`{ tag, val }` variants,
/// omitted `option` fields). wasmtime's bindgen cannot produce this
/// encoding, so this stays hand-written — kept honest by the exhaustive
/// `match` in `Host::update` below.
mod view {
    use serde::Serialize;

    #[derive(Serialize)]
    #[serde(tag = "tag", content = "val", rename_all = "kebab-case")]
    pub enum Widget {
        StatusWidget(StatusWidget),
    }

    #[derive(Serialize)]
    pub struct StatusWidget {
        pub id: String,
        pub icon: String,
        pub label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub tooltip: Option<String>,
        pub actions: Vec<Action>,
    }

    #[derive(Serialize)]
    pub struct Action {
        pub id: String,
        pub label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub tooltip: Option<String>,
    }
}

impl widgets::Host for crate::wasm::HostData {
    async fn update(&mut self, widget: widgets::Widget) -> bool {
        let vw = match widget {
            widgets::Widget::StatusWidget(w) => view::Widget::StatusWidget(view::StatusWidget {
                id: format!("{}|{}", self.name, w.id),
                icon: w.icon,
                label: w.label,
                tooltip: w.tooltip,
                actions: w
                    .actions
                    .into_iter()
                    .map(|a| view::Action {
                        id: a.id,
                        label: a.text,
                        tooltip: a.description,
                    })
                    .collect(),
            }),
        };
        call_js_or_default::<_, bool>("widgetUpdate", vw).await
    }
}

#[tauri::command]
pub async fn status_widget_clicked(id: String) -> Result<(), String> {
    let (plugin_name, widget_id) = match id.split_once('|') {
        Some((a, b)) => (a.to_owned(), b.to_owned()),
        None => return Err("Invalid ID format".to_string()),
    };
    wasm::plugin_call_by_name(
        &plugin_name,
        "pato:plugin/widget-events",
        "on-event",
        vec![
            Val::Variant("clicked".to_string(), None),
            Val::String(widget_id.clone()),
        ],
        vec![],
    )
    .await?;
    Ok(())
}

pub fn init() -> Result<(), String> {
    println!("Initializing UI module...");
    wasm::link(|linker| widgets::add_to_linker::<_, HasSelf<_>>(linker, |host| host))
}

#[cfg(test)]
mod tests {
    use super::view;

    /// Guards that `view::*` serializes to the component-model JSON shape jco
    /// generates for `pato:internal/widget-view` (see src-ui/generated/). If
    /// this drifts, the typed JS `widgetUpdate` receives a shape it can't read.
    #[test]
    fn wire_encoding_matches_jco() {
        let w = view::Widget::StatusWidget(view::StatusWidget {
            id: "twitch|live".into(),
            icon: "i".into(),
            label: "l".into(),
            tooltip: None,
            actions: vec![view::Action {
                id: "a".into(),
                label: "Go".into(),
                tooltip: Some("t".into()),
            }],
        });
        assert_eq!(
            serde_json::to_value(&w).unwrap(),
            serde_json::json!({
                "tag": "status-widget",
                "val": {
                    "id": "twitch|live",
                    "icon": "i",
                    "label": "l",
                    "actions": [{ "id": "a", "label": "Go", "tooltip": "t" }],
                }
            })
        );
    }
}
