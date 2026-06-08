import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("../api/ask", () => ({
  askApi: { query: vi.fn(), cancel: vi.fn() },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { askApi } from "../api/ask";
import { __resetAskStoreForTests } from "../lib/useAsk";
import AskPanel from "./AskPanel";

beforeEach(() => {
  __resetAskStoreForTests();
  vi.mocked(askApi.query).mockReset();
  vi.mocked(askApi.cancel).mockReset();
});

describe("AskPanel", () => {
  it("shows the empty-state hint when there are no threads", () => {
    render(<AskPanel recordingId="rec-1" disabled={false} onCitationClick={() => {}} onClose={() => {}} />);
    expect(screen.getByText(/Ask anything about this transcript/i)).toBeInTheDocument();
  });

  it("disables the send button when input is whitespace", async () => {
    render(<AskPanel recordingId="rec-1" disabled={false} onCitationClick={() => {}} onClose={() => {}} />);
    const send = screen.getByRole("button", { name: /send/i });
    expect(send).toBeDisabled();
    await userEvent.type(screen.getByRole("textbox"), "   ");
    expect(send).toBeDisabled();
    await userEvent.type(screen.getByRole("textbox"), "hi");
    expect(send).toBeEnabled();
  });

  it("renders an answer with clickable citation pills", async () => {
    vi.mocked(askApi.query).mockResolvedValue({
      text: "It happened in Q3.",
      citations: [{ quote: "Q3 budget review", start: 5, end: 21 }],
    });
    const onCite = vi.fn();
    render(<AskPanel recordingId="rec-1" disabled={false} onCitationClick={onCite} onClose={() => {}} />);
    await userEvent.type(screen.getByRole("textbox"), "when?");
    await userEvent.click(screen.getByRole("button", { name: /send/i }));
    expect(await screen.findByText(/It happened in Q3/i)).toBeInTheDocument();
    // The pill renders as a `<button>` whose accessible name is the quote
    // wrapped in curly quotes — match by the inner text via regex.
    const pill = screen.getByRole("button", { name: /Q3 budget review/i });
    await userEvent.click(pill);
    expect(onCite).toHaveBeenCalledWith({ quote: "Q3 budget review", start: 5, end: 21 });
  });

  it("renders unmatched citations as non-clickable", async () => {
    vi.mocked(askApi.query).mockResolvedValue({
      text: "answer.",
      citations: [{ quote: "untraceable", start: null, end: null }],
    });
    render(<AskPanel recordingId="rec-1" disabled={false} onCitationClick={() => {}} onClose={() => {}} />);
    await userEvent.type(screen.getByRole("textbox"), "any?");
    await userEvent.click(screen.getByRole("button", { name: /send/i }));
    await screen.findByText(/answer\./);
    // The unmatched pill is rendered as a plain element, not a button. The
    // visible text is wrapped in curly quotes, so use a regex matcher.
    expect(screen.queryByRole("button", { name: /untraceable/i })).toBeNull();
    expect(screen.getByText(/untraceable/)).toBeInTheDocument();
  });

  it("disables input when the recording has no transcript yet", () => {
    render(<AskPanel recordingId="rec-1" disabled={true} onCitationClick={() => {}} onClose={() => {}} />);
    expect(screen.getByRole("textbox")).toBeDisabled();
    expect(screen.getByRole("button", { name: /send/i })).toBeDisabled();
  });

  it("dismisses a thread via its × button", async () => {
    vi.mocked(askApi.query).mockResolvedValue({ text: "answer.", citations: [] });
    render(<AskPanel recordingId="rec-1" disabled={false} onCitationClick={() => {}} onClose={() => {}} />);
    await userEvent.type(screen.getByRole("textbox"), "q?");
    await userEvent.click(screen.getByRole("button", { name: /send/i }));
    await screen.findByText(/answer\./);
    // The dismiss button is in the thread item.
    const dismiss = screen.getByRole("button", { name: /dismiss thread/i });
    await userEvent.click(dismiss);
    // Thread is gone; only the empty-state hint remains.
    expect(screen.getByText(/Ask anything about this transcript/i)).toBeInTheDocument();
  });

  it("calls onClose when Escape is pressed", () => {
    const onClose = vi.fn();
    render(
      <AskPanel
        recordingId="rec-1"
        disabled={false}
        onCitationClick={() => {}}
        onClose={onClose}
      />,
    );
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });

  it("renders a visible close button that calls onClose", async () => {
    const onClose = vi.fn();
    render(
      <AskPanel
        recordingId="rec-1"
        disabled={false}
        onCitationClick={() => {}}
        onClose={onClose}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: /close ask/i }));
    expect(onClose).toHaveBeenCalled();
  });
});
