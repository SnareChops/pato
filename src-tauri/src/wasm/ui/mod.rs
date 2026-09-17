use std::sync::LazyLock;

use dashmap::DashMap;
use serde::Deserialize;
use serde_json::json;
use wasmtime::component::{HasSelf, Val};

use crate::js::call_js_or_default;
use crate::wasm;

mod plugin {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "ui",
        imports: { default: async },
        exports: { default: async },
    });
}
use plugin::pato::plugin::widgets;

pub mod validate;
pub mod view;

const WIDGET_EVENTS: &str = "pato:plugin/widget-events";

/// Registered custom widgets, keyed by namespaced id (`"<plugin>|<local>"`).
/// Written from plugin threads (`Host::register`); read from the Tauri command
/// thread (`widget_resized`) to clamp user resizes to the plugin's bounds.
static REGISTERED: LazyLock<DashMap<String, view::WidgetSpec>> = LazyLock::new(DashMap::new);

/// Splits a namespaced widget id into `(plugin name, local id)`.
fn split_id(id: &str) -> Result<(String, String), String> {
    id.split_once('|')
        .map(|(a, b)| (a.to_owned(), b.to_owned()))
        .ok_or_else(|| format!("Invalid widget id format: {id}"))
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
            widgets::Widget::Custom(w) => {
                let nodes = match validate::tree(w.nodes, &self.name) {
                    Ok(nodes) => nodes,
                    Err(e) => {
                        eprintln!("Rejected custom widget `{}` from {}: {e}", w.id, self.name);
                        return false;
                    }
                };
                view::Widget::Custom(view::CustomWidget {
                    id: format!("{}|{}", self.name, w.id),
                    nodes,
                })
            }
        };
        call_js_or_default::<_, bool>("widgetUpdate", vw).await
    }

    async fn register(&mut self, spec: widgets::WidgetSpec) -> bool {
        let spec = view::WidgetSpec {
            id: format!("{}|{}", self.name, spec.id),
            size: spec.size.into(),
            min: spec.min.map(Into::into),
            max: spec.max.map(Into::into),
        };
        REGISTERED.insert(spec.id.clone(), spec.clone());
        call_js_or_default::<_, bool>("widgetRegister", spec).await
    }

    async fn set_value(&mut self, widget_id: String, slot: String, value: String) -> bool {
        call_js_or_default::<_, bool>(
            "widgetSetValue",
            json!({
                "widgetId": format!("{}|{}", self.name, widget_id),
                "slot": slot,
                "value": value,
            }),
        )
        .await
    }

    async fn request_render(&mut self, widget_id: String) {
        // Push model: the plugin re-renders by calling `update` itself. This
        // hint is a no-op until the core grows debounced/visibility-aware
        // rendering (see custom-widgets.md).
        println!("request-render hint for {}|{widget_id}", self.name);
    }
}

// --- webview -> core: user interactions --------------------------------------

/// Payload the webview extracted from a DOM event. Mirrors
/// `pato:plugin/widget-events.payload`.
#[derive(Deserialize)]
#[serde(tag = "tag", content = "val", rename_all = "kebab-case")]
pub enum EventPayload {
    None,
    Text(String),
    Toggled(bool),
    Fields(Vec<(String, String)>),
}
impl From<EventPayload> for Val {
    fn from(p: EventPayload) -> Val {
        match p {
            EventPayload::None => Val::Variant("none".into(), None),
            EventPayload::Text(s) => Val::Variant("text".into(), Some(Box::new(Val::String(s)))),
            EventPayload::Toggled(b) => {
                Val::Variant("toggled".into(), Some(Box::new(Val::Bool(b))))
            }
            EventPayload::Fields(fields) => Val::Variant(
                "fields".into(),
                Some(Box::new(Val::List(
                    fields
                        .into_iter()
                        .map(|(k, v)| Val::Tuple(vec![Val::String(k), Val::String(v)]))
                        .collect(),
                ))),
            ),
        }
    }
}

#[tauri::command]
pub async fn custom_widget_event(
    widget_id: String,
    node_key: String,
    kind: String,
    payload: EventPayload,
) -> Result<(), String> {
    let (plugin_name, local_id) = split_id(&widget_id)?;
    let record = Val::Record(vec![
        ("widget-id".into(), Val::String(local_id)),
        ("node-key".into(), Val::String(node_key)),
        ("kind".into(), Val::Enum(kind)),
        ("payload".into(), payload.into()),
    ]);
    wasm::plugin_call_by_name(
        &plugin_name,
        WIDGET_EVENTS,
        "on-event",
        vec![record],
        vec![],
    )
    .await?;
    Ok(())
}

/// The webview reports a custom widget's current tile layout — once when it
/// mounts, and again whenever the user resizes it. The core clamps to the
/// registered `min`/`max` and forwards to the plugin's `on-layout`.
#[tauri::command]
pub async fn widget_resized(widget_id: String, w: u32, h: u32) -> Result<(), String> {
    let (plugin_name, local_id) = split_id(&widget_id)?;

    let (mut w, mut h) = (w.max(1), h.max(1));
    if let Some(spec) = REGISTERED.get(&widget_id) {
        let lo = spec.min.unwrap_or(spec.size);
        let hi = spec.max.unwrap_or(spec.size);
        w = w.clamp(lo.w, hi.w);
        h = h.clamp(lo.h, hi.h);
    }

    let layout = Val::Record(vec![
        ("widget-id".into(), Val::String(local_id)),
        (
            "tiles".into(),
            Val::Record(vec![("w".into(), Val::U32(w)), ("h".into(), Val::U32(h))]),
        ),
    ]);
    wasm::plugin_call_by_name(
        &plugin_name,
        WIDGET_EVENTS,
        "on-layout",
        vec![layout],
        vec![],
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn status_widget_clicked(id: String) -> Result<(), String> {
    let (plugin_name, widget_id) = split_id(&id)?;
    wasm::plugin_call_by_name(
        &plugin_name,
        WIDGET_EVENTS,
        "on-status-event",
        vec![Val::String(widget_id), Val::Variant("clicked".into(), None)],
        vec![],
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn status_widget_action(id: String, action: String) -> Result<(), String> {
    let (plugin_name, widget_id) = split_id(&id)?;
    wasm::plugin_call_by_name(
        &plugin_name,
        WIDGET_EVENTS,
        "on-status-event",
        vec![
            Val::String(widget_id),
            Val::Variant("action".into(), Some(Box::new(Val::String(action)))),
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
