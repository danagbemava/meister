import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onAppearanceChange } from "./appearance";

function currentAppearance(): "light" | "dark" {
  // Read from the DOM so this stays in sync with whatever `appearance.ts`
  // (or a test) has written, regardless of whether `initAppearance` has run.
  return document.documentElement.dataset.appearance === "light"
    ? "light"
    : "dark";
}

type Rgb = [number, number, number];
interface AccentColors {
  light: Rgb;
  dark: Rgb;
}

const FALLBACK: AccentColors = {
  light: [0, 122, 255],
  dark: [10, 132, 255],
};

let current: AccentColors = FALLBACK;

function setVar(name: string, value: string) {
  document.documentElement.style.setProperty(name, value);
}

function lighten(rgb: Rgb, amount: number): Rgb {
  return rgb.map((c) =>
    Math.round(Math.min(255, c + (255 - c) * amount)),
  ) as Rgb;
}

function darken(rgb: Rgb, amount: number): Rgb {
  return rgb.map((c) => Math.round(Math.max(0, c * (1 - amount)))) as Rgb;
}

/** Relative luminance (WCAG) — used to pick black vs white label text. */
function luminance([r, g, b]: Rgb): number {
  const srgb = [r, g, b].map((v) => {
    const n = v / 255;
    return n <= 0.03928 ? n / 12.92 : Math.pow((n + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * srgb[0] + 0.7152 * srgb[1] + 0.0722 * srgb[2];
}

function labelFor(rgb: Rgb): Rgb {
  // White text unless the accent is light enough that white would wash out
  // (Yellow, Graphite). Blues/Purples/Greens/Reds/Pinks sit below ~0.25;
  // Graphite sits around 0.32, Yellow around 0.69.
  return luminance(rgb) > 0.3 ? [0, 0, 0] : [255, 255, 255];
}

function apply() {
  const rgb = currentAppearance() === "dark" ? current.dark : current.light;
  setVar("--color-accent-rgb", rgb.join(" "));
  setVar("--color-accent-hover-rgb", lighten(rgb, 0.08).join(" "));
  setVar("--color-accent-pressed-rgb", darken(rgb, 0.12).join(" "));
  setVar("--color-accent-label-rgb", labelFor(rgb).join(" "));
}

export function getAccentRgb(): Rgb {
  return currentAppearance() === "dark" ? current.dark : current.light;
}

export async function initAccent(): Promise<void> {
  try {
    current = await invoke<AccentColors>("get_system_accent");
  } catch (e) {
    console.warn("accent: get_system_accent failed, using fallback", e);
    current = FALLBACK;
  }
  apply();

  try {
    await listen<AccentColors>("system-accent-changed", (e) => {
      current = e.payload;
      apply();
    });
  } catch (e) {
    console.warn("accent: listen failed, accent will stay on boot value", e);
  }

  onAppearanceChange(apply);
}
