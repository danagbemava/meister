import { describe, expect, it } from "vitest";
import { captureStamp, recordingTitle } from "./recordingTitle";
import { uniqueName } from "./uniqueName";

describe("recordingTitle", () => {
  it("turns a raw capture file name into a dated title", () => {
    const expected = `Recording · ${captureStamp(new Date(Date.UTC(2026, 10, 2, 21, 29, 12)))}`;
    expect(recordingTitle("capture-20261102-212912-c996297f")).toBe(expected);
  });

  it("keeps names the user chose", () => {
    expect(recordingTitle("Fathers day meeting 2")).toBe("Fathers day meeting 2");
  });
});

describe("uniqueName", () => {
  it("adds the first free number when the name is taken", () => {
    expect(uniqueName("Testing", ["testing", "Testing 2"])).toBe("Testing 3");
    expect(uniqueName("Planning", ["Testing"])).toBe("Planning");
  });
});
