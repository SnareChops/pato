use crate::wasm;
use std::collections::HashMap;
use std::path::PathBuf;
use walkdir::WalkDir;
use wasmtime::component::Val;

/// A plugin's name is its `.wasm` file stem; it keys the plugin's asset root,
/// settings and cross-plugin calls. If two plugins claim the same name we can't
/// tell which is legitimate, so none of them load - the whole platform refuses
/// to start rather than picking a winner. Names are compared case-insensitively
/// and with `-`/`_` folded together so lookalikes can't slip past.
fn check_unique_names(files: &[PathBuf]) -> Result<(), String> {
    let mut seen: HashMap<String, Vec<&PathBuf>> = HashMap::new();
    for file in files {
        let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        seen.entry(wasm::canonical_plugin_name(stem)).or_default().push(file);
    }
    let conflicts: Vec<String> = seen
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .map(|(name, paths)| {
            let paths: Vec<_> = paths.iter().map(|p| p.display().to_string()).collect();
            format!("`{name}`: {}", paths.join(", "))
        })
        .collect();
    if conflicts.is_empty() {
        return Ok(());
    }
    Err(format!(
        "Refusing to load any plugins: duplicate plugin names ({})",
        conflicts.join("; ")
    ))
}

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
    check_unique_names(&wasm_files)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(p: &[&str]) -> Vec<PathBuf> {
        p.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn distinct_names_pass() {
        assert!(check_unique_names(&paths(&["a/pato_twitch.wasm", "b/pato_obs.wasm"])).is_ok());
    }

    #[test]
    fn duplicates_refuse_all_even_lookalikes() {
        let err = check_unique_names(&paths(&[
            "a/pato_twitch.wasm",
            "b/Pato-Twitch.wasm",
            "c/pato_obs.wasm",
        ]))
        .unwrap_err();
        assert!(err.contains("a/pato_twitch.wasm") && err.contains("b/Pato-Twitch.wasm"));
    }
}
