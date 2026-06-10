import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createRef } from "react";
import { Button } from "../Button";

describe("Button", () => {
  it("renders children", () => {
    render(<Button>Click me</Button>);
    expect(screen.getByRole("button", { name: "Click me" })).toBeInTheDocument();
  });

  it("applies the primary variant classes", () => {
    render(<Button variant="primary">Run</Button>);
    const btn = screen.getByRole("button", { name: "Run" });
    expect(btn.className).toMatch(/bg-accent/);
    expect(btn.className).toMatch(/text-accent-label/);
  });

  it("applies the size classes (defaults to md)", () => {
    render(<Button>Default</Button>);
    expect(screen.getByRole("button", { name: "Default" }).className).toMatch(/h-8/);
    render(<Button size="sm">Small</Button>);
    expect(screen.getByRole("button", { name: "Small" }).className).toMatch(/h-6/);
    render(<Button size="lg">Large</Button>);
    expect(screen.getByRole("button", { name: "Large" }).className).toMatch(/h-9/);
  });

  it("does not fire onClick when disabled", async () => {
    const onClick = vi.fn();
    render(
      <Button onClick={onClick} disabled>
        Disabled
      </Button>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Disabled" }));
    expect(onClick).not.toHaveBeenCalled();
  });

  it("forwards ref to the underlying button element", () => {
    const ref = createRef<HTMLButtonElement>();
    render(<Button ref={ref}>Ref</Button>);
    expect(ref.current).toBeInstanceOf(HTMLButtonElement);
  });

  it("renders iconLeft before children", () => {
    const { container } = render(
      <Button iconLeft="play">Resume</Button>,
    );
    const btn = container.querySelector("button");
    expect(btn?.textContent).toContain("Resume");
    const svgs = container.querySelectorAll("svg");
    expect(svgs.length).toBe(1);
    // The icon should be the first child of the button.
    expect(btn?.firstElementChild?.tagName.toLowerCase()).toBe("svg");
  });

  it("renders iconRight after children", () => {
    const { container } = render(
      <Button iconRight="chevron-right">Next</Button>,
    );
    const btn = container.querySelector("button");
    expect(btn?.lastElementChild?.tagName.toLowerCase()).toBe("svg");
  });

  it("renders icon-only when no children + iconLeft + aria-label", () => {
    render(<Button iconLeft="pause" aria-label="Pause" />);
    const btn = screen.getByRole("button", { name: "Pause" });
    expect(btn.querySelector("svg")).not.toBeNull();
    // No visible text content.
    expect(btn.textContent?.trim()).toBe("");
  });

  it("warns in dev when icon-only Button has no aria-label", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    render(<Button iconLeft="pause" />);
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });

  it("uses the control radius and pressed scale", () => {
    render(<Button>Go</Button>);
    const btn = screen.getByRole("button", { name: "Go" });
    expect(btn.className).toContain("rounded-control");
    expect(btn.className).toContain("active:scale-[0.97]");
    expect(btn.className).not.toContain("cursor-pointer");
  });

  it("primary variant carries the glass top highlight", () => {
    render(<Button variant="primary">Go</Button>);
    const btn = screen.getByRole("button", { name: "Go" });
    expect(btn.className).toContain("shadow-[inset_0_1px_0_0_rgba(255,255,255,0.18)]");
  });
});
