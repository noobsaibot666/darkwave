---
title: Bug Log v2
description: Internal dev-session log, not user-facing documentation.
---

Fresh session, continuing from `bug-log.md` (not edited — that one stays as
the record of the import/analysis session). This covers general usability
pass: clicking through the app's main functions. Logging only, no fixes.

---

## OPEN-1 — Main Play/Pause (transport) button plays the wrong track instead of stopping the current one, if a different row is merely selected

- **Reported:** by user — "I wasn't able to stop a sound... after [selecting
  a different track] I could stop in another track."
- **Status:** OPEN — root cause identified, exact repro understood

**Repro that matches the report:**
1. Play track A (e.g. via its row's play button, or the transport bar).
2. Click on a *different* row, track B, anywhere except its own play
   button — e.g. to view its tags/metadata. This only selects B, it does
   not start playing it.
3. Press the main transport Play/Pause button (or the spacebar shortcut)
   intending to stop track A, which is still audibly playing.
4. **Nothing stops.** Instead, track B starts loading and playing on top
   of/instead of A.
5. Now that B is both selected and playing, pressing Play/Pause again
   correctly pauses B — which reads as "I could stop it in another track."

**Root cause:** `togglePlayback`
(`apps/desktop/src-ui/src/App.tsx:1181-1194`) decides whether to
pause/resume vs. load-and-play a new track by comparing
`playingAssetId === selectedAssetId`:

```ts
if (playingAssetId && playingAssetId === selectedAssetId) {
  if (audio.paused) audio.play().catch(() => {});
  else audio.pause();
  return;
}
if (selectedAsset) {
  loadAssetForPlayback(selectedAsset, true);
}
```

Selecting a row (`handleRowClick`, sets `selectedAssetId` only — confirmed
no playback side effect at that call site) is a completely separate action
from playing one, so `selectedAssetId` and `playingAssetId` routinely
diverge — merely browsing the list while something plays is the normal
case, not an edge case. Once they diverge, this function stops being a
"pause what's playing" control and instead becomes "load whatever row
happens to be selected," discarding the actually-playing track without
ever pausing it.

Contrast with the per-row play button (`App.tsx:2818-2827`), which gets
this right — it checks `playingAssetId === asset.id` (the *specific
row's* id), not `selectedAssetId`:

```ts
if (playingAssetId === asset.id) {
  const audio = audioRef.current;
  if (audio) (audio.paused ? audio.play() : audio.pause());
} else {
  loadAssetForPlayback(asset, true);
}
```

Same bug reachable via the spacebar shortcut too
(`App.tsx:2040-2043`, `TogglePlayback` binding calls the same
`togglePlayback`).

**Impact:** any time a user clicks around the library while a track is
playing — which is normal browsing behavior, not a rare action — the main
transport button (and spacebar) stops reliably controlling the track
they're listening to and silently redirects to whatever row is currently
highlighted. Reads exactly like "can't stop the sound."

**Suggested direction (not implemented — logging only):** have
`togglePlayback` key off `playingAssetId` alone (pause/resume whatever is
actually loaded in `audioRef`), independent of `selectedAssetId` — matching
the per-row button's already-correct logic — and only fall through to
loading a new track when nothing is currently playing at all.

---
