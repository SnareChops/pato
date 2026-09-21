// Core dashboard lock state: whether widgets can be dragged/resized right
// now. Shared between the system menu (toggle) and the grid (enforcement)
// without either needing a DOM reference to the other.
const KEY = "pato:dashboard-locked";

type Listener = (locked: boolean) => void;

function load(): boolean {
  try {
    return localStorage.getItem(KEY) === "true";
  } catch {
    return false;
  }
}
function save(value: boolean) {
  try {
    localStorage.setItem(KEY, String(value));
  } catch {}
}

let locked = load();
const listeners = new Set<Listener>();

export function isLocked(): boolean {
  return locked;
}

export function setLocked(value: boolean) {
  if (value === locked) return;
  locked = value;
  save(locked);
  for (const listener of listeners) listener(locked);
}

export function toggleLocked(): boolean {
  setLocked(!locked);
  return locked;
}

/** Calls `listener` immediately with the current state, then on every change. Returns an unsubscribe function. */
export function onLockChange(listener: Listener): () => void {
  listeners.add(listener);
  listener(locked);
  return () => listeners.delete(listener);
}
