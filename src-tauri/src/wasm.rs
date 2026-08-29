use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::thread;
use tokio::sync::{mpsc, oneshot, Mutex, MutexGuard};

use wasmtime::component::types::{ComponentInstance, ComponentItem};
use wasmtime::component::{types, HasData, HasSelf};
use wasmtime::component::{Component, Instance, Linker, LinkerInstance, ResourceTable, Val};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

pub mod auth;
pub mod db;
pub mod log;
mod plugins;
pub mod storage;
pub mod ui;

static ACTOR: OnceLock<Mutex<WasmActor>> = OnceLock::new();

// pub fn current_runtime() -> Result<MutexGuard<'static, WasmRuntime>, wasmtime::Error> {
//     if let Some(handle) = WASM_HANDLE.get() {
//         let runtime = handle
//             .lock()
//             .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
//         Ok(runtime)
//     } else {
//         Err(wasmtime::Error::msg("Cannot find current wasm runtime"))
//     }
// }

// pub async fn get_instance(instance_id: &str) -> Result<Instance, wasmtime::Error> {
//     let runtime = current_runtime()?;
//     runtime
//         .store
//         .data()
//         .plugins
//         .get(&instance_id.to_string())
//         .cloned()
//         .ok_or_else(|| wasmtime::Error::msg(format!("No plugin found for {instance_id}")))
// }

// pub fn with_current_instance<F, Fut, R>(
//     f: F,
// ) -> impl std::future::Future<Output = Result<R, wasmtime::Error>>
// where
//     F: FnOnce(String, &mut Instance) -> Fut,
//     Fut: std::future::Future<Output = R>,
// {
//     async {
//         let (instance_id, mut instance) =
//             CURRENT_INSTANCE.with(|ref_cell| match &*ref_cell.borrow_mut() {
//                 Some((instance_id, instance)) => Ok((instance_id.clone(), instance.clone())),
//                 None => Err(wasmtime::Error::msg("No current instance set")),
//             })?;
//         Ok(f(instance_id, &mut instance).await)
//     }
// }

// pub fn with_wasm_runtime<F, Fut, R>(
//     f: F,
// ) -> impl std::future::Future<Output = Result<R, wasmtime::Error>>
// where
//     F: FnOnce(&mut WasmRuntime) -> Fut,
//     Fut: std::future::Future<Output = R>,
// {
//     async {
//         let mut runtime = WASM_HANDLE
//             .get()
//             .ok_or_else(|| wasmtime::Error::msg("WASM runtime not initialized"))?
//             .lock()
//             .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
//         Ok(f(&mut runtime).await)
//     }
// }

pub struct HostData {
    wasi: WasiCtx,
    http_ctx: WasiHttpCtx,
    pub table: ResourceTable,
    plugins: HashMap<String, Instance>, // pid -> instance
    names: HashMap<String, String>,     // pid -> name
    pids: HashMap<String, String>,      // name -> pid
    schemas: HashMap<String, Vec<db::database::Store>>,
}
impl WasiView for HostData {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}
impl WasiHttpView for HostData {
    fn ctx(&mut self) -> &mut WasiHttpCtx {
        &mut self.http_ctx
    }
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}
impl HostData {
    pub fn add_plugin(&mut self, pid: String, name: String, instance: Instance) {
        self.plugins.insert(pid.clone(), instance);
        self.names.insert(pid.clone(), name.clone());
        self.pids.insert(name.clone(), pid.clone());
    }
    pub fn instance(&self, pid: String) -> Option<&Instance> {
        self.plugins.get(&pid)
    }
    pub fn name(&self, pid: String) -> Option<String> {
        self.names.get(&pid).cloned()
    }
    pub fn pid(&self, name: String) -> Option<String> {
        self.pids.get(&name).cloned()
    }
}

pub enum WasmRequest {
    Link {
        linker_fn: fn(&mut Linker<HostData>) -> Result<(), wasmtime::Error>,
        resp: oneshot::Sender<Result<(), wasmtime::Error>>,
    },
    LoadPlugin {
        path: PathBuf,
        resp: oneshot::Sender<Result<(String, String), String>>, // Result<(pid, name), error>
    },
    CallExport {
        pid: String,
        interface: String,
        func: String,
        params: Vec<Val>,
        default: Vec<Val>,
        resp: oneshot::Sender<Result<Vec<Val>, String>>,
    },
}

