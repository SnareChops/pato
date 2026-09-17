use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::thread;
use std::time::Duration;

use dashmap::DashMap;
use tauri::Emitter;
use tokio::sync::{mpsc, oneshot};

use wasmtime::component::{Component, Instance, Linker, Val};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpCtxView, WasiHttpView};

pub mod auth;
pub mod db;
pub mod log;
mod plugins;
pub mod storage;
pub mod ui;

/// How often the epoch ticker bumps the engine's epoch. Plugin call deadlines
/// are expressed as a number of these ticks.
const EPOCH_TICK_INTERVAL: Duration = Duration::from_millis(50);
/// Maximum time a single plugin export call is allowed to run before it is
/// forcibly trapped. This protects the rest of the app from a hung or
/// malicious plugin, since each plugin runs on its own dedicated thread.
const PLUGIN_CALL_TIMEOUT: Duration = Duration::from_secs(10);

fn plugin_call_timeout_ticks() -> u64 {
    (PLUGIN_CALL_TIMEOUT.as_millis() / EPOCH_TICK_INTERVAL.as_millis()) as u64
}

static ENGINE: OnceLock<Engine> = OnceLock::new();
static BASE_LINKER: OnceLock<StdMutex<Linker<HostData>>> = OnceLock::new();
static FROZEN_LINKER: OnceLock<Arc<Linker<HostData>>> = OnceLock::new();
static BROKER: OnceLock<Broker> = OnceLock::new();

fn engine() -> Engine {
    ENGINE.get().expect("WASM engine not initialized").clone()
}

fn broker() -> &'static Broker {
    BROKER.get().expect("WASM broker not initialized")
}

/// Routes calls to the dedicated thread that owns each loaded plugin's
/// `Store`. Holds no lock across an `.await` - `DashMap` guards are only ever
/// held long enough to clone out a cheap, already-`Send + Sync` handle.
struct Broker {
    names: DashMap<String, String>,                          // name -> pid
    channels: DashMap<String, mpsc::UnboundedSender<PluginRequest>>, // pid -> plugin thread
    roots: DashMap<String, PathBuf>,                         // name -> plugin directory
}

/// Host state private to a single plugin's `Store`. Every plugin gets its own
/// instance of this on its own thread, so `pid`/`name` are this plugin's true,
/// host-assigned identity
pub struct HostData {
    pid: String,
    name: String,
    wasi: WasiCtx,
    http_ctx: WasiHttpCtx,
    pub table: ResourceTable,
    schema: Vec<db::database::Store>,
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
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.http_ctx,
            table: &mut self.table,
            hooks: Default::default(),
        }
    }
}

enum PluginRequest {
    CallExport {
        interface: String,
        func: String,
        params: Vec<Val>,
        default: Vec<Val>,
        resp: oneshot::Sender<Result<Vec<Val>, String>>,
    },
}

pub async fn init() -> Result<(), String> {
    if ENGINE.get().is_some() {
        return Err("WASM subsystem already initialized".to_string());
    }
    println!("Initializing WASM module...");

    let mut config = Config::new();
    config.wasm_component_model(true);
    config.epoch_interruption(true);
    let engine = Engine::new(&config).map_err(|e| e.to_string())?;
    ENGINE
        .set(engine.clone())
        .map_err(|_| "WASM engine already initialized".to_string())?;

    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)
        .map_err(|e| e.to_string())?;
    wasmtime_wasi_http::p2::add_only_http_to_linker_async(&mut linker)
        .map_err(|e| e.to_string())?;
    BASE_LINKER
        .set(StdMutex::new(linker))
        .map_err(|_| "WASM linker already initialized".to_string())?;

    BROKER
        .set(Broker {
            names: DashMap::new(),
            channels: DashMap::new(),
            roots: DashMap::new(),
        })
        .map_err(|_| "WASM broker already initialized".to_string())?;

    spawn_epoch_ticker(engine);

    ui::init()?;
    auth::init()?;
    storage::init()?;
    db::init()?;
    freeze_linker()?;
    plugins::init().await?;
    Ok(())
}

fn spawn_epoch_ticker(engine: Engine) {
    thread::spawn(move || loop {
        thread::sleep(EPOCH_TICK_INTERVAL);
        engine.increment_epoch();
    });
}

