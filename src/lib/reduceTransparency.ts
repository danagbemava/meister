import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

let current = false;
let initialized = false;

function apply(value: boolean) {
  current = value;
  if (value) {
    document.documentElement.dataset.reduceTransparency = "true";
  } else {
    delete document.documentElement.dataset.reduceTransparency;
  }
}

export function isReduceTransparency(): boolean {
  return current;
}

export async function initReduceTransparency(): Promise<void> {
  if (initialized) return;
  initialized = true;

  let reduceTx = false;
  try {
    reduceTx = await invoke<boolean>("get_reduce_transparency");
  } catch (e) {
    console.warn(
      "reduceTransparency: get_reduce_transparency failed, defaulting to false",
      e,
    );
  }

  let vibrancyActive = true;
  try {
    vibrancyActive = await invoke<boolean>("is_vibrancy_active");
  } catch (e) {
    console.warn(
      "reduceTransparency: is_vibrancy_active failed, assuming active",
      e,
    );
  }

  // If user has reduce-transparency on, OR vibrancy failed to attach,
  // force solid surfaces. Otherwise let vibrancy show through.
  apply(reduceTx || !vibrancyActive);

  try {
    await listen<boolean>("system-reduce-transparency-changed", (e) => {
      apply(Boolean(e.payload));
    });
  } catch (e) {
    console.warn("reduceTransparency: listen failed, no live updates", e);
  }

  try {
    await listen("vibrancy-unavailable", () => {
      apply(true);
    });
  } catch (e) {
    console.warn("reduceTransparency: vibrancy-unavailable listen failed", e);
  }
}

/** For tests only. */
export function __resetForTests(): void {
  current = false;
  initialized = false;
  document.documentElement.removeAttribute("data-reduce-transparency");
}
