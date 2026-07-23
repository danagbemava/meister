import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// Tauri invoke is already mocked globally in test-setup.ts
import { invoke } from "@tauri-apps/api/core";
import SettingsForm from "./SettingsForm";
import type { GlobalSettings } from "../types/storage";

const defaultSettings: GlobalSettings = {
  ollamaUrl: "http://localhost:11434",
  model: "gemma3",
  whisperModel: "base",
  language: "auto",
  defaultExportFormat: "md",
};

beforeEach(() => {
  // Clean up dataset and localStorage between tests so theme state is fresh.
  delete document.documentElement.dataset.theme;
  delete document.documentElement.dataset.appearance;
  localStorage.clear();
  vi.mocked(invoke).mockResolvedValue([]);
});

describe("SettingsForm — Appearance section", () => {
  it("renders four theme swatch buttons", () => {
    render(<SettingsForm settings={defaultSettings} onChange={() => {}} />);
    const buttons = screen.getAllByRole("button", {
      name: /Ink & Indigo|Forest & Brass|Plum & Rose|Manuscript/,
    });
    expect(buttons).toHaveLength(4);
  });

  it("clicking Manuscript sets document.documentElement.dataset.theme to 'manuscript'", async () => {
    render(<SettingsForm settings={defaultSettings} onChange={() => {}} />);
    const manuscriptBtn = screen.getByRole("button", { name: /Manuscript/i });
    await userEvent.click(manuscriptBtn);
    expect(document.documentElement.dataset.theme).toBe("manuscript");
  });

  it("aria-pressed moves to the clicked theme", async () => {
    render(<SettingsForm settings={defaultSettings} onChange={() => {}} />);
    const manuscriptBtn = screen.getByRole("button", { name: /Manuscript/i });

    // Before clicking: manuscript should not be pressed
    expect(manuscriptBtn).toHaveAttribute("aria-pressed", "false");

    await userEvent.click(manuscriptBtn);

    // After clicking: manuscript is pressed
    expect(manuscriptBtn).toHaveAttribute("aria-pressed", "true");

    // The previously-active button (ink-indigo by default) should no longer be pressed
    const inkIndigoBtn = screen.getByRole("button", { name: /Ink & Indigo/i });
    expect(inkIndigoBtn).toHaveAttribute("aria-pressed", "false");
  });

  it("reflects the stored theme as initially active", () => {
    localStorage.setItem("meister-theme", "forest-brass");
    render(<SettingsForm settings={defaultSettings} onChange={() => {}} />);
    const forestBtn = screen.getByRole("button", { name: /Forest & Brass/i });
    expect(forestBtn).toHaveAttribute("aria-pressed", "true");
  });
});
