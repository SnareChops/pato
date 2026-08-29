/*
 * A very simple dom creation lib
 * https://github.com/SnareChops/html
 * See documentation for usage.
 */

type Appendable = string | Element | HTMLElement | Text;
export type EventHandler = (event: Event) => any;
type Attributes = Record<string, string | EventHandler>;
export type ElementArg = Attributes | Appendable;

export class Element {
  #el: HTMLElement;
  #handlers = new Map<string, EventHandler[]>();

  constructor(tag: string | HTMLElement, ...args: ElementArg[]) {
    if (tag instanceof HTMLElement) this.#el = tag;
    else this.#el = document.createElement(tag);
    const h = applyElementArgs(this.#el, ...args);
    for (const key in h) {
      this.#handlers.set(key, h[key]);
    }
  }

  id(): string;
  id(id: string): this;
  id(id?: string): this | string {
    if (!!id) {
      this.#el.id = id;
      return this;
    }
    return this.#el.id;
  }

  class(): string;
  class(name: string): this;
  class(name?: string): this | string {
    if (!!name) {
      this.#el.className = name;
      return this;
    }
    return this.#el.className;
  }

  attr(): Record<string, string>;
  attr(attrs: Attributes): this;
  attr(attrs?: Attributes): this | Record<string, string> {
    if (!!attrs) {
      for (const key in attrs) {
        if (key == "id") {
          if (typeof attrs[key] === "string") this.id(attrs[key]);
          continue;
        }
        if (key == "class") {
          if (typeof attrs[key] === "string") this.class(attrs[key]);
          continue;
        }
        if (typeof attrs[key] === "function") return this.on(key, attrs[key]);
        if (attrs[key] === null || attrs[key] === undefined)
          this.#el.removeAttribute(key);
        else this.#el.setAttribute(key, attrs[key]);
      }
      return this;
    }
    const result: Record<string, string> = {
      ...(this.id() ? { id: this.id() } : {}),
      ...(this.class() ? { class: this.class() } : {}),
    };
    for (const attr of this.#el.attributes) {
      result[attr.name] = attr.value;
    }
    return result;
  }

  clear() {
    if (this.#el.childNodes)
      for (const child of this.#el.childNodes) {
        this.#el.removeChild(child);
      }
  }

  text(value: string): this {
    if (!value) return this;
    this.#el.append(document.createTextNode(value));
    return this;
  }

  append(...elements: (Element | HTMLElement | Text)[]): this {
    for (const e of elements) {
      if (e instanceof Element) this.#el.append(e.el());
      else this.#el.append(e);
    }
    return this;
  }

  on(event: string, handler: EventHandler | undefined): this {
    // If undefined, clear all handlers for that event
    if (!handler) {
      for (const h of this.#handlers.get(event) || [])
        this.#el.removeEventListener(event, h);
      return this;
    }
    this.#el.addEventListener(event, handler);
    if (!this.#handlers.has(event)) this.#handlers.set(event, []);
    // @ts-ignore undefined check for handler above
    this.#handlers.get(event).push(handler);
    return this;
  }

  el(): HTMLElement {
    return this.#el;
  }

  query(selector: string): Element | undefined {
    const el = this.#el.querySelector(selector);
    if (el instanceof HTMLElement) return new Element(el);
  }

  queryAll(selector: string): Element[] {
    const els = this.#el.querySelectorAll(selector);
    const result = [];
    for (const el of els) {
      if (el instanceof HTMLElement) result.push(new Element(el));
    }
    return result;
  }
}

export function applyElementArgs(
  el: HTMLElement,
  ...args: ElementArg[]
): Record<string, EventHandler[]> {
  const handlers: Record<string, EventHandler[]> = {};
  for (const arg of args) {
    if (typeof arg === "string") {
      el.append(document.createTextNode(arg));
      continue;
    }
    if (arg instanceof Element) {
      el.append(arg.el());
      continue;
    }
    if (arg instanceof HTMLElement || arg instanceof Text) {
      el.append(arg);
      continue;
    }
    if (typeof arg === "object" && arg !== null) {
      for (const key in arg) {
        if (key == "id") {
          if (typeof arg[key] === "string") el.id = arg[key];
          continue;
        }
        if (key == "class") {
          if (typeof arg[key] === "string") el.className = arg[key];
          continue;
        }
        if (typeof arg[key] === "function") {
          if (handlers[key] === void 0) handlers[key] = [];
          handlers[key].push(arg[key]);
          continue;
        }
        el.setAttribute(key, arg[key]);
      }
    }
  }
  return handlers;
}
/** Query the DOM for an element */
export function query(selector: string): Element | undefined {
  const el = document.querySelector(selector);
  if (el instanceof HTMLElement) return new Element(el);
}
/** Query the DOM for elements */
export function queryAll(selector: string): Element[] {
  const els = document.querySelectorAll(selector);
  const result = [];
  for (const el of els) {
    if (el instanceof HTMLElement) result.push(new Element(el));
  }
  return result;
}

const factory =
  (tag: string) =>
  (...args: ElementArg[]): Element =>
    new Element(tag, ...args);

export const a = factory("a");
export const abbr = factory("abbr");
export const area = factory("area");
export const article = factory("article");
export const aside = factory("aside");
export const audio = factory("audio");
export const b = factory("b");
export const bdi = factory("bdi");
export const bdo = factory("bdo");
export const blockquote = factory("blockquote");
export const br = factory("br");
export const button = factory("button");
export const canvas = factory("canvas");
export const caption = factory("caption");
export const cite = factory("cite");
export const code = factory("code");
export const col = factory("col");
export const colgroup = factory("colgroup");
export const data = factory("data");
export const datalist = factory("datalist");
export const dd = factory("dd");
export const del = factory("del");
export const details = factory("details");
export const dialog = factory("dialog");
export const dfn = factory("dfn");
export const div = factory("div");
export const dl = factory("dl");
export const dt = factory("dt");
export const em = factory("em");
export const embed = factory("embed");
export const fieldset = factory("fieldset");
export const figcaption = factory("figcaption");
export const figure = factory("figure");
export const footer = factory("footer");
export const form = factory("form");
export const h1 = factory("h1");
export const h2 = factory("h2");
export const h3 = factory("h3");
export const h4 = factory("h4");
export const h5 = factory("h5");
export const h6 = factory("h6");
export const header = factory("header");
export const hgroup = factory("hgroup");
export const i = factory("i");
export const iframe = factory("iframe");
export const img = factory("img");
export const input = factory("input");
export const ins = factory("ins");
export const kbd = factory("kbd");
export const label = factory("label");
export const legend = factory("legend");
export const li = factory("li");
export const link = factory("link");
export const main = factory("main");
export const map = factory("map");
export const mark = factory("mark");
export const menu = factory("menu");
export const meter = factory("meter");
export const nav = factory("nav");
export const object = factory("object");
export const ol = factory("ol");
export const optgroup = factory("optgroup");
export const option = factory("option");
export const output = factory("output");
export const p = factory("p");
export const param = factory("param");
export const picture = factory("picture");
export const pre = factory("pre");
export const progress = factory("progress");
export const q = factory("q");
export const rp = factory("rp");
export const rt = factory("rt");
export const ruby = factory("ruby");
export const s = factory("s");
export const samp = factory("samp");
export const search = factory("search");
export const section = factory("section");
export const select = factory("select");
export const small = factory("small");
export const source = factory("source");
export const span = factory("span");
export const strong = factory("strong");
export const sub = factory("sub");
export const summary = factory("summary");
export const sup = factory("sup");
export const svg = factory("svg");
export const table = factory("table");
export const tbody = factory("tbody");
export const template = factory("template");
export const textarea = factory("textarea");
export const tfoot = factory("tfoot");
export const th = factory("th");
export const thead = factory("thead");
export const time = factory("time");
export const tr = factory("tr");
export const track = factory("track");
export const u = factory("u");
export const ul = factory("ul");
export const video = factory("video");
