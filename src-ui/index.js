(() => {
  // src-ui/lib/bindings.ts
  var { invoke } = window.__TAURI__.core;
  var { listen } = window.__TAURI__.event;
  var BINDINGS = /* @__PURE__ */ new Map();
  function bind(fn, name = fn.name) {
    console.log("Binding JS function:", name);
    BINDINGS.set(name, fn);
  }
  async function callRust(cmd, args) {
    console.log("Calling rust:", cmd, args);
    return await invoke(cmd, args);
  }
  document.addEventListener("DOMContentLoaded", async () => {
    console.log("Registering JS binding listener");
    await listen("call-js", async (event) => {
      console.log("call-js received", event.payload);
      const { id, name, args } = event.payload;
      const fn = BINDINGS.get(name);
      if (!fn) {
        console.error("No JS binding found for", name);
        return;
      }
      console.log("Found JS binding for", name, fn);
      const result = await fn(...Array.isArray(args) ? args : [args]);
      console.log("Emitting JS response", { id, result });
      await invoke("js_response", { id, result });
    });
  });

  // src-ui/lib/db.ts
  var version = 3;
  var db;
  var Store = (name, key, auto, indexes) => ({
    name,
    key,
    auto,
    indexes
  });
  var stores = [
    Store("status-widgets", ["owner", "id"], false, []),
    Store("layout", ["owner", "id"], false, []),
    Store("plugins", "package", false, [])
  ];
  function init() {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open("pato-db", version);
      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(db = request.result);
      request.onupgradeneeded = (event) => {
        const db2 = event?.target?.result;
        for (const def of stores) {
          if (!db2.objectStoreNames.contains(def.name)) {
            const store = db2.createObjectStore(def.name, {
              keyPath: def.key,
              autoIncrement: def.auto
            });
            for (const idx of def.indexes) {
              store.createIndex(idx.name, idx.key, idx.unique ? { unique: true } : void 0);
            }
          }
        }
      };
    });
  }
  async function set(store, data2) {
    return new Promise((resolve, reject) => {
      const tx = db.transaction(store, "readwrite");
      const request = tx.objectStore(store).put(data2);
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
    });
  }
  async function get(store, key, index) {
    return new Promise((resolve, reject) => {
      const tx = db.transaction(store, "readonly");
      const source2 = index ? tx.objectStore(store).index(index) : tx.objectStore(store);
      const request = source2.get(key);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
  }
  async function getAll(store) {
    return new Promise((resolve, reject) => {
      const tx = db.transaction(store, "readonly");
      const request = tx.objectStore(store).getAll();
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
  }
  async function del(store, key) {
    return new Promise((resolve, reject) => {
      const tx = db.transaction(store, "readwrite");
      const request = tx.objectStore(store).delete(key);
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
    });
  }
  async function count(store, query, index) {
    return new Promise((resolve, reject) => {
      const tx = db.transaction(store, "readonly");
      const source2 = index ? tx.objectStore(store).index(index) : tx.objectStore(store);
      const request = source2.count(query);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
  }
  function createRange(range) {
    if ("lower" in range && "upper" in range) {
      return IDBKeyRange.bound(range.lower.key, range.upper.key, range.lower.open ?? false, range.upper.open ?? false);
    }
    if ("lower" in range) {
      return IDBKeyRange.lowerBound(range.lower.key, range.lower.open ?? false);
    }
    if ("upper" in range) {
      return IDBKeyRange.upperBound(range.upper.key, range.upper.open ?? false);
    }
    throw new Error("Invalid QueryRange");
  }
  function createQuery(query) {
    if ("key" in query) {
      return query.key;
    }
    if ("keys" in query) {
      return query.keys;
    }
    if ("range" in query) {
      return createRange(query.range);
    }
    throw new Error("Invalid Query");
  }
  function serializeResult(result) {
    if (Array.isArray(result)) {
      return result.map((item) => Object.entries(item).map(([k, v]) => [k, v]));
    }
    return [Object.entries(result).map(([k, v]) => [k, v])];
  }
  bind(async function dbGet({ plugin, store, index, query }) {
    const result = await get(`${plugin}|${store}`, createQuery(query), index);
    if (!result) return [];
    return serializeResult(result);
  });
  bind(async function dbSet({ plugin, store, data: data2 }) {
    const entry = data2.reduce((obj, [k, v]) => {
      obj[k] = v;
      return obj;
    }, {});
    await set(`${plugin}|${store}`, entry);
    return true;
  });
  bind(async function dbGetAll({ plugin, store }) {
    const result = await getAll(`${plugin}|${store}`);
    return serializeResult(result);
  });
  bind(async function dbDel({ plugin, store, query }) {
    await del(`${plugin}|${store}`, createQuery(query));
    return true;
  });
  bind(async function dbCount({ plugin, store, index, query }) {
    return await count(`${plugin}|${store}`, query ? createQuery(query) : void 0, index);
  });

  // src-ui/lib/html.ts
  var Element = class _Element {
    #el;
    #handlers = /* @__PURE__ */ new Map();
    constructor(tag, ...args) {
      if (tag instanceof HTMLElement) this.#el = tag;
      else this.#el = document.createElement(tag);
      const h = applyElementArgs(this.#el, ...args);
      for (const key in h) {
        this.#handlers.set(key, h[key]);
      }
    }
    id(id) {
      if (!!id) {
        this.#el.id = id;
        return this;
      }
      return this.#el.id;
    }
    class(name) {
      if (!!name) {
        this.#el.className = name;
        return this;
      }
      return this.#el.className;
    }
    attr(attrs) {
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
          if (attrs[key] === null || attrs[key] === void 0)
            this.#el.removeAttribute(key);
          else this.#el.setAttribute(key, attrs[key]);
        }
        return this;
      }
      const result = {
        ...this.id() ? { id: this.id() } : {},
        ...this.class() ? { class: this.class() } : {}
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
    text(value) {
      if (!value) return this;
      this.#el.append(document.createTextNode(value));
      return this;
    }
    append(...elements) {
      for (const e of elements) {
        if (e instanceof _Element) this.#el.append(e.el());
        else this.#el.append(e);
      }
      return this;
    }
    on(event, handler) {
      if (!handler) {
        for (const h of this.#handlers.get(event) || [])
          this.#el.removeEventListener(event, h);
        return this;
      }
      this.#el.addEventListener(event, handler);
      if (!this.#handlers.has(event)) this.#handlers.set(event, []);
      this.#handlers.get(event).push(handler);
      return this;
    }
    el() {
      return this.#el;
    }
    query(selector) {
      const el = this.#el.querySelector(selector);
      if (el instanceof HTMLElement) return new _Element(el);
    }
    queryAll(selector) {
      const els = this.#el.querySelectorAll(selector);
      const result = [];
      for (const el of els) {
        if (el instanceof HTMLElement) result.push(new _Element(el));
      }
      return result;
    }
  };
  function applyElementArgs(el, ...args) {
    const handlers = {};
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
  var factory = (tag) => (...args) => new Element(tag, ...args);
  var a = factory("a");
  var abbr = factory("abbr");
  var area = factory("area");
  var article = factory("article");
  var aside = factory("aside");
  var audio = factory("audio");
  var b = factory("b");
  var bdi = factory("bdi");
  var bdo = factory("bdo");
  var blockquote = factory("blockquote");
  var br = factory("br");
  var button = factory("button");
  var canvas = factory("canvas");
  var caption = factory("caption");
  var cite = factory("cite");
  var code = factory("code");
  var col = factory("col");
  var colgroup = factory("colgroup");
  var data = factory("data");
  var datalist = factory("datalist");
  var dd = factory("dd");
  var del2 = factory("del");
  var details = factory("details");
  var dialog = factory("dialog");
  var dfn = factory("dfn");
  var div = factory("div");
  var dl = factory("dl");
  var dt = factory("dt");
  var em = factory("em");
  var embed = factory("embed");
  var fieldset = factory("fieldset");
  var figcaption = factory("figcaption");
  var figure = factory("figure");
  var footer = factory("footer");
  var form = factory("form");
  var h1 = factory("h1");
  var h2 = factory("h2");
  var h3 = factory("h3");
  var h4 = factory("h4");
  var h5 = factory("h5");
  var h6 = factory("h6");
  var header = factory("header");
  var hgroup = factory("hgroup");
  var i = factory("i");
  var iframe = factory("iframe");
  var img = factory("img");
  var input = factory("input");
  var ins = factory("ins");
  var kbd = factory("kbd");
  var label = factory("label");
  var legend = factory("legend");
  var li = factory("li");
  var link = factory("link");
  var main = factory("main");
  var map = factory("map");
  var mark = factory("mark");
  var menu = factory("menu");
  var meter = factory("meter");
  var nav = factory("nav");
  var object = factory("object");
  var ol = factory("ol");
  var optgroup = factory("optgroup");
  var option = factory("option");
  var output = factory("output");
  var p = factory("p");
  var param = factory("param");
  var picture = factory("picture");
  var pre = factory("pre");
  var progress = factory("progress");
  var q = factory("q");
  var rp = factory("rp");
  var rt = factory("rt");
  var ruby = factory("ruby");
  var s = factory("s");
  var samp = factory("samp");
  var search = factory("search");
  var section = factory("section");
  var select = factory("select");
  var small = factory("small");
  var source = factory("source");
  var span = factory("span");
  var strong = factory("strong");
  var sub = factory("sub");
  var summary = factory("summary");
  var sup = factory("sup");
  var svg = factory("svg");
  var table = factory("table");
  var tbody = factory("tbody");
  var template = factory("template");
  var textarea = factory("textarea");
  var tfoot = factory("tfoot");
  var th = factory("th");
  var thead = factory("thead");
  var time = factory("time");
  var tr = factory("tr");
  var track = factory("track");
  var u = factory("u");
  var ul = factory("ul");
  var video = factory("video");

  // src-ui/lib/component.ts
  var Component = class extends HTMLElement {
    static get tag() {
      return this.name.replace(/([A-Z])/g, "-$1").toLowerCase().replace(/^-/, "");
    }
    #handlers = /* @__PURE__ */ new Map();
    constructor(...args) {
      super();
      const handlers = applyElementArgs(this, ...args);
      for (const key in handlers) {
        this.#handlers.set(key, handlers[key]);
      }
    }
    setStyle(styles) {
      for (const [key, value] of Object.entries(styles)) {
        this.style[key] = value;
      }
    }
    append(...nodes) {
      super.append(...nodes.map((x) => x instanceof Element ? x.el() : x));
    }
    on(event, callback) {
      this.addEventListener(event, callback, false);
    }
  };
  function component(element, options) {
    console.log("Defining component:", element.tag);
    customElements.define(element.tag, element, options);
    return element;
  }

  // src-ui/components/pato-grid.ts
  var PatoGrid = class extends Component {
    connectedCallback() {
    }
  };
  var pato_grid_default = component(PatoGrid);

  // src-ui/components/pato-header-left.ts
  var PatoHeaderLeft = class extends Component {
    connectedCallback() {
      this.setStyle({
        display: "flex",
        alignItems: "center",
        gap: "12px"
      });
      this.append(
        span({ style: 'fontSize: "28px"; line-height: 1;' }, "\u{1F986}"),
        h1(
          {
            style: 'font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 20px; font-weight: 600; color: white; margin: 0; text-shadow: 0 1px 2px rgba(0,0,0,0.2);'
          },
          "Pato"
        )
      );
    }
  };
  var pato_header_left_default = component(PatoHeaderLeft);

  // src-ui/components/pato-header-center.ts
  var PatoHeaderCenter = class extends Component {
    connectedCallback() {
      this.setStyle({ flex: "1" });
    }
  };
  var pato_header_center_default = component(PatoHeaderCenter);

  // src-ui/components/pato-header-right.ts
  var PatoHeaderRight = class extends Component {
    connectedCallback() {
      this.setStyle({
        display: "flex",
        alignItems: "center",
        gap: "8px"
      });
    }
  };
  var pato_header_right_default = component(PatoHeaderRight);

  // src-ui/components/pato-header.ts
  var PatoHeader = class extends Component {
    connectedCallback() {
      this.setStyle({
        display: "flex",
        width: "100%",
        background: "linear-gradient(135deg, #1e3a8a 0%, #1d4ed8 100%)",
        boxShadow: "0 2px 8px rgba(0, 0, 0, 0.15)",
        borderBottom: "1px solid rgba(255, 255, 255, 0.1)",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "12px 20px",
        height: "60px",
        boxSizing: "border-box"
      });
    }
  };
  var pato_header_default = component(PatoHeader);

  // src-ui/widgets/status-widget.ts
  var StatusWidget = class extends Component {
    #icon = img();
    #label = p();
    set icon(value) {
      this.#icon.attr({ src: value ?? "" });
    }
    set label(value) {
      this.#label.clear();
      this.#label.text(value ?? "");
    }
    connectedCallback() {
      this.append(this.#icon, this.#label);
      this.on("click", this.onClick);
    }
    async onClick() {
      await callRust("status_widget_clicked", { id: this.id });
    }
  };
  var status_widget_default = component(StatusWidget);

  // src-ui/components/pato-status-widgets.ts
  var PatoStatusWidgets = class extends Component {
    #widgets = [];
    add(widget) {
      this.#widgets.push(widget);
      this.append(widget);
    }
    updateStatusWidget(props) {
      if (!props.id) return false;
      console.log("Updating status widget", props);
      let widget = this.querySelector(`status-widget[id="${props.id}"]`);
      if (!widget) {
        widget = new StatusWidget({ id: props.id });
        this.#widgets.push(widget);
        this.append(widget);
      }
      for (const key in props) {
        console.log("setting", key, "to", props[key]);
        widget[key] = props[key];
      }
      return true;
    }
  };
  var pato_status_widgets_default = component(PatoStatusWidgets);

  // src-ui/lib/storage.ts
  function get2(key) {
    return localStorage.getItem(key);
  }
  function set2(key, value) {
    localStorage.setItem(key, value);
  }
  function del3(key) {
    localStorage.removeItem(key);
  }
  bind(function storeGet(arg) {
    return get2(`${arg.plugin}|${arg.key}`) ?? void 0;
  });
  bind(function storeSet(arg) {
    set2(`${arg.plugin}|${arg.key}`, arg.value);
    return true;
  });
  bind(function storeDel(arg) {
    del3(`${arg.plugin}|${arg.key}`);
    return true;
  });

  // src-ui/lib/widgets.ts
  bind(function widgetUpdate(widget) {
    console.log("Updating widget from Rust:", widget);
    switch (widget.type) {
      case "status":
        const statusWidgets = document.querySelector("pato-status-widgets");
        if (statusWidgets) statusWidgets.updateStatusWidget(widget);
    }
  });

  // src-ui/lib/log.ts
  function info(...message) {
    console.info(...message);
  }
  function warn(...message) {
    console.warn(...message);
  }
  function error(...message) {
    console.error(...message);
  }
  bind(function logInfo(message) {
    info(message);
  });
  bind(function logWarn(message) {
    warn(message);
  });
  bind(function logError(message) {
    error(message);
  });

  // src-ui/pato.ts
  async function init2() {
    await init();
    await initHeader();
    await callRust("pato_ready");
  }
  async function initHeader() {
    document.body.append(new pato_header_default(new pato_header_left_default(), new pato_header_center_default(), new pato_header_right_default(new pato_status_widgets_default())));
  }
  init2();
})();
//# sourceMappingURL=index.js.map
