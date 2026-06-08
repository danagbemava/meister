import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Sidebar from "./Sidebar";
import type { Project, Recording } from "../types/storage";

const proj = (id: string, name: string): Project => ({
  id,
  name,
  createdAt: 1000,
  updatedAt: 1000,
  template: { source: "inline", content: "", sourcePath: null },
});

const rec = (id: string, projectId: string, name: string): Recording => ({
  id,
  projectId,
  name,
  sourcePaths: [],
  createdAt: 1000,
  updatedAt: 1000,
  stages: [],
});

const baseProps = () => ({
  projects: [proj("p1", "Quarterly Review"), proj("p2", "Customer Calls")],
  recordingsByProject: {
    p1: [rec("r1", "p1", "Standup notes"), rec("r2", "p1", "Retro session")],
    p2: [rec("r3", "p2", "Call with Acme")],
  },
  selection: { kind: "none" } as const,
  onSelect: vi.fn(),
  onProjectsChanged: vi.fn(),
  onRecordingsChanged: vi.fn(),
  onPickFilesForProject: vi.fn(),
});

describe("Sidebar", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("renders all projects and their recordings expanded by default", () => {
    render(<Sidebar {...baseProps()} />);
    expect(screen.getByText("Quarterly Review")).toBeInTheDocument();
    expect(screen.getByText("Customer Calls")).toBeInTheDocument();
    expect(screen.getByText("Standup notes")).toBeInTheDocument();
    expect(screen.getByText("Call with Acme")).toBeInTheDocument();
  });

  it("filters to projects whose name matches the search query", async () => {
    render(<Sidebar {...baseProps()} />);
    const search = screen.getByLabelText("Search projects and recordings");
    await userEvent.type(search, "quarterly");
    expect(screen.getByText("Quarterly Review")).toBeInTheDocument();
    expect(screen.queryByText("Customer Calls")).toBeNull();
  });

  it("auto-expands a project that contains a matching recording even if the project name doesn't match", async () => {
    render(<Sidebar {...baseProps()} />);
    const search = screen.getByLabelText("Search projects and recordings");
    await userEvent.type(search, "acme");
    expect(screen.getByText("Customer Calls")).toBeInTheDocument();
    expect(screen.getByText("Call with Acme")).toBeInTheDocument();
    expect(screen.queryByText("Standup notes")).toBeNull();
  });

  it("persists disclosure state to localStorage", () => {
    const { rerender } = render(<Sidebar {...baseProps()} />);
    // Click the disclosure (clicking the row toggles expansion).
    fireEvent.click(screen.getByLabelText(/Quarterly Review, project/));
    expect(localStorage.getItem("sidebar-expanded-p1")).toBe("false");
    rerender(<Sidebar {...baseProps()} />);
    expect(screen.queryByText("Standup notes")).toBeNull();
  });

  it("renders the empty state when no projects exist", () => {
    render(<Sidebar {...baseProps()} projects={[]} recordingsByProject={{}} />);
    expect(screen.getByText("No projects yet.")).toBeInTheDocument();
    // The empty-state CTA is a button whose visible text is "New project"
    // (distinct from the header "+" icon button which has the same aria-label).
    const newProjectButtons = screen.getAllByRole("button", { name: "New project" });
    expect(newProjectButtons.length).toBeGreaterThanOrEqual(1);
    expect(newProjectButtons.some((b) => b.textContent === "New project")).toBe(true);
  });

  it("marks the selected project row with aria-current=page", () => {
    render(<Sidebar {...baseProps()} selection={{ kind: "project", projectId: "p1" }} />);
    const row = screen.getByLabelText(/Quarterly Review, project/);
    expect(row).toHaveAttribute("aria-current", "page");
  });

  it("clears the search field when the clear button is clicked", async () => {
    render(<Sidebar {...baseProps()} />);
    const search = screen.getByLabelText("Search projects and recordings") as HTMLInputElement;
    await userEvent.type(search, "abc");
    expect(search.value).toBe("abc");
    await userEvent.click(screen.getByLabelText("Clear search"));
    expect(search.value).toBe("");
  });
});
