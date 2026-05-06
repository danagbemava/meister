import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "./ui";
import RecordingWaveform from "./RecordingWaveform";
import { useMicAmplitude } from "../lib/useMicAmplitude";

type CaptureState = "idle" | "recording" | "paused" | "saving" | "error";

interface Props {
  onPicked: (filePaths: string[]) => void;
  /** Fired when a capture becomes active; pass `null` when it ends. The parent
   *  uses this to install a navigation guard (confirm before leaving). */
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

function pickMime(): string | undefined {
  if (typeof MediaRecorder === "undefined") return undefined;
  if (MediaRecorder.isTypeSupported("audio/webm;codecs=opus")) {
    return "audio/webm;codecs=opus";
  }
  if (MediaRecorder.isTypeSupported("audio/webm")) {
    return "audio/webm";
  }
  return undefined;
}

export default function RecordCard({ onPicked, onActiveChange }: Props) {
  const [state, setState] = useState<CaptureState>("idle");
  const [errorMsg, setErrorMsg] = useState<string>("");
  const [elapsedMs, setElapsedMs] = useState(0);
  const [stream, setStream] = useState<MediaStream | null>(null);

  const micAmp = useMicAmplitude(stream, state === "recording");

  const recorderRef = useRef<MediaRecorder | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const startedAtRef = useRef<number>(0);
  const accumulatedRef = useRef<number>(0);
  const tickerRef = useRef<number | null>(null);

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

  const stopStreamAndRecorder = () => {
    stopTicker();
    try { recorderRef.current?.stop(); } catch { /* ignore */ }
    recorderRef.current = null;
    streamRef.current?.getTracks().forEach((t) => t.stop());
    streamRef.current = null;
    setStream(null);
  };

  useEffect(() => {
    return () => { stopStreamAndRecorder(); };
  }, []);

  const discard = () => {
    const r = recorderRef.current;
    if (!r) {
      chunksRef.current = [];
      setState("idle");
      setElapsedMs(0);
      onActiveChange?.(null);
      return;
    }
    (r as MediaRecorder & { _discarded?: boolean })._discarded = true;
    r.stop();
  };

  const start = async () => {
    setErrorMsg("");
    setElapsedMs(0);
    accumulatedRef.current = 0;
    chunksRef.current = [];
    try {
      const mediaStream = await navigator.mediaDevices.getUserMedia({ audio: true });
      streamRef.current = mediaStream;
      setStream(mediaStream);
      const mime = pickMime();
      const recorder = mime
        ? new MediaRecorder(mediaStream, { mimeType: mime })
        : new MediaRecorder(mediaStream);
      recorderRef.current = recorder;

      recorder.ondataavailable = (e) => {
        if (e.data.size > 0) chunksRef.current.push(e.data);
      };

      recorder.onstop = async () => {
        stopTicker();
        const wasDiscarded = (recorder as MediaRecorder & { _discarded?: boolean })._discarded === true;
        streamRef.current?.getTracks().forEach((t) => t.stop());
        streamRef.current = null;
        setStream(null);
        recorderRef.current = null;
        onActiveChange?.(null);

        if (wasDiscarded) {
          chunksRef.current = [];
          setState("idle");
          return;
        }

        setState("saving");
        try {
          const blob = new Blob(chunksRef.current, {
            type: recorder.mimeType || "audio/webm",
          });
          const buf = await blob.arrayBuffer();
          const path = await invoke<string>("save_capture", {
            bytes: Array.from(new Uint8Array(buf)),
          });
          chunksRef.current = [];
          setState("idle");
          setElapsedMs(0);
          onPicked([path]);
        } catch (e) {
          setState("error");
          setErrorMsg(`Could not save: ${String(e)}`);
        }
      };

      recorder.start();
      startedAtRef.current = Date.now();
      startTicker();
      setState("recording");
      onActiveChange?.(discard);
    } catch (e) {
      const err = e as DOMException | Error;
      setState("error");
      if ("name" in err && err.name === "NotAllowedError") {
        setErrorMsg("Microphone access denied. Open System Settings → Privacy & Security → Microphone to allow.");
      } else if ("name" in err && err.name === "NotFoundError") {
        setErrorMsg("No microphone detected. Plug in a mic and click Retry.");
      } else {
        setErrorMsg(String(err));
      }
    }
  };

  const pause = () => {
    const r = recorderRef.current;
    if (!r || r.state !== "recording") return;
    r.pause();
    stopTicker();
    accumulatedRef.current += Date.now() - startedAtRef.current;
    setState("paused");
  };

  const resume = () => {
    const r = recorderRef.current;
    if (!r || r.state !== "paused") return;
    r.resume();
    startedAtRef.current = Date.now();
    startTicker();
    setState("recording");
  };

  const stop = () => {
    const r = recorderRef.current;
    if (!r) return;
    r.stop();
  };

  const retry = () => {
    setState("idle");
    setErrorMsg("");
  };

  const cardBase = "rounded-xl border-2 flex flex-col items-center justify-center gap-2 py-10 px-4 transition-colors";

  if (state === "idle") {
    return (
      <div
        className={`${cardBase} border-dashed border-separator hover:border-separator bg-control-background cursor-pointer`}
        onClick={start}
      >
        <span className="text-3xl opacity-30">🎙</span>
        <p className="text-body text-label-secondary">Record audio</p>
        <p className="text-footnote text-label-tertiary">Click to start</p>
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
          samples={micAmp.samples}
          state={state === "recording" ? "recording" : "paused"}
          className="h-16"
        />

        <div className="w-full flex items-center gap-2">
          {state === "recording" ? (
            <Button onClick={pause} variant="secondary" size="md" aria-label="Pause">⏸</Button>
          ) : (
            <Button onClick={resume} variant="primary" size="md">▶ Resume</Button>
          )}
          <Button onClick={stop} variant={state === "paused" ? "secondary" : "primary"} size="md">
            ◼ Stop
          </Button>
          <Button
            onClick={discard}
            variant="ghost"
            size="md"
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
        <p className="text-body text-label-secondary">Saving…</p>
      </div>
    );
  }

  return (
    <div className={`${cardBase} border-solid border-status-error/30 bg-status-error/10`}>
      <span className="text-3xl opacity-50">⚠</span>
      <p className="text-body text-status-error text-center px-2">{errorMsg}</p>
      <Button onClick={retry} variant="secondary" size="md" className="mt-2">Retry</Button>
    </div>
  );
}
