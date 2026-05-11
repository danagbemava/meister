import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
const listenMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));

import { useGlobalSettings, __resetGlobalSettingsForTests } from "./useGlobalSettings";

const SAMPLE = {
  ollamaUrl: "http://localhost:11434",
  model: "gemma3",
  whisperModel: "base",
  language: "auto",
  defaultExportFormat: "md" as const,
};

beforeEach(() => {
  invokeMock.mockReset();
  listenMock.mockReset();
  listenMock.mockResolvedValue(() => {});
  __resetGlobalSettingsForTests();
});

describe("useGlobalSettings", () => {
  it("hydrates from get_global_settings on first mount", async () => {
    invokeMock.mockImplementation((name: string) => {
      if (name === "get_global_settings") return Promise.resolve(SAMPLE);
      return Promise.resolve();
    });
    const { result } = renderHook(() => useGlobalSettings());
    await waitFor(() => expect(result.current.settings).toEqual(SAMPLE));
  });

  it("update() calls set_global_settings and refreshes locally", async () => {
    invokeMock.mockImplementation((name: string) => {
      if (name === "get_global_settings") return Promise.resolve(SAMPLE);
      if (name === "set_global_settings") {
        return Promise.resolve();
      }
      return Promise.resolve();
    });
    const { result } = renderHook(() => useGlobalSettings());
    await waitFor(() => expect(result.current.settings).toEqual(SAMPLE));

    await act(async () => {
      await result.current.update({ ...SAMPLE, model: "qwen3" });
    });

    expect(invokeMock).toHaveBeenCalledWith("set_global_settings", {
      settings: { ...SAMPLE, model: "qwen3" },
    });
    expect(result.current.settings).toEqual({ ...SAMPLE, model: "qwen3" });
  });

  it("two consumers see the same snapshot", async () => {
    invokeMock.mockResolvedValue(SAMPLE);
    const a = renderHook(() => useGlobalSettings());
    const b = renderHook(() => useGlobalSettings());
    await waitFor(() => expect(a.result.current.settings).toEqual(SAMPLE));
    expect(b.result.current.settings).toEqual(SAMPLE);
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
});
