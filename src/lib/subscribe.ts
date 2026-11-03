import { listen, type EventCallback } from "@tauri-apps/api/event";

export function subscribe<T>(event: string, handler: EventCallback<T>): () => void {
  let off: (() => void) | undefined;
  let cancelled = false;
  listen<T>(event, handler).then((fn) => {
    if (cancelled) fn();
    else off = fn;
  });
  return () => {
    cancelled = true;
    off?.();
  };
}
