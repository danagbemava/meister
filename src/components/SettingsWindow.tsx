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
      <Toolbar data-tauri-drag-region scrolled={scrolled}>
        <h1 className="font-display text-[22px] font-semibold">Settings</h1>
      </Toolbar>
      <div ref={scrollRef} className="flex-1 overflow-y-auto p-8">
        {settings ? (
          <SettingsForm settings={settings} onChange={(s) => void update(s)} />
        ) : (
          <p className="text-label-tertiary">Loading…</p>
        )}
      </div>
    </div>
  );
}
