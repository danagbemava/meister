import { useGlobalSettings } from "../lib/useGlobalSettings";
import SettingsForm from "./SettingsForm";

export default function SettingsWindow() {
  const { settings, update } = useGlobalSettings();
  return (
    <div className="h-screen overflow-y-auto p-8 bg-[rgb(var(--color-window-background-rgb))]">
      <h1 className="text-title-2 font-semibold mb-6" data-tauri-drag-region>Settings</h1>
      {settings ? (
        <SettingsForm settings={settings} onChange={(s) => void update(s)} />
      ) : (
        <p className="text-label-tertiary">Loading…</p>
      )}
    </div>
  );
}
