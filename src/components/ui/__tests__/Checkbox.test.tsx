import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { Checkbox } from "../Checkbox";

describe("Checkbox", () => {
  it("renders unchecked by default", () => {
    render(<Checkbox aria-label="Agree" />);
    expect(screen.getByRole("checkbox", { name: "Agree" })).not.toBeChecked();
  });

  it("toggles on Space keypress", async () => {
    const onCheckedChange = vi.fn();
    render(
      <Checkbox aria-label="Agree" onCheckedChange={onCheckedChange} />,
    );
    const cb = screen.getByRole("checkbox", { name: "Agree" });
    cb.focus();
    await userEvent.keyboard(" ");
    expect(onCheckedChange).toHaveBeenCalledWith(true);
  });

  it("supports controlled checked state", () => {
    function Wrapper() {
      const [v, setV] = useState(false);
      return (
        <Checkbox
          aria-label="Agree"
          checked={v}
          onCheckedChange={(c) => setV(Boolean(c))}
        />
      );
    }
    render(<Wrapper />);
    const cb = screen.getByRole("checkbox", { name: "Agree" });
    expect(cb).not.toBeChecked();
  });

  it("renders indeterminate state", () => {
    render(<Checkbox aria-label="Agree" checked="indeterminate" />);
    const cb = screen.getByRole("checkbox", { name: "Agree" });
    expect(cb.getAttribute("data-state")).toBe("indeterminate");
  });

  it("forwards ref", () => {
    let captured: HTMLButtonElement | null = null;
    render(
      <Checkbox
        aria-label="Agree"
        ref={(el) => {
          captured = el;
        }}
      />,
    );
    expect(captured).toBeInstanceOf(HTMLButtonElement);
  });
});
