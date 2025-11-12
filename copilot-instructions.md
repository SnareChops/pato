# Copilot Instructions - Local WASM Plugin Host Platform (Pato)

## Project Overview
This is a **Local WASM Plugin Host Platform** built with Rust, Tauri, and Wasmtime. The platform serves as a unified, extensible application that runs WebAssembly (WASM) plugins to connect with and control various third-party APIs (Twitch, Discord, Kick, OBS, VTube Studio). The goal is to replace multiple separate streaming tools with a single, secure, local platform that supports community-developed plugins written in any language that compiles to WASM.

## Key Technical Stack
- **Core Language**: Rust
- **Desktop Framework**: Tauri (for cross-platform UI with web technologies)
- **WASM Runtime**: Wasmtime (for loading and executing plugins)
- **Plugin Target**: `wasm32-wasip2` (WASI preview 2)
- **Plugin Bindings**: wit-bindgen v0.47.0
- **WASI Version**: CLI v0.2.8 + additional interfaces (sockets, filesystem, etc.)
- **UI Technologies**: HTML, CSS, TypeScript/JavaScript (via Tauri)
- **Plugin Output**: WebAssembly Component (.wasm)
- **Host Output**: Native desktop application (Windows, macOS, Linux)

## Critical Project Insights

### WIT (WebAssembly Interface Types) Structure
- **Main Definition**: `wit/world.wit` - defines the component's imports and exports
- **Dependencies**: `wit/deps/` - contains all WASI interface definitions
- **Versioning**: MUST use versioned imports like `import wasi:cli/stdout@0.2.8`
- **NO root-level WIT folders** - all dependencies go in `wit/deps/`

### Correct WIT Syntax Patterns
```wit
// ✅ CORRECT - Versioned imports
import wasi:cli/stdout@0.2.8;
export wasi:cli/run@0.2.8;

// ❌ WRONG - Unversioned imports (old syntax)
use wasi:cli/stdout;
```

### Rust Implementation Patterns
- Use `wit_bindgen::generate!` macro with `generate_all: true`
- Generated bindings create crate-level modules (e.g., `wasi::cli::stdout`)
- NOT `bindings::` namespace - this is outdated documentation
- Implement the Guest trait for your exported interfaces

### WIT Dependencies Management - CRITICAL SETUP GUIDE

#### Official WASI WIT Sources
- **Primary Repository**: https://github.com/WebAssembly/wasi-cli
- **Version Used**: v0.2.8 (matches wit-bindgen v0.47.0)
- **Command to Clone**: `git clone --depth 1 --branch v0.2.8 https://github.com/WebAssembly/wasi-cli.git`

#### Required Directory Structure (MUST be exact)
```
wit/
├── world.wit                    # Your component world definition
└── deps/                        # All external dependencies (FLAT structure)
    ├── cli/                     # WASI CLI package
    │   ├── command.wit
    │   ├── imports.wit
    │   ├── run.wit
    │   ├── stdio.wit
    │   ├── deps.toml
    │   └── deps.lock
    ├── io/                      # WASI IO package (transitive dep)
    │   ├── streams.wit
    │   ├── poll.wit
    │   └── world.wit
    ├── filesystem/              # WASI Filesystem package
    ├── clocks/                  # WASI Clocks package
    ├── random/                  # WASI Random package
    └── sockets/                 # WASI Sockets package
```

#### ⚠️ CRITICAL: Dependency Structure Rules
1. **Flat Structure Required**: All WASI packages MUST be at `wit/deps/` level
2. **No Nesting**: Don't put transitive deps under `wit/deps/cli/deps/`
3. **Copy Transitive Dependencies**: Manual copy from `cli/deps/*` to `wit/deps/`
4. **Use `generate_all`**: Simplest approach for wit-bindgen configuration

#### Setup Commands (Copy-Paste Solution)
```bash
# From project root
cd wit/deps
git clone --depth 1 --branch v0.2.8 https://github.com/WebAssembly/wasi-cli.git temp_cli
cp -r temp_cli/wit cli
cp -r cli/deps/* .  # Copy transitive dependencies to flat structure
rm -rf temp_cli
```

