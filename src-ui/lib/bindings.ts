type TauriCore = {
  invoke: (cmd: string, args?: unknown) => Promise<unknown>;
};
//@ts-ignore
const { invoke } = window.__TAURI__.core as TauriCore;
type Event<T> = {
  event: string;
  windowLabel: string;
  payload: T;
  id: number;
};
type TauriEvent = {
  listen: <T>(event: string, handler: (event: Event<T>) => void) => Promise<() => void>;
};
//@ts-ignore
const { listen } = window.__TAURI__.event as TauriEvent;

const BINDINGS = new Map<string, Function>();

type Binding = {
  id: string;
  name: string;
  args: unknown[];
};

export function bind(fn: Function, name: string = fn.name): void {
  console.log("Binding JS function:", name);
  BINDINGS.set(name, fn);
}

export async function callRust<T>(cmd: string, args?: object): Promise<T> {
  console.log("Calling rust:", cmd, args);
  //@ts-ignore
  return await invoke(cmd, args);
}

document.addEventListener("DOMContentLoaded", async () => {
  console.log("Registering JS binding listener");
  // Listen for events from Rust
  await listen("call-js", async (/** @type {Event<Binding>} */ event: Event<Binding>) => {
    console.log("call-js received", event.payload);
    const { id, name, args } = event.payload;
    const fn = BINDINGS.get(name);
    if (!fn) {
      console.error("No JS binding found for", name);
      return;
    }
    console.log("Found JS binding for", name, fn);
    const result = await fn(...(Array.isArray(args) ? args : [args]));
    console.log("Emitting JS response", { id, result });
    await invoke("js_response", { id, result });
  });
});
