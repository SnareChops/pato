import { bind } from "./bindings.js";

export function get(key: string): string | null {
  return localStorage.getItem(key);
}

export function set(key: string, value: string): void {
  localStorage.setItem(key, value);
}

export function del(key: string): void {
  localStorage.removeItem(key);
}

type PluginKey = {
  plugin: string;
  key: string;
};
bind(function storeGet(arg: PluginKey) {
  return get(`${arg.plugin}|${arg.key}`) ?? undefined;
});
bind(function storeSet(arg: PluginKey & { value: string }) {
  set(`${arg.plugin}|${arg.key}`, arg.value);
  return true;
});
bind(function storeDel(arg: PluginKey) {
  del(`${arg.plugin}|${arg.key}`);
  return true;
});
