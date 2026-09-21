import { callRust } from "../lib/bindings.js";
import { component, Component } from "../lib/component.js";
import { WIDGET_BASE_CSS, styleToCss } from "../lib/widget-style.js";
import type { Node as WNode, Element as WElement, EventKind } from "pato:internal/widget-dom@0.1.0";

/** Allowed DOM event types */
const EVENT_TYPE: Record<EventKind, string> = {
  click: "click",
  input: "input",
  change: "change",
  submit: "submit",
  focus: "focus",
  blur: "blur",
  "enter-key": "keydown",
};

/** Allowed attribute names */
const MANAGED_ATTRS = ["href", "src", "alt", "title", "name", "placeholder", "rows", "min", "max", "step", "type"];

/** Pato listeners attached to a DOM node, so they can be torn off on re-render. */
const listeners = new WeakMap<Element, Array<[string, EventListener]>>();

/**
 * A scrollable element within this many pixels of its bottom counts as "at
 * the bottom" for the sticky-scroll behavior below.
 */
const STICK_TO_BOTTOM_PX = 24;

function isNearBottom(el: HTMLElement): boolean {
  return el.scrollHeight - el.scrollTop - el.clientHeight <= STICK_TO_BOTTOM_PX;
}

type Payload = { tag: "none" } | { tag: "text"; val: string } | { tag: "toggled"; val: boolean };

function extractPayload(kind: EventKind, dom: Element): Payload {
  if (kind === "input" || kind === "change") {
    if (dom instanceof HTMLInputElement && (dom.type === "checkbox" || dom.type === "radio")) {
      return { tag: "toggled", val: dom.checked };
    }
    if (dom instanceof HTMLInputElement || dom instanceof HTMLTextAreaElement || dom instanceof HTMLSelectElement) {
      return { tag: "text", val: dom.value };
    }
  }
  return { tag: "none" };
}

/**
 * Host element for a plugin-authored custom widget. Renders a flat node table
 * (pato:internal/widget-dom) into its shadow root and forwards whitelisted interactions to the
 * core. Keyed elements and value-slots keep their DOM identity across renders.
 */
export class PatoCustomWidget extends Component {
  #root: HTMLDivElement;
  #keyed = new Map<string, HTMLElement>();
  #slots = new Map<string, HTMLSpanElement>();
  widgetId = "";

