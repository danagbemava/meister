export type ThemeKey = "ink-indigo" | "forest-brass" | "plum-rose" | "manuscript";

export interface ThemeInfo {
  key: ThemeKey;
  name: string;
  mode: "light" | "dark";
  /** Swatch colors for the settings picker. */
  swatch: { bg: string; accent: string };
}

export const THEMES: ThemeInfo[] = [
  { key: "ink-indigo",   name: "Ink & Indigo",   mode: "dark",  swatch: { bg: "#191a21", accent: "#8b93f8" } },
  { key: "forest-brass", name: "Forest & Brass", mode: "dark",  swatch: { bg: "#181d1a", accent: "#b8a058" } },
  { key: "plum-rose",    name: "Plum & Rose",    mode: "dark",  swatch: { bg: "#1d181c", accent: "#d4899c" } },
  { key: "manuscript",   name: "Manuscript",     mode: "light", swatch: { bg: "#f5f1e8", accent: "#1f5e4e" } },
];

const STORAGE_KEY = "meister-theme";
const DEFAULT_THEME: ThemeKey = "ink-indigo";

export function getStoredTheme(): ThemeKey {
  const v = localStorage.getItem(STORAGE_KEY);
  return THEMES.some((t) => t.key === v) ? (v as ThemeKey) : DEFAULT_THEME;
}

export function applyTheme(key: ThemeKey): void {
  const theme = THEMES.find((t) => t.key === key) ?? THEMES[0];
  document.documentElement.dataset.theme = theme.key;
  document.documentElement.dataset.appearance = theme.mode;
  localStorage.setItem(STORAGE_KEY, theme.key);
}

/** Apply the persisted theme and follow changes made in other windows. */
export function initTheme(): void {
  applyTheme(getStoredTheme());
  window.addEventListener("storage", (e) => {
    if (e.key === STORAGE_KEY) applyTheme(getStoredTheme());
  });
}
