import { afterEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";

afterEach(() => {
  cleanup();
});

// Tauri invoke/event mocks — tests override per-case.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

// jsdom ships matchMedia as undefined; stub a minimal impl.
if (!window.matchMedia) {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }) as MediaQueryList;
}

// jsdom doesn't ship ResizeObserver; provide a no-op stub so components
// that observe element size (Sidebar's frame-reporting effect) don't crash.
if (!globalThis.ResizeObserver) {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
}

// Node 25 ships a `localStorage` global that's a plain object (no Storage API
// methods). Replace it with a working in-memory Storage shim so components
// that rely on getItem/setItem/clear work in tests.
if (typeof localStorage === "undefined" || typeof localStorage.setItem !== "function") {
  const store = new Map<string, string>();
  const storage: Storage = {
    get length() { return store.size; },
    clear() { store.clear(); },
    getItem(key: string) { return store.has(key) ? store.get(key)! : null; },
    key(index: number) { return Array.from(store.keys())[index] ?? null; },
    removeItem(key: string) { store.delete(key); },
    setItem(key: string, value: string) { store.set(key, String(value)); },
  };
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: storage });
  if (typeof window !== "undefined") {
    Object.defineProperty(window, "localStorage", { configurable: true, value: storage });
  }
}