  constructor() {
    super();
    const shadow = this.attachShadow({ mode: "open" });
    const style = document.createElement("style");
    style.textContent = WIDGET_BASE_CSS;
    this.#root = document.createElement("div");
    this.#root.className = "pato-root";
    shadow.append(style, this.#root);
  }

  /** Replace the widget contents with a new node table (node 0 = root). */
  render(nodes: WNode[]) {
    const root = nodes[0];
    if (!root || root.kind.tag !== "element") {
      this.#root.replaceChildren();
      return;
    }

    // parent index -> child indices, in ascending (render) order
    const children = new Map<number, number[]>();
    for (let i = 1; i < nodes.length; i++) {
      const parent = nodes[i].parent ?? -1;
      let bucket = children.get(parent);
      if (!bucket) children.set(parent, (bucket = []));
      bucket.push(i);
    }

    const keyed = new Map<string, HTMLElement>();
    const slots = new Map<string, HTMLSpanElement>();
    // Elements to pin to their bottom edge once the whole tree is back in
    // the live document - see the comment below `stickToBottom` collects
    // into, and why the write can't happen during `#build`.
    const stickyBottom: HTMLElement[] = [];

    this.#applyElement(this.#root, root.kind.val);
    // `.pato-root` is always a scroll container (base CSS), but it only
    // *follows* new content when the plugin opts in with `sticky: bottom` -
    // most widgets' root has nothing worth auto-following. Measured before
    // mutating children.
    if (root.kind.val.style?.sticky === "bottom" && isNearBottom(this.#root)) {
      stickyBottom.push(this.#root);
    }
    this.#root.replaceChildren(
      ...(children.get(0) ?? []).map((ci) => this.#build(ci, nodes, children, keyed, slots, stickyBottom)),
    );
    // `replaceChildren` builds a DocumentFragment from its arguments, which
    // detaches-then-reattaches even an already-current, reused child (ours,
    // via the keyed map) - and browsers reset `scrollTop` to 0 across any
    // disconnect, even a same-tick one. So a `scrollTop` write made *during*
    // `#build`, while a sticky element's own ancestor chain up to `#root`
    // hasn't finished being reattached, gets silently clobbered back to 0 a
    // moment later ("jumps to top" on every update). Deferring every write
    // to here - strictly after the one `replaceChildren` call that could
    // still move things - is the fix: nothing gets reattached after this
    // point during this render, so the write sticks.
    for (const el of stickyBottom) el.scrollTop = el.scrollHeight;

    this.#keyed = keyed;
    this.#slots = slots;
  }

  /** Cheap update of one value-slot without a re-render. */
  setValue(slot: string, value: string) {
    const span = this.#slots.get(slot);
    if (span) span.textContent = value;
  }

  #build(
    i: number,
    nodes: WNode[],
    children: Map<number, number[]>,
    keyed: Map<string, HTMLElement>,
    slots: Map<string, HTMLSpanElement>,
    stickyBottom: HTMLElement[],
  ): ChildNode {
    const node = nodes[i];
    switch (node.kind.tag) {
      case "text":
        return document.createTextNode(node.kind.val);
      case "value-slot": {
        const name = node.kind.val;
        const span = this.#slots.get(name) ?? document.createElement("span");
        span.dataset.slot = name;
        slots.set(name, span);
        return span;
      }
      case "element": {
        const el = node.kind.val;
        let dom = el.key ? this.#keyed.get(el.key) : undefined;
        if (!dom || dom.tagName.toLowerCase() !== el.tag) dom = document.createElement(el.tag);
        this.#applyElement(dom, el);
        // `sticky: bottom` opts a specific scrollable element into the same
        // bottom-pinning as `.pato-root` - see `render()`. Only meaningful
        // for a *reused* keyed node; a freshly created one has nothing to
        // preserve. The actual `scrollTop` write is deferred - see `render()`.
        if (el.style?.sticky === "bottom" && isNearBottom(dom)) stickyBottom.push(dom);
        dom.replaceChildren(
          ...(children.get(i) ?? []).map((ci) => this.#build(ci, nodes, children, keyed, slots, stickyBottom)),
        );
        if (el.key) keyed.set(el.key, dom);
        return dom;
      }
    }
  }

  #applyElement(dom: HTMLElement, el: WElement) {
    this.#applyAttrs(dom, el);
    dom.style.cssText = styleToCss(el.style);
    this.#applyEvents(dom, el);
  }

  #applyAttrs(dom: HTMLElement, el: WElement) {
    const attrs = new Set<string>();
    let hasDisabled = false;

    for (const a of el.attrs) {
      switch (a.tag) {
        case "href":
          if (dom.tagName === "A") (dom.setAttribute("href", a.val), attrs.add("href"));
          break;
        case "src":
          dom.setAttribute("src", a.val), attrs.add("src");
          break;
        case "alt":
          dom.setAttribute("alt", a.val), attrs.add("alt");
          break;
        case "title":
          dom.setAttribute("title", a.val), attrs.add("title");
          break;
        case "name":
          dom.setAttribute("name", a.val), attrs.add("name");
          break;
        case "placeholder":
          dom.setAttribute("placeholder", a.val), attrs.add("placeholder");
          break;
        case "rows":
          dom.setAttribute("rows", String(a.val)), attrs.add("rows");
          break;
        case "min":
          dom.setAttribute("min", String(a.val)), attrs.add("min");
          break;
        case "max":
          dom.setAttribute("max", String(a.val)), attrs.add("max");
          break;
        case "step":
          dom.setAttribute("step", String(a.val)), attrs.add("step");
          break;
        case "kind":
          dom.setAttribute("type", a.val), attrs.add("type");
          break;
        case "value":
          // `value`/`checked` are set only when the plugin sends them, so an
          // uncontrolled input keeps whatever the user typed across renders.
          (dom as HTMLInputElement).value = a.val;
          break;
        case "checked":
          (dom as HTMLInputElement).checked = a.val;
          break;
        case "disabled":
          (dom as HTMLInputElement).disabled = a.val;
          hasDisabled = true;
          break;
      }
    }

    for (const name of MANAGED_ATTRS) {
      if (!attrs.has(name) && dom.hasAttribute(name)) dom.removeAttribute(name);
    }
    if (!hasDisabled && "disabled" in dom) (dom as HTMLInputElement).disabled = false;
  }

  #applyEvents(dom: HTMLElement, el: WElement) {
    for (const [type, fn] of listeners.get(dom) ?? []) dom.removeEventListener(type, fn);

    const added: Array<[string, EventListener]> = [];
    for (const binding of el.events) {
      const type = EVENT_TYPE[binding.kind];
      const fn: EventListener = (ev) => {
        if (binding.kind === "enter-key" && (ev as KeyboardEvent).key !== "Enter") return;
        void callRust("custom_widget_event", {
          widgetId: this.widgetId,
          // The firing *binding's* handler, not the element's own `key` -
          // an element can carry several bindings (e.g. `input` to track
          // live text and `enter-key` to submit) that need to be told apart
          // on the way back. `key` is reconciliation identity only.
          nodeKey: binding.handler,
          kind: binding.kind,
          payload: extractPayload(binding.kind, dom),
        });
      };
      dom.addEventListener(type, fn);
      added.push([type, fn]);
    }
    listeners.set(dom, added);
  }
}

export default component(PatoCustomWidget);
