import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { Toggle } from "../Toggle";

describe("Toggle", () => {
  it("renders unchecked by default", () => {
    render(<Toggle aria-label="Notifications" />);
    expect(screen.getByRole("switch", { name: "Notifications" })).not.toBeChecked();
  });

  it("toggles on Space keypress", async () => {
    const onCheckedChange = vi.fn();
    render(<Toggle aria-label="Notifications" onCheckedChange={onCheckedChange} />);
    const sw = screen.getByRole("switch", { name: "Notifications" });
    sw.focus();
    await userEvent.keyboard(" ");
    expect(onCheckedChange).toHaveBeenCalledWith(true);
  });

  it("supports controlled checked state", async () => {
    function Wrapper() {
      const [v, setV] = useState(false);
      return (
        <Toggle
          aria-label="Notifications"
          checked={v}
          onCheckedChange={setV}
        />
      );
    }
    render(<Wrapper />);
    const sw = screen.getByRole("switch", { name: "Notifications" });
    expect(sw).not.toBeChecked();
    await userEvent.click(sw);
    expect(sw).toBeChecked();
  });

  it("forwards ref", () => {
    let captured: HTMLButtonElement | null = null;
    render(
      <Toggle
        aria-label="Notifications"
        ref={(el) => {
          captured = el;
        }}
      />,
    );
    expect(captured).toBeInstanceOf(HTMLButtonElement);
  });
});
