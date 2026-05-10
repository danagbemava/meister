import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";

vi.mock("../api/ask", () => ({
  askApi: {
    query: vi.fn(),
    cancel: vi.fn(),
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { askApi } from "../api/ask";
import { useAsk, __resetAskStoreForTests } from "./useAsk";

beforeEach(() => {
  __resetAskStoreForTests();
  vi.mocked(askApi.query).mockReset();
  vi.mocked(askApi.cancel).mockReset();
});

describe("useAsk", () => {
  it("starts with no threads", () => {
    const { result } = renderHook(() => useAsk("rec-1"));
    expect(result.current.threads).toEqual([]);
  });

  it("appends a new thread for a top-level question", async () => {
    vi.mocked(askApi.query).mockResolvedValue({
      text: "answer",
      citations: [{ quote: "q", start: 0, end: 1 }],
    });
    const { result } = renderHook(() => useAsk("rec-1"));
    await act(async () => { await result.current.ask("what?"); });
    expect(result.current.threads).toHaveLength(1);
    expect(result.current.threads[0].turns).toEqual([
      { role: "user", content: "what?" },
      expect.objectContaining({ role: "assistant", content: "answer" }),
    ]);
  });

  it("appends a follow-up to an existing thread when threadId is provided", async () => {
    vi.mocked(askApi.query).mockResolvedValueOnce({ text: "first", citations: [] });
    vi.mocked(askApi.query).mockResolvedValueOnce({ text: "second", citations: [] });
    const { result } = renderHook(() => useAsk("rec-1"));
    await act(async () => { await result.current.ask("Q1"); });
    const tid = result.current.threads[0].id;
    await act(async () => { await result.current.ask("Q2", tid); });
    expect(result.current.threads).toHaveLength(1);
    expect(result.current.threads[0].turns.map((t) => t.content)).toEqual(["Q1", "first", "Q2", "second"]);
  });

  it("preserves threads when switching recordings and back", async () => {
    vi.mocked(askApi.query).mockResolvedValue({ text: "a", citations: [] });
    const { result, rerender } = renderHook(({ id }) => useAsk(id), {
      initialProps: { id: "rec-1" },
    });
    await act(async () => { await result.current.ask("Q"); });
    rerender({ id: "rec-2" });
    expect(result.current.threads).toEqual([]);
    rerender({ id: "rec-1" });
    expect(result.current.threads).toHaveLength(1);
  });

  it("clears threads for the current recording", async () => {
    vi.mocked(askApi.query).mockResolvedValue({ text: "a", citations: [] });
    const { result } = renderHook(() => useAsk("rec-1"));
    await act(async () => { await result.current.ask("Q"); });
    act(() => { result.current.clearAll(); });
    expect(result.current.threads).toEqual([]);
  });

  it("forwards askApi errors as a failed assistant turn", async () => {
    vi.mocked(askApi.query).mockRejectedValue("some other error");
    const { result } = renderHook(() => useAsk("rec-1"));
    await act(async () => { await result.current.ask("Q"); });
    const last = result.current.threads[0].turns.at(-1)!;
    expect(last.role).toBe("assistant");
    expect(last.content).toMatch(/some other error/);
  });

  it("cancel() invokes askApi.cancel for the current recording", async () => {
    vi.mocked(askApi.cancel).mockResolvedValue(undefined);
    const { result } = renderHook(() => useAsk("rec-1"));
    await act(async () => { await result.current.cancel(); });
    expect(askApi.cancel).toHaveBeenCalledWith("rec-1");
  });

  it("drops the partial assistant turn when the query is cancelled", async () => {
    vi.mocked(askApi.query).mockRejectedValue("cancelled");
    const { result } = renderHook(() => useAsk("rec-1"));
    await act(async () => { await result.current.ask("Q"); });
    // The user turn stays; no assistant turn is appended.
    expect(result.current.threads).toHaveLength(1);
    expect(result.current.threads[0].turns).toEqual([
      { role: "user", content: "Q" },
    ]);
    expect(result.current.threads[0].pending).toBe(false);
  });

  it("rephrases Ollama unreachable errors", async () => {
    vi.mocked(askApi.query).mockRejectedValue("Cannot reach Ollama at http://localhost:11434: ECONNREFUSED");
    const { result } = renderHook(() => useAsk("rec-1"));
    await act(async () => { await result.current.ask("Q"); });
    const last = result.current.threads[0].turns.at(-1)!;
    expect(last.content).toBe("Error: Couldn't reach the model — is Ollama running?");
  });

  it("removeThread drops the named thread", async () => {
    vi.mocked(askApi.query).mockResolvedValue({ text: "a", citations: [] });
    const { result } = renderHook(() => useAsk("rec-1"));
    await act(async () => { await result.current.ask("Q1"); });
    await act(async () => { await result.current.ask("Q2"); });
    expect(result.current.threads).toHaveLength(2);
    const tid = result.current.threads[0].id;
    act(() => { result.current.removeThread(tid); });
    expect(result.current.threads).toHaveLength(1);
    expect(result.current.threads[0].turns[0].content).toBe("Q2");
  });
});
