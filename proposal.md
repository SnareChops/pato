# Proposal: Local WASM Plugin Host Platform

## Overview
This document describes a proposed platform designed to consolidate and simplify
streaming-related tools into a unified, extensible application. The platform
acts as a local host that runs WebAssembly (WASM) plugins to connect with and
control various third-party APIs such as Twitch, Discord, Kick, OBS, and VTube
Studio. The goal is to reduce the number of separate programs streamers and
VTubers use, while providing a flexible plugin system that encourages community
development and cross-language contributions.

## Motivation
Streamers and VTubers often rely on many separate bots and applications, each
handling one or two narrow functions—such as chat moderation, overlay control,
or channel events. This results in high system overhead, complex configuration,
and duplicated connections to external APIs.

This project introduces a single local runtime that handles all third-party API
connections. Developers can then extend functionality through plugins, which
interoperate within a shared event and API system. By centralizing connection
management and messaging while delegating functionality to plugins, the
platform aims to simplify streaming workflows and improve performance.

### Key Goals
- **Unification**: Combine disparate bot and event applications into one
  extensible local tool.
- **Extensibility**: Support user-built plugins written in any language that
  compiles to WebAssembly.
- **Performance**: Reduce duplicate API connections and leverage WASM's
  lightweight execution model.
- **Safety**: Maintain strong isolation between plugins using WASM sandboxing.
- **Decentralization**: Provide a plugin discovery system without requiring a
  centralized proprietary store.

## Core Concept
The application functions as a **WASM host**, running on the user's machine and
loading plugins compiled as WASM components. Each plugin registers to receive
and emit events and can call functions exposed by the host or other plugins.

For example, a plugin might:
- Listen for `on_chat_message` events from Twitch or Discord.
- React to incoming messages by performing actions (e.g. responding to a
  command, triggering an OBS scene change, or sending an event that another
  plugin listens to).

The host manages event routing, plugin initialization, and lifecycle. All
third-party API connections (e.g., Twitch, Discord) are handled centrally in the
host, allowing multiple plugins to share one connection rather than maintaining
duplicate sessions.

## Why This Is Not "Just Another Streamer Bot"

Traditional streamer bots are typically monolithic applications built for a
single environment and specific functions, such as chat moderation or simple
automation. They often have rigid feature sets, require separate configuration,
and duplicate work already done by other tools.

This platform differs in several fundamental ways:

1. **It is a platform, not a single bot.**  
   The core host provides no streaming features by itself. All functionality is
   implemented through independent plugins. Users decide which plugins to load
   and compose their setups accordingly.

2. **It supports any language via WebAssembly.**  
   Plugin developers are free to use their preferred programming languages and
   tools. As long as the plugin compiles to a WASM component that satisfies the
   platform’s interface definitions, it can participate equally in the system.

3. **It unifies rather than duplicates.**  
   Instead of connecting multiple independent applications to the same external
   APIs, the host connects once and shares that data with all loaded plugins.
   This reduces bandwidth, CPU usage, and network complexity.

4. **It enables plugin-to-plugin communication.**  
   Because the platform manages a shared event system and exposes structured
   function interfaces, different plugins can collaborate dynamically—one can
   emit events or invoke another’s exported functions safely through WASM’s
   component model.

5. **It runs entirely locally and securely.**  
   All execution happens on the user’s device inside isolated WASM sandboxes.
   There is no cloud backend, proprietary account requirement, or external data
   dependency. Users maintain full control over both their data and environment.

In short, this is not another specialized bot, but a **local, extensible
framework** for building and combining many kinds of streaming tools.

## WebAssembly Overview

