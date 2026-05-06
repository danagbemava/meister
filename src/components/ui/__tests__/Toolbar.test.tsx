import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Toolbar } from "../Toolbar";

describe("Toolbar", () => {
  it("renders children inside a horizontal row", () => {
    render(
      <Toolbar>
        <span data-testid="left">Left</span>
        <span data-testid="right">Right</span>
      </Toolbar>,
    );
    expect(screen.getByTestId("left")).toBeInTheDocument();
    expect(screen.getByTestId("right")).toBeInTheDocument();
  });

  it("applies a bottom separator border", () => {
    const { container } = render(
      <Toolbar>
        <span>x</span>
      </Toolbar>,
    );
    const root = container.firstChild as HTMLElement;
    expect(root.className).toMatch(/border-b/);
    expect(root.className).toMatch(/border-separator/);
  });

  it("Toolbar.Spacer renders a flex-1 div", () => {
    render(
      <Toolbar>
        <span data-testid="leading">L</span>
        <Toolbar.Spacer />
        <span data-testid="trailing">T</span>
      </Toolbar>,
    );
    const spacer = screen.getByTestId("leading").nextElementSibling as HTMLElement;
    expect(spacer.className).toMatch(/flex-1/);
  });
});
