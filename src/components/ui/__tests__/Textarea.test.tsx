import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createRef } from "react";
import { Textarea } from "../Textarea";

describe("Textarea", () => {
  it("renders with placeholder", () => {
    render(<Textarea placeholder="Notes" />);
    expect(screen.getByPlaceholderText("Notes")).toBeInTheDocument();
  });

  it("accepts user typing", async () => {
    render(<Textarea defaultValue="" />);
    const ta = screen.getByRole("textbox");
    await userEvent.type(ta, "abc");
    expect(ta).toHaveValue("abc");
  });

  it("forwards ref", () => {
    const ref = createRef<HTMLTextAreaElement>();
    render(<Textarea ref={ref} />);
    expect(ref.current).toBeInstanceOf(HTMLTextAreaElement);
  });

  it("applies rounded-control radius class", () => {
    render(<Textarea />);
    const textarea = screen.getByRole("textbox");
    expect(textarea.className).toContain("rounded-control");
  });
});
