// macOS-style title bar. Every child also has data-tauri-drag-region so
// the webview's drag hit-test resolves to a draggable element regardless
// of where the user clicks within the strip. Using pointer-events-none
// on children doesn't reliably fall through on macOS, so each child opts
// in explicitly instead.
export default function TitleBar() {
  return (
    <div
      data-tauri-drag-region
      className="h-11 flex items-center px-4 select-none shrink-0 bg-window-background"
    >
      {/* Leave space for native traffic lights on macOS */}
      <div data-tauri-drag-region className="w-16" />
      <div data-tauri-drag-region className="w-16" />
    </div>
  );
}
