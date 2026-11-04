import { describe, expect, it, vi } from "vitest";
import { act, render } from "@testing-library/react";
import RecordCard from "./RecordCard";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../lib/useMicAmplitude", () => ({
  useMicAmplitude: () => ({ samples: new Float32Array(8) }),
}));

describe("RecordCard autoStart", () => {
  it("stays mounted and starts recording once the microphone is granted", async () => {
    let grant!: (s: MediaStream) => void;
    const stream = { getTracks: () => [] } as unknown as MediaStream;
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: { getUserMedia: () => new Promise<MediaStream>((r) => { grant = r; }) },
    });
    class FakeRecorder {
      static isTypeSupported = () => true;
      mimeType = "audio/webm";
      state = "inactive";
      start() { this.state = "recording"; }
      stop() {}
    }
    vi.stubGlobal("MediaRecorder", FakeRecorder);

    const onDone = vi.fn();
    const onActiveChange = vi.fn();
    const { findByText } = render(
      <RecordCard autoStart onPicked={() => {}} onDone={onDone} onActiveChange={onActiveChange} />,
    );

    expect(onDone).not.toHaveBeenCalled();
    await act(async () => { grant(stream); });
    expect(await findByText("Recording")).toBeTruthy();
    expect(onDone).not.toHaveBeenCalled();
    expect(onActiveChange).toHaveBeenCalledWith(expect.any(Function));
  });
});