#### wit-bindgen Configuration
```rust
wit_bindgen::generate!({
    world: "platform",
    path: "wit",
    generate_all,  // This is the key - generates all discovered interfaces
});
```

#### Lessons Learned from Dependency Resolution

**What Didn't Work:**
1. **Nested Dependencies**: Keeping transitive deps under `wit/deps/cli/deps/` - wit-bindgen couldn't find them
2. **Manual `with` Mappings**: Trying to explicitly map each interface led to complex, error-prone configuration
3. **Mixed Structures**: Combining custom WIT files with official WASI structures in wrong locations
4. **Version Mismatches**: Using different WASI versions across dependencies

**What Worked:**
1. **Flat Dependency Structure**: All WASI packages at `wit/deps/` level for easy discovery
2. **`generate_all` Parameter**: Let wit-bindgen automatically discover and generate all interfaces
3. **Official Sources**: Using exact official WASI CLI repository ensures compatibility
4. **Consistent Versioning**: All WASI interfaces use same version (v0.2.8)

**Key Insight:** wit-bindgen's dependency resolution is designed for flat structures, not hierarchical ones. The `generate_all` approach is more reliable than manual interface mapping for complex dependency trees.

#### Common Errors and Solutions
- **"package 'wasi:io@0.2.8' not found"** → Transitive deps not in flat structure
- **"interface or world `imports` not found"** → WASI CLI not properly copied
- **"missing `with` mapping for the key"** → Use `generate_all` instead of manual `with` mappings

## Working Code Patterns

### Cargo.toml Configuration
```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.47.0"
```

### WIT World Definition
```wit
package snare:platform@0.1.0;

world platform {
  import wasi:cli/stdout@0.2.8;
  export wasi:cli/run@0.2.8;
}
```

### Rust Implementation
```rust
wit_bindgen::generate!({
    world: "platform",
    path: "wit",
    generate_all,  // Note: no "true", just the bare identifier
});

use wasi::cli::stdout;
use wasi::filesystem::{preopens, types};

struct Platform;

impl exports::wasi::cli::run::Guest for Platform {
    fn run() -> Result<(), ()> {
        let out = stdout::get_stdout();
        let _ = out.write("🚀 Platform starting up...\n".as_bytes());
        let _ = out.flush();
        
        // Example filesystem access
        let preopens = preopens::get_directories();
        if let Some((root_dir, _)) = preopens.into_iter().next() {
            // Can access filesystem through root_dir
        }
        
        Ok(())
    }
}

export!(Platform);
```

## Build & Development Commands

### Platform Development (Main Application)
```bash
# Install Tauri CLI (if not already installed)
cargo install tauri-cli

# Development with hot-reload
cargo tauri dev

# Build production app for current platform
cargo tauri build

# Build for specific platforms (cross-compilation)
cargo tauri build --target x86_64-pc-windows-msvc     # Windows
cargo tauri build --target x86_64-apple-darwin        # macOS Intel
cargo tauri build --target aarch64-apple-darwin       # macOS Apple Silicon
cargo tauri build --target x86_64-unknown-linux-gnu   # Linux
```

### Plugin Development (WASM Components)
```bash
# Build plugin WASM component (ALWAYS use --release for minimal size)
cargo build --target wasm32-wasip2 --release

# Test plugin with wasmtime (standalone)
wasmtime target/wasm32-wasip2/release/my_plugin.wasm

# Inspect plugin component structure
wasm-tools component wit target/wasm32-wasip2/release/my_plugin.wasm

# Validate plugin interface compliance
wasm-tools validate target/wasm32-wasip2/release/my_plugin.wasm
```

### Project Structure Commands
```bash
# Initialize new plugin project
cargo new --lib my-plugin
cd my-plugin
# Add WASM target and configure Cargo.toml

# Test platform with example plugins
cargo tauri dev                                    # Start platform
# Load plugins through UI or API calls
```

### 🎯 **Build Strategy Notes**
- **Platform (Tauri)**: Native builds for each target OS, optimized for performance
- **Plugins (WASM)**: Always use `--release` for minimal size and optimal sandbox performance
- **Development**: Use `cargo tauri dev` for rapid iteration with hot-reload
- **Production**: Use `cargo tauri build` for optimized, signed, installable packages
- **Plugin Testing**: Standalone WASM testing with wasmtime before platform integration