/// Registers host functions on the shared linker template. Must only be
/// called during startup, before any plugin has been loaded - see
/// `freeze_linker`.
pub fn link(f: impl FnOnce(&mut Linker<HostData>) -> Result<(), wasmtime::Error>) -> Result<(), String> {
    let mutex = BASE_LINKER
        .get()
        .ok_or_else(|| "WASM linker not initialized".to_string())?;
    let mut linker = mutex
        .lock()
        .map_err(|_| "WASM linker lock poisoned".to_string())?;
    f(&mut linker).map_err(|e| e.to_string())
}

/// Freezes the linker template so it can be shared (read-only, no locking)
/// across every plugin's dedicated thread.
fn freeze_linker() -> Result<(), String> {
    let mutex = BASE_LINKER
        .get()
        .ok_or_else(|| "WASM linker not initialized".to_string())?;
    let linker = mutex
        .lock()
        .map_err(|_| "WASM linker lock poisoned".to_string())?
        .clone();
    FROZEN_LINKER
        .set(Arc::new(linker))
        .map_err(|_| "WASM linker already frozen".to_string())?;
    Ok(())
}

fn frozen_linker() -> Arc<Linker<HostData>> {
    FROZEN_LINKER
        .get()
        .expect("WASM linker not frozen yet")
        .clone()
}

pub fn plugin_pid(name: &str) -> Option<String> {
    broker().names.get(name).map(|e| e.value().clone())
}

/// The on-disk directory a plugin was loaded from. Assets referenced by the
/// plugin's custom widgets (`pato-asset://<name>/...`) are resolved relative to
/// this directory's `assets/` subfolder.
pub fn plugin_root(name: &str) -> Option<PathBuf> {
    broker().roots.get(name).map(|e| e.value().clone())
}

async fn plugin_call(
    pid: &str,
    interface: &str,
    func: &str,
    params: Vec<Val>,
    default: Vec<Val>,
) -> Result<Vec<Val>, String> {
    let tx = broker()
        .channels
        .get(pid)
        .map(|e| e.value().clone())
        .ok_or_else(|| format!("Unknown plugin id {pid}"))?;
    let (resp_tx, resp_rx) = oneshot::channel();
    tx.send(PluginRequest::CallExport {
        interface: interface.to_string(),
        func: func.to_string(),
        params,
        default,
        resp: resp_tx,
    })
    .map_err(|_| format!("Plugin {pid} is no longer running"))?;
    resp_rx
        .await
        .map_err(|_| format!("Plugin {pid} dropped the response channel"))?
}

async fn plugin_call_by_name(
    name: &str,
    interface: &str,
    func: &str,
    params: Vec<Val>,
    default: Vec<Val>,
) -> Result<Vec<Val>, String> {
    let pid = plugin_pid(name).ok_or_else(|| "Plugin name not found".to_string())?;
    plugin_call(&pid, interface, func, params, default).await
}

/// Compiles and loads a plugin onto its own dedicated OS thread, with its own
/// `Store` (and therefore its own WASI context and resource table). Returns
/// once the plugin has been instantiated and is ready to receive calls.
pub async fn load_plugin(path: PathBuf) -> Result<String, String> {
    let engine = engine();
    let component = Component::from_file(&engine, &path).map_err(|e| e.to_string())?;
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "Invalid plugin filename".to_string())?
        .to_string();
    let root = path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let pid = uuid::Uuid::new_v4().to_string();
    let linker = frozen_linker();

    let (tx, rx) = mpsc::unbounded_channel::<PluginRequest>();
    let (ready_tx, ready_rx) = oneshot::channel::<Result<(), String>>();

    thread::spawn({
        let pid = pid.clone();
        let name = name.clone();
        move || run_plugin_thread(engine, linker, component, pid, name, rx, ready_tx)
    });

    ready_rx
        .await
        .map_err(|_| "Plugin thread exited before finishing setup".to_string())??;

    broker().channels.insert(pid.clone(), tx);
    broker().roots.insert(name.clone(), root);
    broker().names.insert(name, pid.clone());
    Ok(pid)
}

