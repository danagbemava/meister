import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { Icon, type IconName } from "../Icon";

const ALL_NAMES: IconName[] = [
  "mic", "waveform", "doc-text", "media", "sparkles", "equalizer", "save",
  "lock", "warning",
  "play", "pause", "stop", "trash",
  "plus", "x", "check", "pencil", "chevron-right",
  "folder", "recording",
  "gear", "question", "info",
  "sidebar",
];

describe("Icon", () => {
  it("renders an svg with width/height matching the size prop", () => {
    const sizes: Array<["sm" | "md" | "lg", string]> = [
      ["sm", "14"],
      ["md", "20"],
      ["lg", "32"],
    ];
    for (const [size, px] of sizes) {
      const { container, unmount } = render(<Icon name="mic" size={size} />);
      const svg = container.querySelector("svg");
      expect(svg, `size=${size} renders an svg`).not.toBeNull();
      expect(svg?.getAttribute("width")).toBe(px);
      expect(svg?.getAttribute("height")).toBe(px);
      unmount();
    }
  });

  it("defaults to size md (20px)", () => {
    const { container } = render(<Icon name="mic" />);
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("width")).toBe("20");
  });

  it("is decorative by default (aria-hidden, no role=img)", () => {
    const { container } = render(<Icon name="warning" />);
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("aria-hidden")).toBe("true");
    expect(svg?.getAttribute("role")).not.toBe("img");
  });

  it("becomes labeled when aria-label is provided", () => {
    const { container } = render(<Icon name="warning" aria-label="Warning" />);
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("aria-hidden")).not.toBe("true");
    expect(svg?.getAttribute("aria-label")).toBe("Warning");
  });

  it("forwards className to the svg", () => {
    const { container } = render(
      <Icon name="mic" className="text-status-error" />,
    );
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("class") ?? "").toContain("text-status-error");
  });

  it.each(ALL_NAMES)("renders without throwing for name=%s", (name) => {
    const { container } = render(<Icon name={name} />);
    expect(container.querySelector("svg")).not.toBeNull();
  });
});