pub async fn init() -> Result<(), String> {
    println!("Initializing WASM module...");
    let actor = init_actor();
    ACTOR
        .set(Mutex::new(actor))
        .map_err(|_| "WASM actor already initialized".to_string())?;

    ui::init().await?;
    auth::init().await?;
    storage::init().await?;
    db::init().await?;
    plugins::init().await?;
    Ok(())
}

async fn actor() -> Result<MutexGuard<'static, WasmActor>, wasmtime::Error> {
    Ok(ACTOR
        .get()
        .ok_or_else(|| wasmtime::Error::msg("WASM actor handle not initialized".to_string()))?
        .lock()
        .await)
}

async fn plugin_call(
    pid: &String,
    interface: &str,
    func: &str,
    params: Vec<Val>,
    default: Vec<Val>,
) -> Result<Vec<Val>, String> {
    actor()
        .await
        .map_err(|e| e.to_string())?
        .call(&pid, interface, func, params, default)
        .await
}

async fn plugin_call_by_name(
    name: &String,
    interface: &str,
    func: &str,
    params: Vec<Val>,
    default: Vec<Val>,
) -> Result<Vec<Val>, String> {
    if let Some(pid) = plugin_pid(&name).await {
        plugin_call(&pid, interface, func, params, default).await
    } else {
        Err("Plugin name not found".to_string())
    }
}

async fn plugin_name(pid: String) -> Option<String> {
    actor().await.ok()?.name(pid)
}

async fn plugin_pid(name: &String) -> Option<String> {
    actor().await.ok()?.pid(name)
}

pub struct WasmActor {
    tx: mpsc::UnboundedSender<WasmRequest>,
    names: HashMap<String, String>,
    pids: HashMap<String, String>,
}
impl WasmActor {
    pub async fn load_plugin(&mut self, path: PathBuf) -> Result<String, String> /* Result<pid, error> */
    {
        let (resp_tx, resp_rx) = oneshot::channel();
        let req = WasmRequest::LoadPlugin {
            path,
            resp: resp_tx,
        };
        self.tx.send(req).map_err(|e| e.to_string())?;
        let (pid, name) = resp_rx.await.map_err(|e| e.to_string())??;
        self.names.insert(pid.clone(), name.clone());
        self.pids.insert(name.clone(), pid.clone());
        Ok(pid)
    }

    pub fn pid(&self, name: &String) -> Option<String> {
        self.pids.get(name).cloned()
    }

    pub fn name(&self, pid: String) -> Option<String> {
        self.names.get(&pid).cloned()
    }

    pub async fn call(
        &mut self,
        pid: &String,
        interface: &str,
        func: &str,
        params: Vec<Val>,
        default: Vec<Val>,
    ) -> Result<Vec<Val>, String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let req = WasmRequest::CallExport {
            pid: pid.clone(),
            interface: interface.to_string(),
            func: func.to_string(),
            params,
            default,
            resp: resp_tx,
        };
        self.tx.send(req).map_err(|e| e.to_string())?;
        resp_rx.await.map_err(|e| e.to_string())?
    }

    pub async fn link(
        &mut self,
        linker_fn: fn(&mut Linker<HostData>) -> Result<(), wasmtime::Error>,
    ) -> Result<(), wasmtime::Error> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let req = WasmRequest::Link {
            linker_fn,
            resp: resp_tx,
        };
        self.tx
            .send(req)
            .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
        resp_rx.await?
    }
}

fn init_actor() -> WasmActor {
    let (tx, mut rx) = mpsc::unbounded_channel::<WasmRequest>();
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");

        rt.block_on(async move {
            let mut config = Config::new();
            config.wasm_component_model(true);
            config.async_support(true);
            let engine = Engine::new(&config).unwrap();
            let wasi = WasiCtxBuilder::new().inherit_stdio().build();
            let http_ctx = WasiHttpCtx::new();
            let table = wasmtime_wasi::ResourceTable::new();
            let host = HostData {
                wasi,
                http_ctx,
                table,
                plugins: HashMap::new(),
                names: HashMap::new(),
                pids: HashMap::new(),
                schemas: HashMap::new(),
            };
            let mut store = Store::new(&engine, host);
            let mut linker: Linker<HostData> = Linker::new(&engine);
            wasmtime_wasi::p2::add_to_linker_async(&mut linker)
                .expect("Failed to add wasi to linker");
            wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)
                .expect("Failed to add wasi-http types to linker");

            while let Some(req) = rx.recv().await {
                match req {
                    WasmRequest::LoadPlugin { path, resp } => {
                        let _ =
                            resp.send(load_plugin(&engine, &mut linker, &mut store, path).await);
                    }

                    WasmRequest::Link { linker_fn, resp } => {
                        let _ = resp.send(linker_fn(&mut linker));
                    }
                    WasmRequest::CallExport {
                        pid,
                        interface,
                        func,
                        params,
                        mut default,
                        resp,
                    } => {
                        let _ = resp.send(
                            call(
                                &engine,
                                &mut store,
                                pid,
                                interface,
                                func,
                                params,
                                &mut default,
                            )
                            .await,
                        );
                    }
                }
            }
        })
    });
    WasmActor {
        tx,
        names: HashMap::new(),
        pids: HashMap::new(),
    }
}