### What Is WebAssembly?
WebAssembly (WASM) is a portable binary instruction format designed for fast,
safe, and predictable execution across different environments. It allows code
compiled from various languages (e.g., Rust, Go, C, JavaScript, C#, Python) to
run efficiently using a sandboxed virtual machine.

### WASM Component Model
The Component Model standardizes how WASM modules interact with each other and
with their host environment. It uses **WebAssembly Interface Types (WIT)** to
define typed imports and exports between components. This makes language
interoperability first-class: any language with component model support can
build plugins that integrate seamlessly.

### Sandboxing and Security
According to the WebAssembly specification:

- Each module runs in its own **sandboxed linear memory**. One plugin cannot
  access another plugin’s memory.
- The execution environment exposes **only explicitly provided host functions**.
  If the host does not expose file or network access, the plugin cannot perform
  those actions.
- This isolation model prevents untrusted plugin code from interfering with the
  system or other plugins, while still enabling controlled communication through
  defined APIs.

## Credential Security and Privacy

One of the platform’s key design principles is that **plugins never have direct
access to user credentials or authentication tokens** for third‑party services.

When a user connects the host to an external API (such as Twitch or Discord),
the authentication process occurs entirely within the host. The resulting access
token is retained **only in the host’s memory** and is never written to disk or
transmitted externally. The plugin system is designed so that:

- Plugins **do not** see or handle any tokens or passwords.  
- Plugins **only** send and receive structured data (e.g., chat messages,
  events, or API responses) through the host’s exposed functions.  
- The host translates plugin requests into authenticated API calls using its own
  credentials, keeping authentication boundaries secure.

Because the platform executes entirely on the user’s machine, no credentials are
sent to a remote backend or cloud service. This ensures that even if a plugin is
malicious or poorly implemented, it cannot steal or misuse authentication data.

This separation of responsibility minimizes the security risk of third‑party
plugins and allows developers to focus on functionality without managing user
authentication directly.

## Architecture Overview

### Host Application
The host serves as the execution environment for all plugins. It is responsible
for:

- Initializing and managing plugin lifecycles.
- Handling connections to supported third-party APIs.
- Translating events and data from external services into a uniform internal
  event model.
- Dispatching these events to subscribed plugins.
- Exposing system functions that plugins can call (e.g., send messages, trigger
  actions, retrieve data).

The host will be implemented in **Rust**, leveraging the **Wasmtime** runtime
and **WASM Component Model** for structured interoperability.

### Plugins
Plugins are compiled WASM components that define how they interact with the
host. Each plugin specifies:

- **Imports** — Functions it expects the host (or other plugins) to provide.
- **Exports** — Handlers and functions it provides to respond to host events or
  to be called by other plugins.

Plugins are fully isolated from one another in memory and execution but may
communicate through explicit event publishing or by invoking exposed functions
on other plugins (subject to defined interfaces).

#### Example Conceptual Interface
A simplified excerpt of a possible WIT definition (not final):

```wit
interface platform {
  /// Send a message through a specific service (e.g. Twitch, Discord)
  send-message: func(service: string, channel: string, message: string)
}

interface plugin {
  /// Called when a new chat message is received
  on-chat-message: func(service: string, user: string, message: string)
}
```

The host implements `platform.send-message`, and plugins implement
`plugin.on-chat-message`. Plugins can be authored in any language with WIT
bindings and WASM component support.

### Event System
The host maintains a unified event bus. Every external trigger (such as messages
from APIs) and internal signal (e.g., plugin state change) is emitted as an
event. Plugins can subscribe to events of interest and may also publish their
own custom events.

This makes it possible for plugins to interoperate indirectly. For instance, an
“alert” plugin could emit an event when a subscription is detected, which a
“sound” plugin might handle to play an audio notification.

### Plugin Communication
While each plugin runs in its own sandbox, the host can facilitate controlled
communication by resolving imports/exports according to WIT definitions. A
plugin can declare optional dependencies on others, which the host can match and
connect dynamically if those plugins are loaded.

The exact mechanics of discovery and dependency resolution will evolve, but the
concept is that all communication flows through host-managed, well-typed
interfaces defined by the component model.

## User Interface Framework

The platform’s user interface will be built using **Tauri**, a lightweight and
secure framework for creating desktop applications using web technologies (HTML,
CSS, TypeScript, etc.) bridged with a Rust backend.

### Benefits of Using Tauri

- **Small Footprint**  
  Tauri applications are compact and efficient, using native system webviews
  instead of bundling an entire browser engine. This minimizes application size
  and resource consumption.

- **Security Model**  
  Tauri isolates the frontend from the backend using minimal and explicit API
  bridges. Combined with WebAssembly sandboxing for plugins, this ensures that
  user data and credentials remain protected within the local environment.

- **Local-First Architecture**  
  The application runs completely client-side. All credentials, event handling,
  and plugin management occur on the user’s machine with no dependency on remote
  servers or cloud infrastructure.

- **Modern UI Development**  
  The use of standard web technologies allows contributors to create rich,
  responsive interfaces and dashboards without requiring native GUI frameworks.
  The UI can evolve independently of the host core, supporting familiar
  front-end tooling and reactive frameworks.

### Plugin-Provided UI Extensions

Plugins will also be able to expose their own interface elements that appear
within the platform’s dashboard. This can take several forms:

- **Widgets** — Small UI panels, quick-action buttons, or simple data displays
  that can be placed on the main dashboard.  
- **Full Views** — Larger pages or configuration panels that provide deeper
  interaction or plugin-specific functionality.  
- **Interactive Controls** — Event-driven components that exchange data with the
  plugin over the host’s internal messaging layer.

The host mediates all communication between the UI and the corresponding plugin.
Plugins cannot directly execute frontend code; instead, they define a structured
UI description or request set (for example, through WIT-defined interfaces), and
the host renders it safely within the unified dashboard. This approach provides
flexibility for plugin authors while maintaining strong sandboxing and
consistent user experience.

Tauri’s Rust integration makes it possible to bridge the UI and plugin system
through well-defined backend APIs, ensuring both performance and safety.



## Plugin Library (Decentralized Distribution)
The platform will include a mechanism to discover and manage plugins. This
system will operate similarly to a library or registry—not a commercial store.

Plugin authors can register metadata describing their plugin and provide a
publicly accessible URL to the `.wasm` binary. Users can add or remove plugins
directly by reference, and the host can stream-download and load a plugin
on-the-fly without restarting.

Because WASM is a streaming format, a plugin can be loaded as it downloads,
allowing nearly instant activation. Versioning, integrity checks, and optional
indexing services will help maintain reliability without centralized control.

## Performance and Efficiency
Several aspects of the proposed design improve performance:

1. **Single-point API connections** — The host connects once to each third-party
   service. Multiple plugins can then listen to and send messages through that
   single connection.
2. **Low-overhead execution** — WASM provides near-native performance due to its
   compact binary format and JIT/AOT compilation strategies in runtimes such as
   Wasmtime.
3. **Parallelism and scalability** — Each plugin runs independently, allowing
   efficient scheduling and concurrency at the host level.
4. **Reduced resource usage** — A single unified host process consumes less
   memory and CPU compared to running multiple standalone bots.

## Browser-Executable Option
Because the entire platform depends only on WASM and client-side execution, a
future version could run fully in a browser. This would eliminate the need for
traditional installation while maintaining entirely local execution and data
ownership. The current design keeps this possibility open by not depending on
any system-level features unavailable in the web sandbox.

## Summary
This platform provides a foundation for a modular, high-performance, and
language-agnostic streaming assistant. By leveraging the WebAssembly Component
Model, developers can build plugins in their preferred languages while users
gain a unified, local, secure, and efficient environment.

---