## Common Pitfalls & Solutions

### WIT Dependency Errors (Most Common Issues)
- **"package 'wasi:cli' not found"** → Missing dependencies in `wit/deps/`
- **"package 'wasi:io@0.2.8' not found"** → Transitive dependencies not flattened to `wit/deps/` level
- **"interface or world `imports` not found"** → WASI CLI package structure incorrect
- **"could not find interface"** → Wrong import syntax, use versioned imports like `@0.2.8`
- **"missing `with` mapping for the key"** → Use `generate_all` instead of manual interface mapping

### Build Target Separation
- **Tauri Platform**: `cargo tauri build` (native target for desktop app)
- **WASM Plugins**: `cargo build --lib --target wasm32-wasip2 --release` (WASM components)
- **Never mix targets**: Platform uses native compilation with wasmtime to load WASM plugins
- **Plugin Development**: Separate Cargo projects for each plugin to avoid target conflicts

### Documentation Issues
- Official docs often lag behind bleeding-edge changes
- Preview 2 syntax differs significantly from preview 1
- wit-bindgen v0.47.0 behavior differs from online examples
- Rely on compiler errors to guide correct patterns - they're very helpful

### Successful Pattern Recognition
- **✅ Works**: Flat dependency structure in `wit/deps/`
- **✅ Works**: `generate_all` parameter for comprehensive binding generation
- **✅ Works**: Official WASI repositories as dependency sources
- **❌ Fails**: Nested dependency structures
- **❌ Fails**: Manual `with` mappings for complex dependency trees
- **❌ Fails**: Mixing different WASI versions

## Plugin Architecture & Platform Design

### Platform Components
The platform consists of three main components:
- **Tauri Host Application** (Rust backend + web frontend) - manages UI, plugins, and API connections
- **Plugin System** (Wasmtime runtime) - loads and executes WASM plugin components
- **Plugin Components** (.wasm files) - user-created functionality for streaming tools

### Plugin Interface Patterns
Plugins will implement standardized WIT interfaces for different categories:

```wit
// Core platform interface - available to all plugins
package pato:platform@0.1.0;

interface platform-api {
  // Event subscription
  subscribe-to-event: func(event-type: string) -> result<u32, string>;
  
  // API interactions (credentials handled by host)
  send-chat-message: func(service: string, channel: string, message: string) -> result<_, string>;
  
  // Inter-plugin communication
  emit-event: func(event-type: string, data: string);
}

// Plugin implementation interface
interface plugin {
  // Lifecycle
  init: func() -> result<_, string>;
  shutdown: func();
  
  // Event handling
  handle-event: func(event-type: string, data: string);
  
  // Optional UI extension
  get-ui-config: func() -> option<string>; // JSON UI description
}
```

### Security Model
- **Credential Isolation**: Plugins never access API tokens - host handles all authentication
- **WASM Sandboxing**: Each plugin runs in isolated memory with only explicit host function access
- **Permission System**: Plugins declare required capabilities (network, filesystem, etc.)
- **Local Execution**: No cloud dependencies - everything runs on user's machine

### Plugin Categories & Examples
- **Chat Moderators**: Filter messages, auto-moderation, custom commands
- **Stream Alerts**: Subscription notifications, donation handling, follower events
- **OBS Integration**: Scene switching, source control, automated recording
- **Social Media**: Cross-platform posting, Discord integration, Twitter updates
- **Analytics**: Stream statistics, audience insights, performance tracking

## Tauri Integration Patterns

### Project Structure
```
src/
├── main.rs              # Tauri app entry point
├── lib.rs               # Shared library code
├── plugin_system/       # WASM plugin runtime management
├── api_clients/         # Third-party API integrations (Twitch, Discord, etc.)
├── ui_bridge/           # Frontend-backend communication
└── security/           # Credential management and sandboxing

src-tauri/
├── Cargo.toml          # Tauri app dependencies
├── tauri.conf.json     # Tauri configuration
├── capabilities/       # Security capability definitions
└── icons/              # App icons

src-ui/                 # Frontend (HTML/CSS/JS/TS)
├── index.html
├── main.js
├── styles/
└── components/

plugins/                # Example/bundled plugins (WASM components)
├── chat-moderator.wasm
├── stream-alerts.wasm
└── obs-integration.wasm
```

