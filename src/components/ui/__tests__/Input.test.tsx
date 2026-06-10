import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createRef, useState } from "react";
import { Input } from "../Input";

describe("Input", () => {
  it("renders with placeholder", () => {
    render(<Input placeholder="Enter name" />);
    expect(screen.getByPlaceholderText("Enter name")).toBeInTheDocument();
  });

  it("supports controlled value updates", async () => {
    function Wrapper() {
      const [v, setV] = useState("");
      return <Input value={v} onChange={(e) => setV(e.target.value)} />;
    }
    render(<Wrapper />);
    const input = screen.getByRole("textbox");
    await userEvent.type(input, "hi");
    expect(input).toHaveValue("hi");
  });

  it("applies error styling when aria-invalid='true'", () => {
    render(<Input aria-invalid="true" />);
    const input = screen.getByRole("textbox");
    expect(input.className).toMatch(/aria-invalid:border-status-error/);
  });

  it("forwards ref", () => {
    const ref = createRef<HTMLInputElement>();
    render(<Input ref={ref} />);
    expect(ref.current).toBeInstanceOf(HTMLInputElement);
  });

  it("applies rounded-control radius class", () => {
    render(<Input />);
    const input = screen.getByRole("textbox");
    expect(input.className).toContain("rounded-control");
  });
});
