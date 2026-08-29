// use crate::plugin_call;
use crate::wasm;
use std::path::PathBuf;
use walkdir::WalkDir;
use wasmtime::component::Val;

// wasmtime::component::bindgen!("plugin" in "wit");

// static INSTANCES: Mutex<Vec<(Store<HostData>, Instance)>> = Mutex::new(Vec::new());

// fn new_wasi_instance(path: &PathBuf) -> Result<(Engine, Store<HostData>, Instance), String> {
//     let mut config = Config::new();
//     config.wasm_component_model(true);
//     let engine = Engine::new(&config).map_err(|e| e.to_string())?;
//     let wasi = WasiCtxBuilder::new().inherit_stdio().build();
//     let table = wasmtime_wasi::ResourceTable::new();
//     let host = HostData { wasi, table };
//     let mut store = Store::new(&engine, host);
//     let mut linker: Linker<HostData> = Linker::new(&engine);

//     // Add WASI support for plugins
//     wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(|e| e.to_string())?;
//     crate::ui::pato::ui::host::add_to_linker::<_, HasSelf<_>>(&mut linker, |plugin| plugin)
//         .map_err(|e| e.to_string())?;

//     let component = Component::from_file(&engine, path).map_err(|e| e.to_string())?;
//     let instance = linker
//         .instantiate(&mut store, &component)
//         .map_err(|e| e.to_string())?;
//     return Ok((engine, store, instance));
// }
pub async fn init() -> Result<(), String> {
    let current_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    println!("CWD: {current_dir:?}");

    let plugins_dir = PathBuf::from("plugins");
    if !plugins_dir.exists() {
        return Ok(());
    }
    let wasm_files: Vec<PathBuf> = WalkDir::new(&plugins_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("wasm"))
        .map(|entry| entry.path().to_path_buf())
        .collect();
    let mut actor = wasm::actor().await.map_err(|e| e.to_string())?;
    for file in wasm_files {
        let pid = actor.load_plugin(file).await?;
        let result = actor
            .call(
                &pid,
                "pato:plugin/init@0.1.0",
                "init",
                vec![Val::String(pid.clone())],
                vec![Val::Result(Ok(None))],
            )
            .await?;
        println!("Plugin {} init result: {:?}", pid, result);
    }
    Ok(())
}

// #[macro_export]
// macro_rules! plugin_call {
//     ($pid:ident :: $namespace:ident : $pkg:ident  / $iface:ident . $func:ident ( $($args:expr),* $(,)? ) -> $ret:expr) => {{
//         $crate::wasm::actor()
//             .await
//             .map_err(|e| e.to_string())?
//             .plugin_call(
//                 &$pid,
//                 stringify!($namespace:$pkg/$iface),
//                 stringify!($func),
//                 &[$(&$crate::wasm::map_arg(&$args)),*],
//                 &[$crate::wasm::map_ret::<$ret>()],
//             ).await
//     }};
//     ($pid:ident :: $($iface:ident)::+ . $func:ident ( $($args:expr),* $(,)? ) ) => {{
//         $wasm::actor()
//             .await
//             .map_err(|e| e.to_string())?
//             .plugin_call(
//                 &$pid,
//                 stringify!($($iface)::+),
//                 stringify!($func),
//                 &[$(&$wasm::map_arg(&$args)),*],
//                 &[],
//             ).await
//     }};
// }
