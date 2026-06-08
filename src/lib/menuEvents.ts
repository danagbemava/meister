import { listen } from "@tauri-apps/api/event";

export type MenuEventName =
  | "menu:new-project"
  | "menu:import-audio"
  | "menu:run-pipeline"
  | "menu:toggle-sidebar"
  | "menu:settings"
  | "menu:export"
  | "menu:demo-mode";

type Handler = () => void;

const handlers = new Map<MenuEventName, Handler>();
let initialized = false;

export function setMenuHandler(event: MenuEventName, handler: Handler): void {
  handlers.set(event, handler);
}

export async function initMenuEvents(): Promise<void> {
  if (initialized) return;
  initialized = true;
  const events: MenuEventName[] = [
    "menu:new-project",
    "menu:import-audio",
    "menu:run-pipeline",
    "menu:toggle-sidebar",
    "menu:settings",
    "menu:export",
    "menu:demo-mode",
  ];
  for (const ev of events) {
    try {
      await listen(ev, () => {
        const h = handlers.get(ev);
        if (h) h();
      });
    } catch (e) {
      console.warn(`menuEvents: listen failed for ${ev}`, e);
    }
  }
}

/** For tests only. */
export function __resetForTests(): void {
  handlers.clear();
  initialized = false;
}
