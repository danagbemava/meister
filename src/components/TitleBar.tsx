// macOS-style title bar — all children use pointer-events-none so
// clicks fall through to the data-tauri-drag-region container.
export default function TitleBar() {
  return (
    <div
      data-tauri-drag-region
      className="h-11 flex items-center px-4 select-none shrink-0"
    >
      {/* Leave space for native traffic lights on macOS */}
      <div className="w-16 pointer-events-none" />
      <span className="flex-1 text-center text-sm font-medium text-white/50 tracking-wide pointer-events-none">
        Meister
      </span>
      <div className="w-16 pointer-events-none" />
    </div>
  );
}
