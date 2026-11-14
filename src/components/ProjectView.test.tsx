import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../api/storage", () => ({
  api: {
    listProjects: vi.fn(),
    listRecordings: vi.fn(),
    updateProject: vi.fn(),
    deleteProject: vi.fn(),
    createRecording: vi.fn(),
  },
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  confirm: vi.fn(),
  open: vi.fn(),
}));

import { api } from "../api/storage";
import type { Project, Recording, StageRecord } from "../types/storage";
import ProjectView from "./ProjectView";

function makeProject(overrides: Partial<Project> = {}): Project {
  return {
    id: "proj-1",
    name: "Demo Project",
    createdAt: 1_700_000_000_000,
    updatedAt: 1_700_000_000_000,
    template: { source: "inline", content: "" },
    ...overrides,
  };
}

function makeRecording(
  id: string,
  name: string,
  stages: StageRecord[] = [],
): Recording {
  return {
    id,
    projectId: "proj-1",
    name,
    sourcePaths: ["/tmp/a.wav"],
    createdAt: 1_700_000_000_000,
    updatedAt: 1_700_000_000_000,
    stages,
  };
}

const noop = () => {};

function renderView(props: Partial<React.ComponentProps<typeof ProjectView>> = {}) {
  return render(
    <ProjectView
      projectId="proj-1"
      onProjectChanged={noop}
      onRecordingCreated={noop}
      onSelectRecording={noop}
      {...props}
    />,
  );
}

beforeEach(() => {
  vi.mocked(api.listRecordings).mockReset().mockResolvedValue([]);
  vi.mocked(api.deleteProject).mockReset().mockResolvedValue(undefined);
  vi.mocked(api.updateProject).mockReset().mockResolvedValue(undefined);
});

describe("ProjectView header + template collapse", () => {
  it("expands the template card when the template is empty", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([
      makeProject({ template: { source: "inline", content: "" } }),
    ]);
    renderView();
    // TemplateEditor's textarea (the "Format Template" body) is visible when expanded.
    await waitFor(() =>
      expect(screen.getByPlaceholderText(/Meeting Minutes/i)).toBeInTheDocument(),
    );
  });

  it("collapses the template card when the template is configured", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([
      makeProject({
        template: { source: "inline", content: "# Minutes\nDate: [DATE]" },
      }),
    ]);
    renderView();
    // The template header toggle is collapsed (aria-expanded false) for a
    // configured template, so the editor body (textarea) is NOT mounted.
    const toggle = await screen.findByRole("button", { name: /format template/i });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByPlaceholderText(/Meeting Minutes/i)).not.toBeInTheDocument();
  });

  it("toggles the template body open when the header is clicked", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([
      makeProject({
        template: { source: "inline", content: "# Minutes" },
      }),
    ]);
    renderView();
    const toggle = await screen.findByRole("button", { name: /format template/i });
    expect(screen.queryByPlaceholderText(/Meeting Minutes/i)).not.toBeInTheDocument();
    await userEvent.click(toggle);
    expect(screen.getByPlaceholderText(/Meeting Minutes/i)).toBeInTheDocument();
  });
});

describe("ProjectView recordings rows", () => {
  it("renders a row per recording and calls onSelectRecording on click", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([makeProject()]);
    vi.mocked(api.listRecordings).mockResolvedValue([
      makeRecording("rec-1", "Kickoff call", [
        { stage: "extract", status: "completed" },
        { stage: "transcribe", status: "completed" },
        { stage: "format", status: "completed" },
      ]),
      makeRecording("rec-2", "Standup", []),
    ]);
    const onSelect = vi.fn();
    renderView({ onSelectRecording: onSelect });

    const row = await screen.findByRole("button", { name: /Kickoff call/i });
    expect(screen.getByRole("button", { name: /Standup/i })).toBeInTheDocument();
    await userEvent.click(row);
    expect(onSelect).toHaveBeenCalledWith("rec-1");
  });

  it("shows the 'Not processed yet' status line for a freshly created recording (empty stages)", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([makeProject()]);
    vi.mocked(api.listRecordings).mockResolvedValue([
      makeRecording("rec-new", "Fresh recording", []),
    ]);
    renderView();

    const row = await screen.findByRole("button", { name: /Fresh recording/i });
    // A new recording (no stages) must NOT report itself as transcribed/formatted.
    expect(row).not.toHaveTextContent(/Transcribed/i);
    const status = within(row).getByText("Not processed yet");
    expect(status).toHaveClass("text-label-tertiary");
  });

  it("shows the empty-state line when there are no recordings", async () => {
    vi.mocked(api.listProjects).mockResolvedValue([makeProject()]);
    vi.mocked(api.listRecordings).mockResolvedValue([]);
    renderView();
    await waitFor(() =>
      expect(screen.getByText(/No recordings yet/i)).toBeInTheDocument(),
    );
  });
});
