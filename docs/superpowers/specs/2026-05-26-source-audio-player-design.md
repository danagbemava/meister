# Source Audio Player — Design

**Status:** Draft
**Date:** 2026-05-26

## Problem

When a transcript looks wrong, there's no fast way to verify the source audio matches the user's expectation. The investigation path today is:

1. Open Finder to the source path.
2. Open it in QuickTime / VLC.
3. Scrub to a suspect region.
4. Switch back to Meister to compare against the transcript.

For the current Twi+English transcript debugging session, this is the bottleneck for ruling out "is the audio itself the problem?" from "is whisper hallucinating against good audio?". An in-app player short-circuits that round trip.

## Goals

- Play any source file listed on a recording, in-app, without leaving the recording view.
- Work for the file formats Meister already accepts as sources (`.mov`, `.mp4`, `.m4a`, `.webm`, `.wav`, …).
- Handle multi-hour files efficiently (seek without loading the whole file into memory).
- Multi-source recordings: each source independently playable.

## Non-goals

- **Playing the extracted WAV.** The WAV is deleted after transcribe completes (`2759714 fix(redo): transcribe redo resets from extract — WAVs deleted post-transcribe`), so it isn't on disk for most of a recording's lifetime. Source-only is enough for the immediate goal; verifying extraction faithfulness can be a separate feature if ffmpeg ever becomes suspect.
- **Sync between player position and transcript highlighting.** Meister's transcripts have no per-segment timestamps (`set_print_timestamps(false)` in `engine.rs`), so there's no anchor for time→text mapping.
- **Custom player chrome.** Native HTML5 `<audio controls>` is sufficient and matches the rest of Meister's "use the platform's native controls" disposition.
- **Video preview for `.mov`/`.mp4` sources.** Goal is audio verification; `<audio>` selects the audio track and is visually compact.
- **Persisting player open/closed state across navigations.** Local component state only.

## Approach

Three small changes, all in well-isolated layers:

1. **Tauri side — enable the asset protocol.** Add `app.security.assetProtocol` to `tauri.conf.json` with `enable: true` and an object-form `scope` that allows `**/*` with `requireLiteralLeadingDot: false` (so hidden-folder paths like `~/Library/...` aren't filtered out). No capability change needed — in Tauri 2 the asset protocol is gated only by `app.security.assetProtocol` + `scope`, not by per-window permissions. CSP isn't a concern either because `tauri.conf.json` has `"csp": null`.
2. **Frontend bridge — use `convertFileSrc`.** `@tauri-apps/api/core` already exposes `convertFileSrc(path)`. Wrap each source path with it to get a webview-loadable URL.
3. **UI — inline expander per source row.** Add a chevron toggle on each row in the existing `Sources` list. Expanded rows render `<audio controls preload="metadata" src={convertFileSrc(p)} />` directly below the path. State is a `Set<number>` of expanded indices, held in `RecordingView`.

### Why broad scope

The asset protocol `scope` gates which paths the webview can fetch. The chosen value (`allow: ["**/*"]`, `requireLiteralLeadingDot: false`) matches the rest of Meister's filesystem posture: source paths are already absolute user-chosen filesystem paths from the OS file picker — the user has already vouched for those paths. The `requireLiteralLeadingDot: false` part is important on macOS, where `app_data_dir` resolves under `~/Library/...` (a hidden-prefixed path); the default `true` setting would refuse to load anything inside it. A tighter scope (e.g. dynamic per-recording, or hardcoded `~/Movies`) would either complicate the build (Tauri 2's scope is configured at build time, not per-invocation) or silently break for sources outside the allowed dirs.

### Why `<audio>` not `<video>`

The webview's media stack will pick the audio track out of `.mov`/`.mp4` containers when bound to an `<audio>` element. Goal here is audio verification, and `<audio controls>` is one row tall vs. a multi-hundred-pixel video frame. No reason to take more space than the task requires.

### Why `preload="metadata"`

On a 2h `.mov`, `preload="auto"` would start buffering hundreds of MB as soon as the element renders. `metadata` fetches just enough to know duration and seek points; the rest streams on play.

## UI

Inside the existing `Sources` `<ul>` in `RecordingView.tsx:332-362`, each `<li>` gains:

```
[chevron] [#] [path .......................................... ] [Locate…?]
   (only if expanded:)
   ┌──────────────────────────────────────────────────────────────────┐
   │  ▷ 0:42 / 2:18:09  ●────────────────────────────────────── │
   └──────────────────────────────────────────────────────────────────┘
```

- Chevron is a small ghost button using the existing `Icon` and `Button` primitives.
- Expanded `<audio>` element uses `w-full h-8` to sit slim under the row.
- Chevron is hidden (not rendered) on rows whose `missingByPath[p]` is true — no playable file, no toggle.

## Error handling

If the `<audio>` element fires `onError` (codec the webview can't decode, file permission issue, etc.), surface the browser's `MediaError.message` in a single line of `text-status-error` below the player. No custom error UI.

## Testing

No automated test for this change. Playback is a webview + OS codec integration; mocking it doesn't tell us anything meaningful. Verification contract: open a recording, expand a source row, hear audio. Manual.

## Migration / compatibility

- Existing recordings: no change. Player just appears for any source that exists on disk.
- Settings window: untouched. No new user-visible setting.
- No data migration.

## Files changed

| File | Change |
|---|---|
| `src-tauri/tauri.conf.json` | Add `app.security.assetProtocol` block. |
| `src/components/RecordingView.tsx` | Per-row chevron + conditional `<audio>` element. New `expandedSources: Set<number>` state. |

## Risks

- **Scope `["**"]` is broad.** This is consistent with existing posture (source paths come from the OS file picker; the app already reads arbitrary user-selected files via `fs:allow-read-text-file`). Worth a note in the security review checklist but not a blocker.
- **Codec coverage isn't 100%.** The webview won't play every codec ffmpeg accepts. If a real source fails, the `onError` line tells the user what happened; they can still fall back to QuickTime. Acceptable.
- **Asset protocol misconfiguration is a foot-gun.** If the capability isn't added correctly, `convertFileSrc` URLs 404 with no obvious frontend signal. Implementation plan should include a smoke check.
