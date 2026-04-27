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
});