fn run_plugin_thread(
    engine: Engine,
    linker: Arc<Linker<HostData>>,
    component: Component,
    pid: String,
    name: String,
    mut rx: mpsc::UnboundedReceiver<PluginRequest>,
    ready: oneshot::Sender<Result<(), String>>,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build plugin runtime");

    rt.block_on(async move {
        let wasi = WasiCtxBuilder::new().inherit_stdio().build();
        let http_ctx = WasiHttpCtx::new();
        let table = ResourceTable::new();
        let host = HostData {
            pid: pid.clone(),
            name: name.clone(),
            wasi,
            http_ctx,
            table,
            schema: Vec::new(),
        };
        let mut store = Store::new(&engine, host);
        store.epoch_deadline_trap();
        // A store's epoch deadline defaults to "already elapsed" - it must be
        // set before any guest code runs, including instantiation itself, or
        // wasmtime traps immediately.
        store.set_epoch_deadline(plugin_call_timeout_ticks());

        let instance = match linker.instantiate_async(&mut store, &component).await {
            Ok(instance) => instance,
            Err(e) => {
                let _ = ready.send(Err(e.to_string()));
                return;
            }
        };
        if ready.send(Ok(())).is_err() {
            // Nobody is waiting for this plugin anymore.
            return;
        }

        while let Some(req) = rx.recv().await {
            match req {
                PluginRequest::CallExport {
                    interface,
                    func,
                    params,
                    mut default,
                    resp,
                } => {
                    store.set_epoch_deadline(plugin_call_timeout_ticks());
                    let outcome = call_export(
                        &engine,
                        &mut store,
                        &instance,
                        &interface,
                        &func,
                        &params,
                        &mut default,
                    )
                    .await;
                    let outcome = outcome.map_err(|e| {
                        let message = if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::Interrupt) {
                            format!(
                                "Plugin call {interface}|{func} timed out after {:?}",
                                PLUGIN_CALL_TIMEOUT
                            )
                        } else {
                            e.to_string()
                        };
                        notify_plugin_error(&pid, &name, &interface, &func, &message);
                        message
                    });
                    // A WIT `result::err` returned by the export is a *successful*
                    // wasmtime call, so it never reaches the branch above. Surface
                    // it here so plugin-reported failures aren't silently dropped.
                    if let Ok(vals) = &outcome {
                        if let Some(message) = result_err_message(vals) {
                            notify_plugin_error(&pid, &name, &interface, &func, &message);
                        }
                    }
                    let _ = resp.send(outcome);
                }
            }
        }
    })
}

/// Extracts the message from the first `result::err(...)` in an export's return
/// values, if any. Used to log WIT-level errors that a plugin returns explicitly.
fn result_err_message(vals: &[Val]) -> Option<String> {
    vals.iter().find_map(|val| match val {
        Val::Result(Err(payload)) => Some(match payload.as_deref() {
            Some(Val::String(s)) => s.clone(),
            Some(other) => format!("{other:?}"),
            None => "plugin returned an error result".to_string(),
        }),
        _ => None,
    })
}

fn notify_plugin_error(pid: &str, name: &str, interface: &str, func: &str, message: &str) {
    eprintln!("Plugin {name} ({pid}) error in {interface}|{func}: {message}");
    if let Some(handle) = crate::APP_HANDLE.get() {
        let _ = handle.emit(
            "plugin-error",
            serde_json::json!({
                "pid": pid,
                "name": name,
                "interface": interface,
                "func": func,
                "message": message,
            }),
        );
    }
}

async fn call_export(
    engine: &Engine,
    store: &mut Store<HostData>,
    instance: &Instance,
    interface: &str,
    func: &str,
    params: &[Val],
    default: &mut [Val],
) -> Result<Vec<Val>, wasmtime::Error> {
    let exports: Vec<String> = instance
        .instance_pre(&store)
        .component()
        .component_type()
        .exports(engine)
        .map(|(name, _)| name.to_string())
        .collect();
    let interface_name = exports
        .iter()
        .find(|name| name.starts_with(interface))
        .ok_or_else(|| wasmtime::Error::msg(format!("Interface {interface} not found")))?;
    let inter = instance
        .get_export_index(&mut *store, None, interface_name)
        .ok_or_else(|| wasmtime::Error::msg(format!("Interface {interface_name} not found")))?;
    let func_index = instance
        .get_export_index(&mut *store, Some(&inter), func)
        .ok_or_else(|| {
            wasmtime::Error::msg(format!(
                "Function {func} not found in interface {interface}"
            ))
        })?;
    let f = instance
        .get_func(&mut *store, func_index)
        .ok_or_else(|| wasmtime::Error::msg(format!("Function {func} not found")))?;

    f.call_async(&mut *store, params, default).await?;
    Ok(default.to_vec())
}
