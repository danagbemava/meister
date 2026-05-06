import { describe, expect, it } from "vitest";
import { computeRms, RingBuffer } from "./audioAmplitude";

describe("computeRms", () => {
  it("returns 0 for silence", () => {
    const buf = new Float32Array(1024);
    expect(computeRms(buf)).toBe(0);
  });

  it("returns ~1/sqrt(2) for a unit sine wave", () => {
    const buf = new Float32Array(1024);
    for (let i = 0; i < buf.length; i++) {
      buf[i] = Math.sin((2 * Math.PI * i) / 16);
    }
    expect(computeRms(buf)).toBeCloseTo(1 / Math.SQRT2, 2);
  });

  it("clamps to [0,1]", () => {
    const buf = new Float32Array(8).fill(2);
    expect(computeRms(buf)).toBe(1);
  });
});

describe("RingBuffer", () => {
  it("starts empty (returns zeros)", () => {
    const r = new RingBuffer(4);
    expect(Array.from(r.snapshot())).toEqual([0, 0, 0, 0]);
  });

  it("pushes values into a fixed-size window, oldest first", () => {
    const r = new RingBuffer(4);
    r.push(0.1);
    r.push(0.2);
    r.push(0.3);
    const snapshot = Array.from(r.snapshot());
    expect(snapshot[0]).toBe(0);
    expect(snapshot[1]).toBeCloseTo(0.1, 5);
    expect(snapshot[2]).toBeCloseTo(0.2, 5);
    expect(snapshot[3]).toBeCloseTo(0.3, 5);
  });

  it("wraps when overflowing", () => {
    const r = new RingBuffer(3);
    r.push(0.1);
    r.push(0.2);
    r.push(0.3);
    r.push(0.4);
    r.push(0.5);
    const snapshot = Array.from(r.snapshot());
    expect(snapshot[0]).toBeCloseTo(0.3, 5);
    expect(snapshot[1]).toBeCloseTo(0.4, 5);
    expect(snapshot[2]).toBeCloseTo(0.5, 5);
  });

  it("returns a new Float32Array reference each snapshot (so React sees changes)", () => {
    const r = new RingBuffer(2);
    const a = r.snapshot();
    r.push(0.1);
    const b = r.snapshot();
    expect(a).not.toBe(b);
  });
});
