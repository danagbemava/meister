import { useEffect, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { GlobalSettings } from "../types/storage";

type Listener = () => void;
const listeners = new Set<Listener>();
let snapshot: GlobalSettings | null = null;
let hydrating: Promise<void> | null = null;
let unlisten: UnlistenFn | null = null;

function emit() {
  for (const l of listeners) l();
}

async function hydrate() {
  if (hydrating) return hydrating;
  hydrating = (async () => {
    snapshot = await invoke<GlobalSettings>("get_global_settings");
    emit();
    unlisten = await listen("global-settings-changed", async () => {
      snapshot = await invoke<GlobalSettings>("get_global_settings");
      emit();
    });
  })();
  return hydrating;
}

function subscribe(l: Listener): () => void {
  listeners.add(l);
  return () => { listeners.delete(l); };
}

function getSnapshot(): GlobalSettings | null {
  return snapshot;
}

export function useGlobalSettings(): {
  settings: GlobalSettings | null;
  update: (next: GlobalSettings) => Promise<void>;
} {
  const settings = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  useEffect(() => { void hydrate(); }, []);
  const update = async (next: GlobalSettings) => {
    await invoke("set_global_settings", { settings: next });
    snapshot = next;
    emit();
  };
  return { settings, update };
}

/** Test-only: reset module state between tests. */
export function __resetGlobalSettingsForTests() {
  listeners.clear();
  snapshot = null;
  hydrating = null;
  if (unlisten) { unlisten(); unlisten = null; }
}
