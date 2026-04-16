import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

interface Props {
  size: string;
  totalBytesEstimate: number;
  onComplete: () => void;
  onCancel: () => void;
}

interface ProgressEvent {
  size: string;
  bytes_downloaded: number;
  bytes_total: number;
}

type State = "confirm" | "downloading" | "error";

function formatGB(bytes: number) {
  return (bytes / (1024 ** 3)).toFixed(1) + " GB";
}

function formatMB(bytes: number) {
  return (bytes / (1024 ** 2)).toFixed(0) + " MB";
}

export default function DownloadModelModal({ size, totalBytesEstimate, onComplete, onCancel }: Props) {
  const [state, setState] = useState<State>("confirm");
  const [bytesDone, setBytesDone] = useState(0);
  const [bytesTotal, setBytesTotal] = useState(totalBytesEstimate);
  const [errorMsg, setErrorMsg] = useState("");

  useEffect(() => {
    if (state !== "downloading") return;
    let unlisten: UnlistenFn | undefined;
    listen<ProgressEvent>("model-download-progress", (e) => {
      if (e.payload.size !== size) return;
      setBytesDone(e.payload.bytes_downloaded);
      setBytesTotal(e.payload.bytes_total || totalBytesEstimate);
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, [state, size, totalBytesEstimate]);

  const startDownload = async () => {
    setState("downloading");
    setBytesDone(0);
    try {
      await invoke("download_model", { size });
      onComplete();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg === "cancelled") {
        onCancel();
      } else {
        setErrorMsg(msg);
        setState("error");
      }
    }
  };

  const cancel = async () => {
    if (state === "downloading") {
      try { await invoke("cancel_model_download", { size }); } catch { /* ignore */ }
    }
    onCancel();
  };

  const pct = bytesTotal > 0 ? Math.min(100, Math.floor((bytesDone / bytesTotal) * 100)) : 0;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="bg-surface rounded-xl shadow-2xl w-[420px] p-6 flex flex-col gap-4">
        {state === "confirm" && (
          <>
            <h2 className="text-lg font-semibold">Download {size} model?</h2>
            <p className="text-sm text-white/70">
              This is a one-time download of <strong>{formatGB(totalBytesEstimate)}</strong>.
              The bundled <code className="font-mono">base</code> model will continue to work in the meantime.
            </p>
            <div className="flex justify-end gap-2 mt-2">
              <button onClick={cancel}
                className="px-4 py-2 rounded-lg bg-surface-secondary hover:bg-surface-tertiary text-sm">Cancel</button>
              <button onClick={startDownload}
                className="px-4 py-2 rounded-lg bg-accent hover:bg-accent-hover text-sm font-semibold">Download</button>
            </div>
          </>
        )}

        {state === "downloading" && (
          <>
            <h2 className="text-lg font-semibold">Downloading {size}…</h2>
            <div className="h-2 bg-surface-secondary rounded overflow-hidden">
              <div className="h-full bg-accent transition-all" style={{ width: `${pct}%` }} />
            </div>
            <p className="text-xs text-white/60 font-mono">
              {formatMB(bytesDone)} / {formatMB(bytesTotal)} ({pct}%)
            </p>
            <div className="flex justify-end mt-2">
              <button onClick={cancel}
                className="px-4 py-2 rounded-lg bg-surface-secondary hover:bg-surface-tertiary text-sm">Cancel</button>
            </div>
          </>
        )}

        {state === "error" && (
          <>
            <h2 className="text-lg font-semibold text-red-400">Download failed</h2>
            <p className="text-sm text-white/70 break-words">{errorMsg}</p>
            <div className="flex justify-end gap-2 mt-2">
              <button onClick={onCancel}
                className="px-4 py-2 rounded-lg bg-surface-secondary hover:bg-surface-tertiary text-sm">Close</button>
              <button onClick={startDownload}
                className="px-4 py-2 rounded-lg bg-accent hover:bg-accent-hover text-sm font-semibold">Retry</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
