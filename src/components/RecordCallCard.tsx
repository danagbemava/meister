import { useEffect, useRef, useState } from "react";
import { api } from "../api/storage";
import { Button, Checkbox, Icon } from "./ui";
import RecordingWaveform from "./RecordingWaveform";
import { useTauriAmplitude } from "../lib/useTauriAmplitude";

type CaptureState =
  | "idle"
  | "permission-needed"
  | "recording"
  | "paused"
  | "saving"
  | "error";

interface Props {
  onPicked: (filePaths: string[]) => void;
  onActiveChange?: (discard: (() => void) | null) => void;
}

function formatElapsed(ms: number): string {
  const total = Math.floor(ms / 1000);
  const hh = Math.floor(total / 3600);
  const mm = Math.floor((total % 3600) / 60);
  const ss = total % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return hh > 0 ? `${pad(hh)}:${pad(mm)}:${pad(ss)}` : `${pad(mm)}:${pad(ss)}`;
}

export default function RecordCallCard({ onPicked, onActiveChange }: Props) {
  const [state, setState] = useState<CaptureState>("idle");
  const [errorMsg, setErrorMsg] = useState<string>("");
  const [micError, setMicError] = useState<string>("");
  const [includeMic, setIncludeMic] = useState(true);
  const [elapsedMs, setElapsedMs] = useState(0);

  const sessionRef = useRef<string | null>(null);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const startedAtRef = useRef<number>(0);
  const accumulatedRef = useRef<number>(0);
  const tickerRef = useRef<number | null>(null);

  const sysAmp = useTauriAmplitude(sessionId);

  const stopTicker = () => {
    if (tickerRef.current !== null) {
      window.clearInterval(tickerRef.current);
      tickerRef.current = null;
    }
  };

  const startTicker = () => {
    stopTicker();
    tickerRef.current = window.setInterval(() => {
      setElapsedMs(accumulatedRef.current + (Date.now() - startedAtRef.current));
    }, 200);
  };

  useEffect(() => {
    return () => { stopTicker(); };
  }, []);

  const discard = async () => {
    const id = sessionRef.current;
    stopTicker();
    sessionRef.current = null;
    setSessionId(null);
    onActiveChange?.(null);
    if (id) {
      try { await api.discardCallCapture(id); } catch { /* best-effort */ }
    }
    setState("idle");
    setElapsedMs(0);
    accumulatedRef.current = 0;
  };

  const start = async () => {
    setErrorMsg("");
    setMicError("");
    setElapsedMs(0);
    accumulatedRef.current = 0;
    try {
      const result = await api.startCallCapture(includeMic);
      if (!result.systemOk) {
        setState("permission-needed");
        return;
      }
      if (includeMic && !result.micOk) {
        setMicError(result.micError ?? "Microphone unavailable — recording system audio only.");
      }
      sessionRef.current = result.sessionId;
      setSessionId(result.sessionId);
      startedAtRef.current = Date.now();
      startTicker();
      setState("recording");
      onActiveChange?.(() => { void discard(); });
    } catch (e) {
      setState("error");
      setErrorMsg(String(e));
    }
  };

  const pause = async () => {
    const id = sessionRef.current;
    if (!id) return;
    try {
      await api.pauseCallCapture(id);
      stopTicker();
      accumulatedRef.current += Date.now() - startedAtRef.current;
      setState("paused");
    } catch (e) {
      setErrorMsg(String(e));
      setState("error");
    }
  };

  const resume = async () => {
    const id = sessionRef.current;
    if (!id) return;
    try {
      await api.resumeCallCapture(id);
      startedAtRef.current = Date.now();
      startTicker();
      setState("recording");
    } catch (e) {
      setErrorMsg(String(e));
      setState("error");
    }
  };

  const stop = async () => {
    const id = sessionRef.current;
    if (!id) return;
    stopTicker();
    setState("saving");
    try {
      const path = await api.stopCallCapture(id);
      sessionRef.current = null;
      setSessionId(null);
      onActiveChange?.(null);
      setElapsedMs(0);
      accumulatedRef.current = 0;
      setState("idle");
      onPicked([path]);
    } catch (e) {
      setErrorMsg(String(e));
      setState("error");
    }
  };

  const openSettings = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-shell");
      await open(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
      );
    } catch { /* ignore */ }
  };

  const retry = () => {
    setState("idle");
    setErrorMsg("");
    setMicError("");
  };

  const cardBase =
    "rounded-xl border-2 flex flex-col items-center justify-center gap-2 py-10 px-4 transition-colors";

  if (state === "idle") {
    return (
      <div
        className={`${cardBase} border-dashed border-separator hover:border-separator bg-control-background cursor-pointer`}
        onClick={start}
      >
        <Icon name="waveform" size="lg" className="text-label-tertiary" />
        <p className="text-body text-label-secondary">Record call</p>
        <label
          className="flex items-center gap-2 text-footnote text-label-secondary cursor-pointer select-none"
          onClick={(e) => e.stopPropagation()}
        >
          <Checkbox
            size="sm"
            checked={includeMic}
            onCheckedChange={(c) => setIncludeMic(Boolean(c))}
          />
          Include microphone
        </label>
      </div>
    );
  }

  if (state === "permission-needed") {
    return (
      <div className={`${cardBase} border-solid border-status-warning/40 bg-status-warning/10`}>
        <Icon name="lock" size="lg" className="text-status-warning/80" />
        <p className="text-body text-status-warning text-center px-2">
          Screen Recording permission required.
        </p>
        <p className="text-footnote text-label-secondary text-center px-2">
          Grant permission in System Settings, then relaunch Meister.
        </p>
        <div className="flex gap-2 mt-2">
          <Button onClick={openSettings} variant="primary" size="md">
            Open Settings
          </Button>
          <Button onClick={retry} variant="secondary" size="md">
            Retry
          </Button>
        </div>
      </div>
    );
  }

  if (state === "recording" || state === "paused") {
    return (
      <div className={`${cardBase} !items-stretch !gap-3 border-solid border-separator bg-control-background`}>
        <div className="w-full flex items-center gap-2">
          <span
            className={`w-2 h-2 rounded-full ${
              state === "recording"
                ? "bg-status-error animate-pulse"
                : "bg-status-warning"
            }`}
          />
          <span className="text-callout font-mono tabular-nums text-label-secondary">
            {formatElapsed(elapsedMs)}
          </span>
          {state === "paused" && (
            <span className="text-footnote text-label-tertiary ml-2">Paused</span>
          )}
        </div>

        <RecordingWaveform
          samples={sysAmp.samples}
          state={state === "recording" ? "recording" : "paused"}
          className="h-16"
        />

        {micError && (
          <p className="text-footnote text-status-warning">{micError}</p>
        )}

        <div className="w-full flex items-center gap-2">
          {state === "recording" ? (
            <Button onClick={pause} variant="secondary" size="md" iconLeft="pause" aria-label="Pause" />
          ) : (
            <Button onClick={resume} variant="primary" size="md" iconLeft="play">Resume</Button>
          )}
          <Button onClick={stop} variant={state === "paused" ? "secondary" : "primary"} size="md" iconLeft="stop">
            Stop
          </Button>
          <Button
            onClick={discard}
            variant="ghost"
            size="md"
            iconLeft="trash"
            className="ml-auto hover:bg-status-error/20"
          >
            Discard
          </Button>
        </div>
      </div>
    );
  }

  if (state === "saving") {
    return (
      <div className={`${cardBase} border-solid border-separator bg-control-background`}>
        <span className="spinner" />
        <p className="text-body text-label-secondary">Mixing tracks…</p>
      </div>
    );
  }

  return (
    <div className={`${cardBase} border-solid border-status-error/30 bg-status-error/10`}>
      <Icon name="warning" size="lg" className="text-status-error/70" />
      <p className="text-body text-status-error text-center px-2">{errorMsg}</p>
      <Button onClick={retry} variant="secondary" size="md" className="mt-2">Retry</Button>
    </div>
  );
}