async fn load_plugin(
    engine: &Engine,
    linker: &mut Linker<HostData>,
    mut store: &mut Store<HostData>,
    path: PathBuf,
) -> Result<(String, String), String> /* Result<(pid, name), error> */ {
    match Component::from_file(&engine, &path).map_err(|e| e.to_string()) {
        Ok(component) => match linker.instantiate_async(&mut store, &component).await {
            Ok(instance) => match path.file_stem().and_then(|s| s.to_str()) {
                Some(name) => {
                    let pid = uuid::Uuid::new_v4().to_string();
                    let data = store.data_mut();
                    data.plugins.insert(pid.clone(), instance);
                    data.names.insert(pid.clone(), name.to_string());
                    data.pids.insert(name.to_string(), pid.clone());
                    Ok((pid, name.to_string()))
                }
                None => Err("Invalid instance ID".to_string()),
            },
            Err(e) => Err(e.to_string()),
        },
        Err(e) => Err(e),
    }
}

async fn call(
    engine: &Engine,
    mut store: &mut Store<HostData>,
    pid: String,
    interface: String,
    func: String,
    params: Vec<Val>,
    default: &mut Vec<Val>,
) -> Result<Vec<Val>, String> {
    let instance = {
        let data = store.data();
        if let Some(i) = data.plugins.get(&pid).cloned() {
            i
        } else {
            return Err(format!("Instance {pid} not found"));
        }
    };
    let exports: Vec<String> = instance
        .instance_pre(&store)
        .component()
        .component_type()
        .exports(&engine)
        .map(|(name, _)| name.to_string())
        .collect();
    println!("Found exports: {:?}", exports);
    let inter = match exports.iter().find(|name| name.starts_with(&interface)) {
        Some(name) => match instance.get_export_index(&mut store, None, &name) {
            Some(inter) => inter,
            None => {
                return Err(format!("Interface {name} not found in {pid}"));
            }
        },
        None => {
            return Err(format!("Interface {interface} not found in {pid}"));
        }
    };
    let func_index = match instance.get_export_index(&mut store, Some(&inter), &func) {
        Some(func_index) => func_index,
        None => {
            return Err(format!(
                "Function {func} not found in interface {interface} in instance {pid}"
            ));
        }
    };
    if let Some(f) = instance.get_func(&mut store, func_index) {
        println!("Calling export: {pid}|{interface}|{func} with params: {params:?}");
        match f.call_async(&mut store, &params, default).await {
            Ok(_) => {
                println!("Result from {pid}|{interface}|{func}: {default:?}");
                let ret = default.clone();
                f.post_return_async(&mut store)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(ret)
            }
            Err(e) => {
                eprintln!(
                    "Error calling export {pid}|{interface}|{func}: {}",
                    e.to_string()
                );
                Err(e.to_string())
            }
        }
    } else {
        Err(format!(
            "Function instance {pid}|{interface}|{func} not found"
        ))
    }
}

// static ENGINE: OnceLock<Engine> = OnceLock::new();
// static STORE: OnceLock<Mutex<Store<HostData>>> = OnceLock::new();
// static PLUGINS: OnceLock<HashMap<String, Instance>> = OnceLock::new();

// struct WasmFunc {
//     func: wasmtime::component::Func,
// }
// impl WasmFunc {
//     async fn call(&self, params: &[Val], results: &mut [Val]) -> Result<(), wasmtime::Error> {
//         let store = STORE
//             .get()
//             .ok_or_else(|| wasmtime::Error::msg("Store not initialized"))
//             .and_then(|mutex| {
//                 mutex
//                     .lock()
//                     .map_err(|e| wasmtime::Error::msg(e.to_string()))
//             })?;
//         self.func.call_async(&mut store, params, results).await
//     }
// }

