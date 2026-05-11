import { describe, expect, it } from "vitest";
import { folderColorFor, type FolderColor } from "./folderColor";

const PALETTE: FolderColor[] = ["blue", "green", "purple", "orange"];

describe("folderColorFor", () => {
  it("returns one of the four palette values", () => {
    for (const id of ["abc", "def", "8f3a", "x", ""]) {
      expect(PALETTE).toContain(folderColorFor(id));
    }
  });

  it("is deterministic for the same input", () => {
    expect(folderColorFor("project-1234")).toBe(folderColorFor("project-1234"));
    expect(folderColorFor("")).toBe(folderColorFor(""));
  });

  it("distributes across the palette for varied inputs", () => {
    const seen = new Set<FolderColor>();
    for (let i = 0; i < 16; i++) {
      seen.add(folderColorFor(`proj-${i}-${i * 7919}`));
    }
    expect(seen.size).toBeGreaterThanOrEqual(3);
  });

  it("returns palette colors for fixed UUID samples", () => {
    const fixture = [
      "550e8400-e29b-41d4-a716-446655440000",
      "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
      "00000000-0000-0000-0000-000000000000",
      "ffffffff-ffff-ffff-ffff-ffffffffffff",
    ].map(folderColorFor);
    for (const color of fixture) {
      expect(PALETTE).toContain(color);
    }
    expect(new Set(fixture).size).toBeGreaterThanOrEqual(2);
  });
});
