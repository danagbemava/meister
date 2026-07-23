import { useRef } from "react";
import { useGlobalSettings } from "../lib/useGlobalSettings";
import { useScrollEdge } from "../lib/useScrollEdge";
import SettingsForm from "./SettingsForm";
import { Toolbar } from "./ui";

export default function SettingsWindow() {
  const { settings, update } = useGlobalSettings();
  const scrollRef = useRef<HTMLDivElement>(null);
  const scrolled = useScrollEdge(scrollRef);
  return (
    <div className="h-screen flex flex-col bg-window-background">
      {/* Empty drag strip: the traffic lights overlay this region
          (titleBarStyle Overlay + hiddenTitle), so it stays content-free. */}
      <Toolbar data-tauri-drag-region scrolled={scrolled} />
      <div ref={scrollRef} className="flex-1 overflow-y-auto p-8 pt-2">
        <h1 className="font-display text-[22px] font-semibold mb-6">Settings</h1>
        {settings ? (
          <SettingsForm settings={settings} onChange={(s) => void update(s)} />
        ) : (
          <p className="text-label-tertiary">Loading…</p>
        )}
      </div>
    </div>
  );
}
