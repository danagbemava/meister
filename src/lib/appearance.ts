export type Appearance = "light" | "dark";

type Listener = (a: Appearance) => void;

let current: Appearance = "light";
let initialized = false;
const listeners = new Set<Listener>();

function apply(a: Appearance) {
  current = a;
  document.documentElement.dataset.appearance = a;
  for (const l of listeners) l(a);
}

export function initAppearance(): void {
  if (initialized) return;
  initialized = true;
  const mql = window.matchMedia?.("(prefers-color-scheme: dark)");
  if (!mql) {
    console.warn("appearance: matchMedia unavailable, defaulting to dark");
    apply("dark");
    return;
  }
  apply(mql.matches ? "dark" : "light");
  mql.addEventListener("change", (e) => apply(e.matches ? "dark" : "light"));
}

export function getAppearance(): Appearance {
  return current;
}

export function onAppearanceChange(cb: Listener): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

/** For tests only. Resets module state between test cases. */
export function __resetForTests(): void {
  current = "light";
  initialized = false;
  listeners.clear();
}
