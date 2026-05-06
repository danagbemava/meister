import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import RecordingWaveform, { downsampleMax } from "./RecordingWaveform";

describe("RecordingWaveform", () => {
  it("renders a canvas element with the requested className", () => {
    const samples = new Float32Array([0.1, 0.2, 0.3]);
    const { container } = render(
      <RecordingWaveform samples={samples} state="recording" className="test-cls" />,
    );
    const canvas = container.querySelector("canvas");
    expect(canvas).not.toBeNull();
    expect(canvas?.className).toContain("test-cls");
  });

  it("re-renders when samples reference changes", () => {
    const a = new Float32Array([0.1]);
    const b = new Float32Array([0.5]);
    const { container, rerender } = render(
      <RecordingWaveform samples={a} state="recording" />,
    );
    const canvas1 = container.querySelector("canvas");
    rerender(<RecordingWaveform samples={b} state="recording" />);
    const canvas2 = container.querySelector("canvas");
    // Same DOM node — React reuses canvas; we just want no crash on re-render.
    expect(canvas1).toBe(canvas2);
  });
});

describe("downsampleMax", () => {
  it("right-aligns when src is shorter than n", () => {
    const out = downsampleMax(new Float32Array([0.5, 0.7]), 4);
    const result = Array.from(out);
    expect(result[0]).toBeCloseTo(0, 5);
    expect(result[1]).toBeCloseTo(0, 5);
    expect(result[2]).toBeCloseTo(0.5, 5);
    expect(result[3]).toBeCloseTo(0.7, 5);
  });

  it("returns max-per-window when src is longer than n", () => {
    const src = new Float32Array([0.1, 0.9, 0.2, 0.3, 0.5, 0.4]);
    const out = downsampleMax(src, 3);
    const result = Array.from(out);
    expect(result[0]).toBeCloseTo(0.9, 5);
    expect(result[1]).toBeCloseTo(0.3, 5);
    expect(result[2]).toBeCloseTo(0.5, 5);
  });

  it("handles empty src", () => {
    const out = downsampleMax(new Float32Array(0), 4);
    const result = Array.from(out);
    expect(result).toEqual([0, 0, 0, 0]);
  });
});