### Tauri Configuration (`tauri.conf.json`)
```json
{
  "productName": "Pato",
  "identifier": "dev.snare.pato",
  "build": {
    "beforeBuildCommand": "",
    "beforeDevCommand": "",
    "devPath": "../src-ui",
    "distDir": "../src-ui"
  },
  "app": {
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": ["deb", "appimage", "nsis", "dmg"],
    "identifier": "dev.snare.pato",
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/icon.icns", "icons/icon.ico"]
  }
}
```

### Rust-Frontend Bridge Pattern
```rust
// src/ui_bridge.rs
use tauri::State;
use std::sync::Mutex;

#[derive(Default)]
pub struct AppState {
    pub loaded_plugins: Mutex<Vec<String>>,
    pub active_connections: Mutex<HashMap<String, ApiConnection>>,
}

#[tauri::command]
async fn load_plugin(plugin_path: String, state: State<'_, AppState>) -> Result<String, String> {
    // Load WASM plugin using wasmtime
    // Update state
    // Return status
}

#[tauri::command] 
async fn get_plugin_list(state: State<'_, AppState>) -> Vec<String> {
    // Return list of loaded plugins
}
```

## Debugging Tips
- Use `wasm-tools component wit` to inspect built components
- Check generated Rust code in `target/` for binding structure
- wasmtime provides good error messages for component issues
- Preview 2 is rapidly evolving - expect breaking changes

## Verified Working Configuration (November 2025)

### ✅ Fully Tested and Working
- **WASI CLI Integration**: Complete stdout, filesystem access, and component execution
- **Official WIT Dependencies**: Using verified v0.2.8 from WebAssembly/wasi-cli
- **Component Model**: Host-to-component execution with proper WASI binding
- **Plugin Discovery**: Filesystem-based plugin scanning and loading simulation
- **Build System**: Both WASM component and native host binary compile successfully
- **Runtime Execution**: End-to-end execution with proper console output

### 🏗️ Architecture Achievements
- **Type-Safe Interfaces**: WIT-enforced contracts between host and component
- **Standard Compliance**: Full alignment with Component Model preview 2 spec
- **Dependency Resolution**: Proper handling of transitive WASI dependencies
- **Development Workflow**: Reliable build-test-run cycle established

### 🚀 Ready for Next Phase
- Dynamic plugin loading with wasmtime integration
- Multi-component orchestration
- Advanced WASI interface exploration (sockets, async operations)
- Component-to-component communication patterns

### 📋 Configuration Validation Checklist
When setting up similar projects, verify:
- [ ] `wit/deps/` contains flattened WASI packages (not nested)
- [ ] All transitive dependencies copied to same level
- [ ] `generate_all` used in wit-bindgen configuration
- [ ] WASI CLI v0.2.8 matches wit-bindgen v0.47.0
- [ ] Separate build commands for WASM component vs host binary
- [ ] Release builds used for WASM components (smaller size)

## Quick Reference - Copy-Paste Commands

### Setup Fresh WIT Dependencies
```bash
# From project root
cd wit/deps
rm -rf * # Clear existing deps
git clone --depth 1 --branch v0.2.8 https://github.com/WebAssembly/wasi-cli.git temp_cli
cp -r temp_cli/wit cli
cp -r cli/deps/* .
rm -rf temp_cli
cd ../..
```

### Quick Development Workflow
```bash
# Start platform in development mode
cargo tauri dev

# Build plugin (from plugin directory)
cargo build --lib --target wasm32-wasip2 --release

# Install plugin through platform UI or copy to plugins/ directory

# Build production platform
cargo tauri build
```

### Troubleshooting Commands
```bash
# Check WIT structure
find wit -name "*.wit" | head -20

# Verify component structure
wasm-tools component wit target/wasm32-wasip2/release/platform.wasm

# Debug wit-bindgen (if needed)
WIT_BINDGEN_DEBUG=1 cargo build --lib --target wasm32-wasip2
```