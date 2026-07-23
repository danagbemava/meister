import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ContextMenu } from "../ContextMenu";

function Harness({
  onRename,
  onDelete,
}: {
  onRename?: () => void;
  onDelete?: () => void;
}) {
  return (
    <ContextMenu>
      <ContextMenu.Trigger>
        <div data-testid="trigger" style={{ width: 100, height: 100 }}>
          row
        </div>
      </ContextMenu.Trigger>
      <ContextMenu.Content>
        <ContextMenu.Item onSelect={onRename}>Rename</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.Item variant="destructive" onSelect={onDelete}>
          Delete
        </ContextMenu.Item>
      </ContextMenu.Content>
    </ContextMenu>
  );
}

describe("ContextMenu", () => {
  it("does not render menu items until the trigger is right-clicked", () => {
    render(<Harness />);
    expect(screen.queryByText("Rename")).not.toBeInTheDocument();
  });

  it("opens the menu on right-click of the trigger", async () => {
    render(<Harness />);
    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByTestId("trigger"),
    });
    expect(await screen.findByText("Rename")).toBeInTheDocument();
    expect(screen.getByText("Delete")).toBeInTheDocument();
  });

  it("fires onSelect when an item is clicked", async () => {
    const onRename = vi.fn();
    render(<Harness onRename={onRename} />);
    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByTestId("trigger"),
    });
    await userEvent.click(await screen.findByText("Rename"));
    expect(onRename).toHaveBeenCalled();
  });

  it("applies destructive styling when variant='destructive'", async () => {
    render(<Harness />);
    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByTestId("trigger"),
    });
    const del = await screen.findByText("Delete");
    expect(del.className).toMatch(/text-status-error/);
  });

  it("content uses the opaque card surface", async () => {
    render(<Harness />);
    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByTestId("trigger"),
    });
    const content = await screen.findByRole("menu");
    expect(content.className).toContain("bg-control-background");
    expect(content.className).toContain("border-separator");
    expect(content.className).toContain("rounded-card");
  });
});
