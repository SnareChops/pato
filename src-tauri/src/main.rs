#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Emitter;
use wasmtime::component::{Component, Linker, Instance};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};

// Generate bindings for the plugin WIT interface
wasmtime::component::bindgen!({
    world: "plugin",
    path: "../plugin-ui/wit/world.wit",
});

// Global plugin storage with proper context
static PLUGIN_INSTANCES: Mutex<Vec<(Engine, Store<PluginHost>, Instance)>> = Mutex::new(Vec::new());

struct PluginHost {
    wasi: WasiCtx,
    table: wasmtime_wasi::ResourceTable,
}

impl WasiView for PluginHost {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
    
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable {
        &mut self.table
    }
}

// No host trait implementation needed for export-only interface

#[tauri::command]
fn handle_button_click(app: tauri::AppHandle) -> Result<(), String> {
    println!("Button clicked in Rust! Calling plugin...");
    
    // Call the plugin function
    match call_plugin_function() {
        Ok(result) => {
            let message = format!("Plugin returned: {}", result);
            println!("✅ {}", message);
            app.emit("button-clicked", message).map_err(|e| e.to_string())?;
        }
        Err(e) => {
            let error_msg = format!("Plugin call failed: {}", e);
            println!("❌ {}", error_msg);
            app.emit("button-clicked", error_msg).map_err(|e| e.to_string())?;
        }
    }
    
    Ok(())
}

fn call_plugin_function() -> Result<u32, Box<dyn std::error::Error>> {
    let mut instances = PLUGIN_INSTANCES.lock().unwrap();
    
    if instances.is_empty() {
        return Err("No plugins loaded".into());
    }
    
    // Get the first plugin instance
    let (_engine, store, instance) = &mut instances[0];
    
    // Create the plugin interface
    let plugin = Plugin::new(&mut *store, instance)?;
    
    // Call the actual get-number function from the WASM plugin
    println!("🔌 Calling real WASM plugin get-number() function...");
    let result = plugin.test().call_get_number(&mut *store)?;
    
    println!("📝 Plugin returned: {}", result);
    Ok(result)
}

fn load_wasm_plugins() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔌 Loading WASM plugins...");
    
    // Setup Wasmtime engine with component model support
    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;
    
    // Setup WASI context
    let wasi = WasiCtxBuilder::new().inherit_stdio().build();
    let table = wasmtime_wasi::ResourceTable::new();
    let host = PluginHost { wasi, table };
    let mut store = Store::new(&engine, host);
    
    // Setup component linker
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_sync(&mut linker)?;
    
    // No additional linker setup needed for export-only plugins
    
    // Get plugins directory path - debug current directory
    let current_dir = std::env::current_dir().unwrap();
    println!("🔍 Current working directory: {:?}", current_dir);
    
    // Try multiple possible plugin directory locations
    let possible_paths = vec![
        PathBuf::from("plugins"),  // This should work since we're in src-tauri dir
        PathBuf::from("src-tauri/plugins"),
        current_dir.join("plugins"),
        current_dir.join("src-tauri/plugins"),
        PathBuf::from("/home/snare/repos/pato/src-tauri/plugins"),
    ];
    
    let mut plugins_dir = None;
    for path in possible_paths {
        println!("🔍 Checking plugin path: {:?} - exists: {}", path, path.exists());
        if path.exists() {
            plugins_dir = Some(path);
            break;
        }
    }
    
    let plugins_dir = match plugins_dir {
        Some(dir) => dir,
        None => {
            println!("❌ No valid plugins directory found");
            return Ok(());
        }
    };
    
    println!("✅ Using plugins directory: {:?}", plugins_dir);
    
    if !plugins_dir.exists() {
        println!("📁 Plugins directory not found, creating: {:?}", plugins_dir);
        std::fs::create_dir_all(&plugins_dir)?;
        return Ok(());
    }
    
    // Scan for .wasm files
    let entries = std::fs::read_dir(&plugins_dir)?;
    let mut plugin_count = 0;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
            plugin_count += 1;
            println!("🔍 Found plugin: {:?}", path.file_name().unwrap());
            
            match load_plugin(&engine, &mut linker, &mut store, &path) {
                Ok(_) => println!("✅ Successfully loaded plugin: {:?}", path.file_name().unwrap()),
                Err(e) => println!("❌ Failed to load plugin {:?}: {}", path.file_name().unwrap(), e),
            }
        }
    }
    
    if plugin_count == 0 {
        println!("📁 No .wasm plugins found in {:?}", plugins_dir);
    } else {
        println!("🎉 Processed {} plugin(s)", plugin_count);
    }
    
    Ok(())
}

fn load_plugin(
    engine: &Engine,
    linker: &mut Linker<PluginHost>,
    store: &mut Store<PluginHost>,
    plugin_path: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    // Read and compile the component
    let component_bytes = std::fs::read(plugin_path)?;
    let component = Component::from_binary(engine, &component_bytes)?;
    
    // Instantiate the component
    let instance = linker.instantiate(store, &component)?;
    
    // Create a new engine/store for this plugin instance to store in global state
    // This allows us to call plugin functions later
    let mut new_config = Config::new();
    new_config.wasm_component_model(true);
    let new_engine = Engine::new(&new_config)?;
    
    let wasi = WasiCtxBuilder::new().inherit_stdio().build();
    let table = wasmtime_wasi::ResourceTable::new();
    let new_host = PluginHost { wasi, table };
    let mut new_store = Store::new(&new_engine, new_host);
    
    let mut new_linker = Linker::new(&new_engine);
    wasmtime_wasi::add_to_linker_sync(&mut new_linker)?;
    
    let new_component = Component::from_binary(&new_engine, &component_bytes)?;
    let new_instance = new_linker.instantiate(&mut new_store, &new_component)?;
    
    // Store the complete plugin context for later function calls
    let mut instances = PLUGIN_INSTANCES.lock().unwrap();
    instances.push((new_engine, new_store, new_instance));
    
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            println!("🦆 Pato platform starting up...");
            
            // Load WASM plugins on startup
            if let Err(e) = load_wasm_plugins() {
                println!("⚠️ Error loading plugins: {}", e);
            }
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![handle_button_click])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}