import { describe, expect, it } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { useRef } from "react";
import { useScrollEdge } from "./useScrollEdge";

function makeScrollable(): HTMLDivElement {
  const el = document.createElement("div");
  Object.defineProperty(el, "scrollTop", { value: 0, writable: true });
  document.body.appendChild(el);
  return el;
}

describe("useScrollEdge", () => {
  it("is false initially when scrolled to top", () => {
    const el = makeScrollable();
    const { result } = renderHook(() => {
      const ref = useRef<HTMLDivElement>(el);
      return useScrollEdge(ref);
    });
    expect(result.current).toBe(false);
  });

  it("becomes true when the container scrolls down, false again at top", () => {
    const el = makeScrollable();
    const { result } = renderHook(() => {
      const ref = useRef<HTMLDivElement>(el);
      return useScrollEdge(ref);
    });

    act(() => {
      (el as any).scrollTop = 120;
      el.dispatchEvent(new Event("scroll"));
    });
    expect(result.current).toBe(true);

    act(() => {
      (el as any).scrollTop = 0;
      el.dispatchEvent(new Event("scroll"));
    });
    expect(result.current).toBe(false);
  });

  it("handles a null ref without crashing", () => {
    const { result } = renderHook(() => {
      const ref = useRef<HTMLDivElement>(null);
      return useScrollEdge(ref);
    });
    expect(result.current).toBe(false);
  });
});
