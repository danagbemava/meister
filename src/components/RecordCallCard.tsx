import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "../api/storage";
import { Button, Checkbox, Icon } from "./ui";
import CaptureLive from "./CaptureLive";
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
  /** When provided, the idle card shows a cancel control and a completed
   *  discard returns the user to the pills via this callback. */
  onCancel?: () => void;
}


export default function RecordCallCard({ onPicked, onActiveChange, onCancel }: Props) {
  const [state, setState] = useState<CaptureState>("idle");
  const [errorMsg, setErrorMsg] = useState<string>("");
  const [micError, setMicError] = useState<string>("");
  const [streamWarning, setStreamWarning] = useState<string>("");
  const [includeMic, setIncludeMic] = useState(true);
  const [elapsedMs, setElapsedMs] = useState(0);

  const sessionRef = useRef<string | null>(null);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const startedAtRef = useRef<number>(0);
  const accumulatedRef = useRef<number>(0);
  const tickerRef = useRef<number | null>(null);

  const sysAmp = useTauriAmplitude(sessionId, "system");
  const micAmp = useTauriAmplitude(sessionId, "mic");
  const [micOn, setMicOn] = useState(false);

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

  const onActiveChangeRef = useRef(onActiveChange);
  onActiveChangeRef.current = onActiveChange;

  const releaseSession = () => {
    const id = sessionRef.current;
    stopTicker();
    sessionRef.current = null;
    setSessionId(null);
    if (!id) return;
    onActiveChangeRef.current?.(null);
    api.discardCallCapture(id).catch(() => {});
  };

  useEffect(() => releaseSession, []);

  // The backend emits this when the ScreenCaptureKit stream dies mid-session
  // (display sleep, screen lock, replayd restart). Recording keeps running but
  // system audio is gone from that point — tell the user right away.
  useEffect(() => {
    if (!sessionId) {
      setStreamWarning("");
      return;
    }
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;
    listen<{ sessionId: string; error: string | null }>(
      "call-capture-stream-stopped",
      (event) => {
        if (cancelled || event.payload.sessionId !== sessionId) return;
        setStreamWarning(
          "System audio capture stopped unexpectedly — audio from this point on is not being recorded. Stop and save now.",
        );
      },
    )
      .then((fn) => {
        if (cancelled) {
          fn();
          return;
        }
        unlisten = fn;
      })
      .catch((e) => {
        console.warn("RecordCallCard: stream-stopped listen failed", e);
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [sessionId]);

  const resetToIdle = () => {
    releaseSession();
    setState("idle");
    setErrorMsg("");
    setMicError("");
    setElapsedMs(0);
    accumulatedRef.current = 0;
  };

  const discard = () => {
    resetToIdle();
    onCancel?.();
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
      setMicOn(includeMic && result.micOk);
      sessionRef.current = result.sessionId;
      setSessionId(result.sessionId);
      startedAtRef.current = Date.now();
      startTicker();
      setState("recording");
      onActiveChange?.(discard);
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
    sessionRef.current = null;
    setSessionId(null);
    onActiveChange?.(null);
    setState("saving");
    try {
      const path = await api.stopCallCapture(id);
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

  const retry = resetToIdle;

  const cardBase =
    "rounded-card border flex flex-col items-center justify-center gap-2 py-10 px-4 transition-colors";

  if (state === "idle") {
    return (
      <div className={`${cardBase} relative border-dashed border-separator bg-fill-quaternary`}>
        {onCancel && (
          <Button
            variant="ghost"
            size="sm"
            iconLeft="x"
            aria-label="Cancel"
            className="absolute top-2 right-2 text-label-tertiary hover:bg-fill-tertiary"
            onClick={onCancel}
          />
        )}
        <button
          type="button"
          onClick={start}
          className="flex flex-col items-center gap-2 rounded-control px-6 py-3 text-label-secondary hover:bg-fill-tertiary hover:text-label-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus-ring transition-colors"
        >
          <Icon name="waveform" size="lg" />
          <span className="text-body font-medium">Start recording call</span>
        </button>
        <label className="flex items-center gap-2 text-footnote text-label-secondary select-none">
          <Checkbox
            size="sm"
            checked={includeMic}
            onCheckedChange={(c) => setIncludeMic(Boolean(c))}
          />
          Include my microphone
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
      <CaptureLive
        live={state === "recording"}
        elapsedMs={elapsedMs}
        meters={[
          { label: "Call audio", samples: sysAmp.samples },
          ...(micOn ? [{ label: "Your microphone", samples: micAmp.samples }] : []),
        ]}
        warning={streamWarning || micError}
        onStop={stop}
        onPause={pause}
        onResume={resume}
        onDiscard={discard}
      />
    );
  }

  if (state === "saving") {
    return (
      <div className={`${cardBase} border-solid border-separator bg-fill-quaternary`}>
        <span className="spinner" />
        <p className="text-body text-label-secondary">Saving recording…</p>
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

