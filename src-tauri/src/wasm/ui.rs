use crate::js::call_js;
use crate::wasm;
use serde_json::{json, Value};
use wasmtime::component::{HasSelf, Val};

wasmtime::component::bindgen!({
    world: "ui",
    imports: { default: async | trappable },
    exports: { default: async },
});

use pato::plugin::widgets;

fn widget_json(name: String, widget: widgets::Widget) -> Value {
    match widget {
        widgets::Widget::StatusWidget(w) => json!({
            "type": "status",
            "id": format!("{}|{}", name, w.id),
            "icon": w.icon,
            "label": w.label,
            "tooltip": w.tooltip,
            // "actions": w.actions,
        }),
    }
}

impl widgets::Host for crate::wasm::HostData {
    async fn update(
        &mut self,
        pid: String,
        widget: widgets::Widget,
    ) -> Result<bool, wasmtime::Error> {
        println!("widgets::update called for pid {}: {:?}", pid, widget);
        if let Some(name) = self.name(pid) {
            call_js::<_, bool>("widgetUpdate", widget_json(name, widget))
                .await
                .map_err(|e| wasmtime::Error::msg(e))
        } else {
            Err(wasmtime::Error::msg("Invalid plugin ID"))
        }
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

pub async fn init() -> Result<(), String> {
    println!("Initializing UI module...");
    wasm::actor()
        .await
        .map_err(|e| e.to_string())?
        .link(|linker| pato::plugin::widgets::add_to_linker::<_, HasSelf<_>>(linker, |host| host))
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
