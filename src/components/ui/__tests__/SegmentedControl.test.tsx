import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SegmentedControl } from "../SegmentedControl";

describe("SegmentedControl", () => {
  it("renders all items", () => {
    render(
      <SegmentedControl value="a" onValueChange={() => {}}>
        <SegmentedControl.Item value="a">Apple</SegmentedControl.Item>
        <SegmentedControl.Item value="b">Banana</SegmentedControl.Item>
      </SegmentedControl>,
    );
    expect(screen.getByRole("radio", { name: "Apple" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Banana" })).toBeInTheDocument();
  });

  it("marks the value-matched item as selected via data-state", () => {
    render(
      <SegmentedControl value="b" onValueChange={() => {}}>
        <SegmentedControl.Item value="a">Apple</SegmentedControl.Item>
        <SegmentedControl.Item value="b">Banana</SegmentedControl.Item>
      </SegmentedControl>,
    );
    expect(
      screen.getByRole("radio", { name: "Banana" }).getAttribute("data-state"),
    ).toBe("on");
    expect(
      screen.getByRole("radio", { name: "Apple" }).getAttribute("data-state"),
    ).toBe("off");
  });

  it("fires onValueChange with the new value when an item is clicked", async () => {
    const onValueChange = vi.fn();
    render(
      <SegmentedControl value="a" onValueChange={onValueChange}>
        <SegmentedControl.Item value="a">Apple</SegmentedControl.Item>
        <SegmentedControl.Item value="b">Banana</SegmentedControl.Item>
      </SegmentedControl>,
    );
    await userEvent.click(screen.getByRole("radio", { name: "Banana" }));
    expect(onValueChange).toHaveBeenCalledWith("b");
  });

  it("applies size-based item height", () => {
    render(
      <SegmentedControl value="a" onValueChange={() => {}} size="lg">
        <SegmentedControl.Item value="a">A</SegmentedControl.Item>
      </SegmentedControl>,
    );
    expect(
      screen.getByRole("radio", { name: "A" }).className,
    ).toMatch(/h-7/);
  });
});