// pub fn get_instance_func(instance_id: String, interface: &str, func: &str) -> Option<WasmFunc> {
//     let runtime = WASM_HANDLE.get()?.lock().ok()?;
//     let instance = PLUGINS.get()?.get(&instance_id)?;
//     let mut store = STORE.get_mut()?.lock().ok()?;
//     let inter = instance.get_export_index(&mut store, None, interface)?;
//     let index = instance.get_export_index(&mut store, Some(&inter), func)?;
//     if let Some(f) = instance.get_func(&mut store, index) {
//         Some(WasmFunc { func: f })
//     } else {
//         None
//     }
// }

// pub async fn init() -> Result<(), String> {
//     let mut config = Config::new();
//     config.wasm_component_model(true);
//     config.async_support(true);
//     let engine = Engine::new(&config).map_err(|e| e.to_string())?;
//     ENGINE
//         .set(engine)
//         .map_err(|_| "Engine already set".to_string())?;
//     let wasi = WasiCtxBuilder::new().inherit_stdio().build();
//     let table = wasmtime_wasi::ResourceTable::new();
//     let data = HostData {
//         wasi,
//         table,
//         plugins: HashMap::new(),
//     };
//     let store = Store::new(&engine, data);
//     STORE
//         .set(Mutex::new(store))
//         .map_err(|_| "Store already set".to_string())?;
//     let mut linker: Linker<HostData> = Linker::new(&engine);
//     wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(|e| e.to_string())?;
//     WASM_HANDLE.set(Mutex::new(WasmRuntime {
//         engine,
//         store,
//         linker,
//     }));
//     ui::init()?;
//     plugins::init().await?;
//     Ok(())
// }

// pub async fn add_component(wasm: &mut WasmRuntime, path: PathBuf) -> Result<(), String> {
//     let component = Component::from_file(&wasm.engine, &path).map_err(|e| e.to_string())?;
//     let instance = wasm
//         .linker
//         .instantiate_async(&mut wasm.store, &component)
//         .await
//         .map_err(|e| e.to_string())?;
//     let instance_id = path
//         .file_stem()
//         .and_then(|s| s.to_str())
//         .unwrap_or("unknown")
//         .to_string();
//     wasm.store
//         .data_mut()
//         .plugins
//         .insert(instance_id.clone(), instance);
//     if let Some(interface) =
//         instance.get_export_index(&mut wasm.store, None, "pato:plugin/init@0.1.0")
//     {
//         println!("Found interface for pato:plugin/init@0.1.0: {interface:?}");
//         if let Some(init) = instance.get_export_index(&mut wasm.store, Some(&interface), "init") {
//             println!("Found init function: {init:?}");
//             if let Some(func) = instance.get_func(&mut wasm.store, init) {
//                 println!("Calling init func");
//                 set_current_instance(instance_id.clone(), &instance);
//                 func.call_async(&mut wasm.store, &[], &mut [])
//                     .await
//                     .map_err(|e| e.to_string())?;
//             }
//         }
//     }
//     Ok(())
// }

// fn add_to_linker(wasm: &mut WasmRuntime, instance: &Instance) -> wasmtime::Result<()> {
//     for (name, item) in instance
//         .instance_pre(&mut wasm.store)
//         .component()
//         .component_type()
//         .exports(&wasm.engine)
//     {
//         let mut linker_instance = wasm.linker.instance(name)?;
//         match item {
//             ComponentItem::Component(_component) => {
//                 println!("Component: {name}");
//                 // link_component()
//             }
//             ComponentItem::ComponentInstance(component_instance) => {
//                 println!("ComponentInstance: {name}");
//                 link_component_instance(
//                     &wasm.engine,
//                     &mut wasm.store,
//                     name,
//                     &instance,
//                     component_instance,
//                     &mut linker_instance,
//                 )?;
//                 // link_component_instance()
//             }
//             ComponentItem::ComponentFunc(_component_func) => {
//                 println!("ComponentFunc: {name}");
//                 link_component_func(&mut wasm.store, name, &instance, &mut linker_instance)?;
//                 // link_component_func()
//             }
//             _ => {
//                 println!("Other component item: {name}");
//             }
//         }
//     }
//     Ok(())
// }

