/** RMS over an interleaved or mono float frame, clamped to [0,1]. */
export function computeRms(samples: Float32Array): number {
  if (samples.length === 0) return 0;
  let sum = 0;
  for (let i = 0; i < samples.length; i++) {
    sum += samples[i] * samples[i];
  }
  const rms = Math.sqrt(sum / samples.length);
  return Math.min(1, Math.max(0, rms));
}

/** Fixed-size ring of floats. `snapshot()` returns a fresh copy each call so
 * React change detection works on the array reference. */
export class RingBuffer {
  private buf: Float32Array;
  private writeIdx = 0;
  private filled = 0;

  constructor(public readonly capacity: number) {
    this.buf = new Float32Array(capacity);
  }

  push(value: number): void {
    this.buf[this.writeIdx] = value;
    this.writeIdx = (this.writeIdx + 1) % this.capacity;
    if (this.filled < this.capacity) this.filled++;
  }

  /** Oldest → newest. Returns a fresh Float32Array. */
  snapshot(): Float32Array {
    const out = new Float32Array(this.capacity);
    if (this.filled < this.capacity) {
      // No wrap yet: data lives in [0, writeIdx).
      out.set(this.buf.subarray(0, this.writeIdx), this.capacity - this.writeIdx);
      return out;
    }
    // Full: oldest is at writeIdx.
    const tail = this.buf.subarray(this.writeIdx);
    const head = this.buf.subarray(0, this.writeIdx);
    out.set(tail, 0);
    out.set(head, tail.length);
    return out;
  }
}
