use crate::{js::call_js_or_default, wasm};
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::json;
use wasmtime::component::HasSelf;

wasmtime::component::bindgen!({
    world: "db",
    imports: { default: async },
    exports: { default: async },
});

pub use pato::plugin::database;
impl Serialize for database::Index {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("key", &self.key)?;
        map.serialize_entry("unique", &self.unique)?;
        map.end()
    }
}
impl Serialize for database::Store {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(4))?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("key", &self.key)?;
        map.serialize_entry("auto", &self.auto)?;
        map.serialize_entry("indexes", &self.indexes)?;
        map.end()
    }
}
impl Serialize for database::Bound {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("key", &self.key)?;
        map.serialize_entry("open", &self.open)?;
        map.end()
    }
}
impl Serialize for database::KeyRange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            database::KeyRange::LowerBound(lb) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("lower", &lb)?;
                map.end()
            }
            database::KeyRange::UpperBound(ub) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("upper", &ub)?;
                map.end()
            }
            database::KeyRange::Between((lower, upper)) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("lower", lower)?;
                map.serialize_entry("upper", upper)?;
                map.end()
            }
        }
    }
}

impl Serialize for database::Query {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            database::Query::Key(k) => {
                map.serialize_entry("key", &k)?;
                map.end()
            }
            database::Query::Keys(ks) => {
                map.serialize_entry("keys", &ks)?;
                map.end()
            }
            database::Query::Range(range) => {
                map.serialize_entry("range", &range)?;
                map.end()
            }
        }
    }
}

impl database::Host for crate::wasm::HostData {
    async fn register(&mut self, schema: Vec<database::Store>) -> bool {
        self.schema = schema;
        true
    }

    async fn get(
        &mut self,
        store: String,
        index: Option<String>,
        query: database::Query,
    ) -> Vec<Vec<(String, String)>> {
        call_js_or_default(
            "dbGet",
            Some(json!({
              "plugin": self.name,
              "store": store,
              "index": index,
              "query": query,
            })),
        )
        .await
    }

    async fn set(&mut self, store: String, data: Vec<(String, String)>) -> bool {
        call_js_or_default(
            "dbSet",
            Some(json!({
              "plugin": self.name,
              "store": store,
              "data": data,
            })),
        )
        .await
    }

    async fn get_all(&mut self, store: String) -> Vec<Vec<(String, String)>> {
        call_js_or_default(
            "dbGetAll",
            Some(json!({
              "plugin": self.name,
              "store": store,
            })),
        )
        .await
    }

    async fn del(&mut self, store: String, query: database::Query) -> bool {
        call_js_or_default(
            "dbDel",
            Some(json!({
              "plugin": self.name,
              "store": store,
              "query": query,
            })),
        )
        .await
    }

    async fn count(
        &mut self,
        store: String,
        index: Option<String>,
        query: Option<database::Query>,
    ) -> u32 {
        call_js_or_default(
            "dbCount",
            Some(json!({
              "plugin": self.name,
              "store": store,
              "index": index,
              "query": query,
            })),
        )
        .await
    }
}

pub fn init() -> Result<(), String> {
    println!("Initializing DB module...");
    wasm::link(|linker| pato::plugin::database::add_to_linker::<_, HasSelf<_>>(linker, |host| host))
}
