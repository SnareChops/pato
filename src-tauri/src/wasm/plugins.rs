use crate::wasm;
use std::path::PathBuf;
use walkdir::WalkDir;
use wasmtime::component::Val;

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
    for file in wasm_files {
        let pid = wasm::load_plugin(file).await?;
        let result = wasm::plugin_call(
            &pid,
            "pato:plugin/init@0.1.0",
            "init",
            vec![],
            vec![Val::Result(Ok(None))],
        )
        .await?;
        println!("Plugin {} init result: {:?}", pid, result);
    }
    Ok(())
}
