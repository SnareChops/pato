//! Host broker for pato-obs's public `pato:pato-obs/scenes` API.
//!
//! Plugins are fully isolated components - each on its own thread with its
//! own `Store` (see `wasm.rs`) - so a plugin can't link directly against
//! another plugin's exports the way it links against a host-provided
//! interface. This module makes it *look* like it can: it implements
//! `scenes::Host` (bindgen'd from `wit-broker/world.wit`, which imports
//! `pato:pato-obs/scenes@0.1.0` fetched straight from `pato-obs/wit/api.wit`
//! - see that crate's `wkg.toml`), registered on the shared linker like any
//! other host interface. A 3rd-party plugin just adds the same import to its
//! own world and calls it normally; every call actually lands here and gets
//! forwarded to the real `pato_obs` plugin instance via the same
//! `wasm::plugin_call_by_name` the host already uses to deliver events *to*
//! a plugin (`wasm/ui/mod.rs`, `wasm/websocket.rs`) - `call_export` doesn't
//! care whether the call originated from host Rust code or, as here, from
//! another plugin's guest code.
//!
//! This is the general pattern for exposing any 1st-party plugin's API to
//! others: define the interface in that plugin's own `wit/`, `export` it
//! from the plugin's world, add an `import` for it to `wit-broker/world.wit`,
//! and write a `Host` impl here that converts between the untyped `Val`s
//! `plugin_call_by_name` speaks and the typed values this module's own
//! bindgen generates for the interface.

use wasmtime::component::{HasSelf, Val};

use crate::wasm;

mod plugin {
    wasmtime::component::bindgen!({
        path: "wit-broker",
        world: "obs-broker",
        imports: { default: async },
        additional_derives: [PartialEq],
    });
}
use plugin::pato::pato_obs::scenes;

const PATO_OBS: &str = "pato_obs";
const SCENES: &str = "pato:pato-obs/scenes@0.1.0";

fn field<'a>(fields: &'a [(String, Val)], name: &str) -> Option<&'a Val> {
    fields.iter().find(|(k, _)| k == name).map(|(_, v)| v)
}
fn str_field(fields: &[(String, Val)], name: &str) -> String {
    match field(fields, name) {
        Some(Val::String(s)) => s.clone(),
        _ => String::new(),
    }
}
fn u32_field(fields: &[(String, Val)], name: &str) -> u32 {
    match field(fields, name) {
        Some(Val::U32(n)) => *n,
        _ => 0,
    }
}
fn bool_field(fields: &[(String, Val)], name: &str) -> bool {
    matches!(field(fields, name), Some(Val::Bool(true)))
}

fn to_scene(v: &Val) -> Option<scenes::Scene> {
    let Val::Record(fields) = v else { return None };
    Some(scenes::Scene {
        name: str_field(fields, "name"),
        uuid: str_field(fields, "uuid"),
    })
}
fn to_scene_item(v: &Val) -> Option<scenes::SceneItem> {
    let Val::Record(fields) = v else { return None };
    Some(scenes::SceneItem {
        id: u32_field(fields, "id"),
        source_name: str_field(fields, "source-name"),
        enabled: bool_field(fields, "enabled"),
    })
}

async fn call(func: &str, params: Vec<Val>, default: Vec<Val>) -> Vec<Val> {
    wasm::plugin_call_by_name(PATO_OBS, SCENES, func, params, default)
        .await
        .unwrap_or_default()
}

async fn call_bool(func: &str, params: Vec<Val>) -> bool {
    matches!(call(func, params, vec![Val::Bool(false)]).await.into_iter().next(), Some(Val::Bool(true)))
}

impl scenes::Host for wasm::HostData {
    async fn list_scenes(&mut self) -> Vec<scenes::Scene> {
        match call("list-scenes", vec![], vec![Val::List(vec![])]).await.into_iter().next() {
            Some(Val::List(items)) => items.iter().filter_map(to_scene).collect(),
            _ => vec![],
        }
    }

    async fn current_program_scene(&mut self) -> Option<scenes::Scene> {
        match call("current-program-scene", vec![], vec![Val::Option(None)]).await.into_iter().next() {
            Some(Val::Option(Some(boxed))) => to_scene(&boxed),
            _ => None,
        }
    }

    async fn list_scene_items(&mut self, scene: String) -> Vec<scenes::SceneItem> {
        match call("list-scene-items", vec![Val::String(scene)], vec![Val::List(vec![])])
            .await
            .into_iter()
            .next()
        {
            Some(Val::List(items)) => items.iter().filter_map(to_scene_item).collect(),
            _ => vec![],
        }
    }

    async fn create_scene(&mut self, name: String) -> bool {
        call_bool("create-scene", vec![Val::String(name)]).await
    }
    async fn remove_scene(&mut self, name: String) -> bool {
        call_bool("remove-scene", vec![Val::String(name)]).await
    }
    async fn set_current_scene(&mut self, name: String) -> bool {
        call_bool("set-current-scene", vec![Val::String(name)]).await
    }
    async fn create_scene_item(&mut self, scene: String, source: String) -> bool {
        call_bool("create-scene-item", vec![Val::String(scene), Val::String(source)]).await
    }
    async fn remove_scene_item(&mut self, scene: String, item_id: u32) -> bool {
        call_bool("remove-scene-item", vec![Val::String(scene), Val::U32(item_id)]).await
    }
    async fn set_scene_item_enabled(&mut self, scene: String, item_id: u32, enabled: bool) -> bool {
        call_bool(
            "set-scene-item-enabled",
            vec![Val::String(scene), Val::U32(item_id), Val::Bool(enabled)],
        )
        .await
    }
}

pub fn init() -> Result<(), String> {
    println!("Initializing OBS broker module...");
    wasm::link(|linker| scenes::add_to_linker::<_, HasSelf<_>>(linker, |host| host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_scene_fields_from_a_dynamic_record() {
        let v = Val::Record(vec![("name".into(), Val::String("Main".into())), ("uuid".into(), Val::String("u1".into()))]);
        assert_eq!(to_scene(&v), Some(scenes::Scene { name: "Main".into(), uuid: "u1".into() }));
    }

    #[test]
    fn extracts_scene_item_fields_from_a_dynamic_record() {
        let v = Val::Record(vec![
            ("id".into(), Val::U32(5)),
            ("source-name".into(), Val::String("Webcam".into())),
            ("enabled".into(), Val::Bool(true)),
        ]);
        assert_eq!(to_scene_item(&v), Some(scenes::SceneItem { id: 5, source_name: "Webcam".into(), enabled: true }));
    }

    #[test]
    fn non_record_values_are_rejected_rather_than_panicking() {
        assert_eq!(to_scene(&Val::Bool(true)), None);
        assert_eq!(to_scene_item(&Val::String("x".into())), None);
    }
}
