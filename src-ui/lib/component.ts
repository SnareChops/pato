import { applyElementArgs, Element, ElementArg, EventHandler } from "./html.js";

export class Component extends HTMLElement {
  static get tag() {
    return this.name
      .replace(/([A-Z])/g, "-$1")
      .toLowerCase()
      .replace(/^-/, "");
  }
  #handlers = new Map<string, EventHandler[]>();

  constructor(...args: ElementArg[]) {
    super();
    const handlers = applyElementArgs(this, ...args);
    for (const key in handlers) {
      this.#handlers.set(key, handlers[key]);
    }
  }

  setStyle(styles: Partial<CSSStyleDeclaration>) {
    for (const [key, value] of Object.entries(styles)) {
      // @ts-ignore
      this.style[key] = value;
    }
  }

  append(...nodes: (string | Node | Element)[]) {
    super.append(...nodes.map((x) => (x instanceof Element ? x.el() : x)));
  }

  on(event: string, callback: EventListenerOrEventListenerObject) {
    this.addEventListener(event, callback, false);
  }
}
/** Decorator to define a web component */
export function component<T extends typeof Component>(element: T, options?: ElementDefinitionOptions): T {
  console.log("Defining component:", element.tag);
  customElements.define(element.tag, element, options);
  return element;
}
