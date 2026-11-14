import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import Notes, { parseNotes } from "./Notes";

const SAMPLE = `**4. ACTION ITEMS AND NEXT STEPS**

*   **Funding Confirmation:** Members are asked to ensure the financial contributions
for the funeral support are finalized.
*   **Retreat Planning:** Logistics will be flexible:
    *   Members may drive their own vehicle.
    *   Dedicated transport will be secured.

## Summary
Plain paragraph text.`;

describe("Notes", () => {
  it("groups bullets, nesting and wrapped lines into one list", () => {
    const blocks = parseNotes(SAMPLE);
    expect(blocks.map((b) => b.kind)).toEqual(["paragraph", "list", "heading", "paragraph"]);
    const list = blocks[1];
    if (list.kind !== "list") throw new Error("expected list");
    expect(list.items.map((i) => i.depth)).toEqual([0, 0, 2, 2]);
    expect(list.items[0].text).toContain("for the funeral support");
  });

  it("renders emphasis without leaving markdown markers", () => {
    const { container } = render(<Notes markdown={SAMPLE} />);
    expect(container.textContent).not.toMatch(/\*\*|^\s*\*\s/m);
    expect(container.querySelectorAll("strong").length).toBe(3);
  });
});
