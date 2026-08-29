use crate::{js::call_js, wasm};
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::json;
use wasmtime::component::HasSelf;

wasmtime::component::bindgen!({
    world: "db",
    imports: { default: async | trappable },
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
    async fn register(
        &mut self,
        pid: String,
        schema: Vec<database::Store>,
    ) -> Result<bool, wasmtime::Error> {
        self.schemas.insert(pid, schema);
        Ok(true)
    }

    async fn get(
        &mut self,
        pid: String,
        store: String,
        index: Option<String>,
        query: database::Query,
    ) -> Result<Vec<Vec<(String, String)>>, wasmtime::Error> {
        call_js::<_, Vec<Vec<(String, String)>>>(
            "dbGet",
            Some(json!({
              "plugin": self.name(pid),
              "store": store,
              "index": index,
              "query": query,
            })),
        )
        .await
        .map_err(|e| wasmtime::Error::msg(e))
    }

    async fn set(
        &mut self,
        pid: String,
        store: String,
        data: Vec<(String, String)>,
    ) -> Result<bool, wasmtime::Error> {
        call_js::<_, bool>(
            "dbSet",
            Some(json!({
              "plugin": self.name(pid),
              "store": store,
              "data": data,
            })),
        )
        .await
        .map_err(|e| wasmtime::Error::msg(e))
    }

    async fn get_all(
        &mut self,
        pid: String,
        store: String,
    ) -> Result<Vec<Vec<(String, String)>>, wasmtime::Error> {
        call_js::<_, Vec<Vec<(String, String)>>>(
            "dbGetAll",
            Some(json!({
              "plugin": self.name(pid),
              "store": store,
            })),
        )
        .await
        .map_err(|e| wasmtime::Error::msg(e))
    }

    async fn del(
        &mut self,
        pid: String,
        store: String,
        query: database::Query,
    ) -> Result<bool, wasmtime::Error> {
        call_js::<_, bool>(
            "dbDel",
            Some(json!({
              "plugin": self.name(pid),
              "store": store,
              "query": query,
            })),
        )
        .await
        .map_err(|e| wasmtime::Error::msg(e))
    }

    async fn count(
        &mut self,
        pid: String,
        store: String,
        index: Option<String>,
        query: Option<database::Query>,
    ) -> Result<u32, wasmtime::Error> {
        call_js::<_, u32>(
            "dbCount",
            Some(json!({
              "plugin": self.name(pid),
              "store": store,
              "index": index,
              "query": query,
            })),
        )
        .await
        .map_err(|e| wasmtime::Error::msg(e))
    }
}

pub async fn init() -> Result<(), String> {
    println!("Initializing DB module...");
    wasm::actor()
        .await
        .map_err(|e| e.to_string())?
        .link(|linker| pato::plugin::database::add_to_linker::<_, HasSelf<_>>(linker, |host| host))
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
