import { bind } from "./bindings";

export function debug<T>(content: T): T {
  console.debug(content);
  return content;
}

export function info(...message: any[]) {
  console.info(...message);
}

export function warn(...message: any[]) {
  console.warn(...message);
}

export function error(...message: any[]) {
  console.error(...message);
}

bind(function logInfo(message: string) {
  info(message);
});
bind(function logWarn(message: string) {
  warn(message);
});
bind(function logError(message: string) {
  error(message);
});
