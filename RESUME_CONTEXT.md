# Pato Project Resume Context

**Date Created**: November 12, 2025  
**Status**: Migrating from Debian 11 WSL2 to Ubuntu 24.04 WSL2 to resolve Tauri v2 dependency issues

## Project Overview

**Pato** is a Local WASM Plugin Host Platform - a unified, extensible application that runs WebAssembly plugins to connect with and control various third-party APIs (Twitch, Discord, Kick, OBS, VTube Studio). The goal is to replace multiple separate streaming tools with a single, secure, local platform.

### Key Architecture Goals
- **Tauri Desktop App**: Cross-platform UI using web technologies + Rust backend
- **Wasmtime Runtime**: For loading and executing WASM plugin components  
- **Plugin System**: Sandboxed WASM components for streaming functionality
- **Local Execution**: No cloud dependencies, all runs on user's machine
- **Security First**: Plugins never access credentials directly

## Current Project State

### Files Created
```
/home/snare/repos/pato/
├── proposal.md                     # Complete project specification
├── copilot-instructions.md         # Updated technical guidelines  
├── src-tauri/
│   ├── Cargo.toml                  # Tauri v2.5 configuration
│   ├── tauri.conf.json             # Tauri v2 app configuration
│   ├── src/main.rs                 # Basic Tauri v2 entry point
│   └── build.rs                    # Tauri build script
└── src-ui/
    └── index.html                  # Minimal HTML frontend
```

### Current File Contents

**src-tauri/Cargo.toml:**
```toml
[package]
name = "pato"
version = "0.1.0"
description = "Local WASM Plugin Host Platform"
authors = ["SnareChops"]
license = "MIT"
repository = "https://github.com/SnareChops/pato"
edition = "2021"

[build-dependencies]
tauri-build = { version = "2.5", features = [] }

[dependencies]
tauri = { version = "2.5", features = [] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

[features]
custom-protocol = ["tauri/custom-protocol"]
```

**src-tauri/src/main.rs:**
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            println!("🦆 Pato platform starting up...");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**src-tauri/tauri.conf.json:**
```json
{
  "$schema": "https://schema.tauri.app/config/2.0.0",
  "productName": "Pato",
  "version": "0.1.0",
  "identifier": "dev.snare.pato",
  "build": {
    "beforeDevCommand": "",
    "beforeBuildCommand": "",
    "frontendDist": "../src-ui"
  },
  "app": {
    "security": {
      "csp": null
    },
    "windows": [
      {
        "title": "Pato",
        "width": 800,
        "height": 600,
        "resizable": true,
        "fullscreen": false
      }
    ]
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

**src-ui/index.html:**
```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Pato - Local WASM Plugin Host Platform</title>
  <style>
    body {
      font-family: Arial, sans-serif;
      margin: 0;
      padding: 20px;
      background-color: #f5f5f5;
    }
    .container {
      max-width: 800px;
      margin: 0 auto;
      background: white;
      padding: 20px;
      border-radius: 8px;
      box-shadow: 0 2px 10px rgba(0, 0, 0, 0.1);
    }
  </style>
</head>
<body>
  <div class="container">
    <h1>🦆 Pato</h1>
    <p>Welcome to Pato - Your Local WASM Plugin Host Platform</p>
    <p>This is a minimal Tauri application. The platform is starting up...</p>
  </div>
</body>
</html>
```

## Problem Encountered

### Issue: Missing System Dependencies
**Environment**: Debian 11 (Bullseye) on WSL2  
**Error**: Tauri v2 requires newer system libraries not available in Debian 11:
- `libwebkit2gtk-4.1-dev` (only 4.0 available)
- `libgtk-4-dev` (only GTK-3 available)  
- `libsoup-3.0-dev` (only 2.4 available)
- Various GLib/GIO dependencies

**Build Errors**:
```
error: The system library `glib-2.0` required by crate `glib-sys` was not found
error: The system library `javascriptcoregtk-4.1` required by crate `javascriptcore-rs-sys` was not found
error: The system library `libsoup-3.0` required by crate `soup3-sys` was not found
```

### Solution: Migrate to Ubuntu 24.04 LTS
**Reason**: Ubuntu 24.04 LTS has all the required modern libraries for Tauri v2
**WSL2 Support**: Better WSLg integration than Debian

## Next Steps in Ubuntu 24.04

### 1. Environment Setup
```bash
# Install system dependencies (Ubuntu 24.04 has all these)
sudo apt update
sudo apt install -y curl build-essential libssl-dev pkg-config
sudo apt install -y libwebkit2gtk-4.1-dev libgtk-4-dev libsoup-3.0-dev librsvg2-dev patchelf

# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install Tauri CLI
cargo install tauri-cli@^2.0
```

### 2. Copy Project Files
Transfer all files from this directory to Ubuntu 24.04 environment

### 3. Test Basic Tauri v2 App
```bash
cd /path/to/pato
cargo tauri dev
```

### 4. Next Development Phases
1. **Basic GUI Working** - Get minimal Tauri app running
2. **Add Wasmtime Integration** - For WASM plugin loading
3. **Plugin Interface Design** - WIT definitions for plugin contracts
4. **API Client Framework** - Twitch/Discord/etc. integrations
5. **Plugin Sandbox System** - Security and isolation
6. **UI Framework** - Dashboard for plugin management

## Technical Stack (Confirmed)
- **Desktop Framework**: Tauri v2.5+ (Rust + HTML/CSS/JS)
- **WASM Runtime**: Wasmtime (for plugin execution)
- **Plugin Target**: `wasm32-wasip2` (WASI preview 2)
- **Plugin Bindings**: wit-bindgen v0.47.0+
- **UI Technologies**: HTML, CSS, TypeScript (via Tauri webview)

## Key Technical Insights from copilot-instructions.md
- **WIT Dependencies**: Use flat structure in `wit/deps/` with `generate_all`
- **WASI Version**: CLI v0.2.8 + additional interfaces
- **Component Model**: For type-safe plugin interfaces
- **Security Model**: Plugins never access credentials, host handles all API calls
- **Build Strategy**: Always use `--release` for WASM components

## Important Files to Reference
- `proposal.md` - Complete project vision and requirements
- `copilot-instructions.md` - Technical implementation guidelines
- Both files contain crucial context for development decisions

## Verification Commands
Once in Ubuntu 24.04, verify setup with:
```bash
# Check environment
uname -a
lsb_release -a
echo $DISPLAY

# Check dependencies
pkg-config --list-all | grep -E "(webkit|gtk-4|soup-3)"

# Test Rust setup
cargo --version
rustc --version

# Test Tauri CLI
cargo tauri --version
```

## Expected Outcome
After migration to Ubuntu 24.04, `cargo tauri dev` should successfully:
1. Build the Tauri v2 application
2. Open a desktop window via WSLg
3. Display the HTML interface with "🦆 Pato" welcome message
4. Print "🦆 Pato platform starting up..." to console

## Ready for Next Phase
Once basic app is running, we can begin implementing:
- Wasmtime integration for plugin loading
- WIT interface definitions for plugin contracts
- Plugin discovery and management system
- API client integrations for streaming services