import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { Select } from "../Select";

describe("Select", () => {
  it("renders option list", () => {
    render(
      <Select defaultValue="b">
        <option value="a">Apple</option>
        <option value="b">Banana</option>
      </Select>,
    );
    expect(screen.getByRole("combobox")).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Apple" })).toBeInTheDocument();
  });

  it("supports controlled value", async () => {
    function Wrapper() {
      const [v, setV] = useState("a");
      return (
        <Select value={v} onChange={(e) => setV(e.target.value)}>
          <option value="a">A</option>
          <option value="b">B</option>
        </Select>
      );
    }
    render(<Wrapper />);
    const sel = screen.getByRole("combobox") as HTMLSelectElement;
    await userEvent.selectOptions(sel, "b");
    expect(sel.value).toBe("b");
  });

  it("renders chevron via background SVG", () => {
    render(
      <Select>
        <option value="x">X</option>
      </Select>,
    );
    expect(screen.getByRole("combobox").className).toMatch(/bg-\[url/);
  });
});