// fn link_component_instance(
//     engine: &Engine,
//     store: &mut Store<HostData>,
//     instance_name: &str,
//     instance: &Instance,
//     component_instance: ComponentInstance,
//     linker_instance: &mut LinkerInstance<'_, HostData>,
// ) -> wasmtime::Result<()> {
//     for (name, component_item) in component_instance.exports(&engine) {
//         match component_item {
//             ComponentItem::ComponentInstance(component_instance) => {
//                 println!("ComponentInstance: {instance_name}::{name}");
//                 link_component_instance(
//                     engine,
//                     store,
//                     name,
//                     instance,
//                     component_instance,
//                     linker_instance,
//                 )?;
//             }
//             ComponentItem::ComponentFunc(_component_func) => {
//                 println!("ComponentFunc: {instance_name}::{name}");
//                 link_component_func(store, name, instance, linker_instance)?;
//             }
//             _ => println!("Other component item: {instance_name}::{name}"),
//         }
//     }
//     Ok(())
// }

// fn link_component_func(
//     store: &mut Store<HostData>,
//     name: &str,
//     instance: &Instance,
//     linker_instance: &mut LinkerInstance<'_, HostData>,
// ) -> wasmtime::Result<()> {
//     if let Some(func) = instance.get_func(store, name) {
//         linker_instance.func_new(name, move |mut store, _component_func, params, results| {
//             func.call(&mut store, params, results)
//         })?;
//     }
//     Ok(())
// }

// #[derive(Clone, Deserialize)]
// pub struct Def {
//     pub package: String,
//     pub wasm: PathBuf,
//     pub wit: PathBuf,
//     pub world: String,
//     pub deps: Vec<String>,
// }

// pub fn init_plugins(wasm: &mut Wasm, plugin_defs: Vec<Def>) -> Result<(), String> {
//     println!("Initializing plugins...");
//     // Get current working directory
//     let current_dir = std::env::current_dir().map_err(|e| e.to_string())?;
//     println!("CWD: {current_dir:?}");

//     // Get plugins directory
//     let plugins_dir = PathBuf::from("plugins");
//     if !plugins_dir.exists() {
//         println!("Creating missing plugins directly...");
//         std::fs::create_dir_all(&plugins_dir).map_err(|e| e.to_string())?;
//         return Ok(());
//     }

//     let sorted = sort_defs(plugin_defs)?;

//     for def in sorted.iter() {
//         let entry = plugins_dir.join(&def.file);
//         // Load WASM plugins
//         println!("Loading plugin: {:?}", entry);
//         let _ = wasm.add_component(&entry).map_err(|e| e.to_string())?;
//     }
//     Ok(())
// }

// fn sort_defs(defs: Vec<Def>) -> Result<Vec<Def>, String> {
//     let mut seen: HashSet<String> = HashSet::new();
//     let mut ordered: Vec<Def> = Vec::new();

//     let mut in_degree: HashMap<String, usize> = HashMap::new();
//     let mut graph: HashMap<String, Vec<String>> = HashMap::new();

//     for def in &defs {
//         in_degree.entry(def.package.clone()).or_insert(0);
//         graph
//             .entry(def.package.clone())
//             .or_insert_with(|| Vec::new());
//         for dep in &def.deps {
//             if defs.iter().find(|d| d.package == *dep).is_some() {
//                 graph
//                     .entry(dep.clone())
//                     .or_insert_with(Vec::new)
//                     .push(def.package.clone());
//                 *in_degree.entry(def.package.clone()).or_insert(0) += 1;
//             }
//         }
//     }

//     let mut queue: VecDeque<String> = in_degree
//         .iter()
//         .filter(|(_, &degree)| degree == 0)
//         .map(|(package, _)| package.clone())
//         .collect();

//     while let Some(current_package) = queue.pop_front() {
//         if let Some(def) = defs.iter().find(|d| d.package == current_package) {
//             ordered.push(def.clone());
//             seen.insert(current_package.clone());
//             if let Some(deps) = graph.get(&current_package) {
//                 for dep in deps {
//                     if let Some(degree) = in_degree.get_mut(dep) {
//                         *degree -= 1;
//                         if *degree == 0 {
//                             queue.push_back(dep.clone());
//                         }
//                     }
//                 }
//             }
//         }
//     }

//     if ordered.len() != defs.len() {
//         return Err("Circular dependency detected".to_string());
//     }
//     Ok(ordered)
// }
