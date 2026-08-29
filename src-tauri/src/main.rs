#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::OnceLock;

mod js;
mod wasm;

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

// Helper function to extract access token from URL fragment

#[tauri::command]
async fn pato_ready() -> Result<bool, String> {
    println!("Pato UI signaled ready");
    wasm::init().await?;
    Ok(true)
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            println!("Starting Pato...");
            let _ = APP_HANDLE.set(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pato_ready,
            wasm::ui::status_widget_clicked,
            js::js_response,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
