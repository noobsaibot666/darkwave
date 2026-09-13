# Editor Workflow section — feature index

The `editor-workflow` section of the app (`App.tsx`) is being repurposed away from
its original Reveal/Copy-Path/Send-to-Project trio — Send-to-Project is now
redundant (Projects does it better via drag-and-drop). This section is meant to
grow into several distinct capabilities over time, starting with the ones below.
Each entry stays 2-3 lines. This file is a fast index, not a design doc — if an
idea needs more than 2-3 lines, it needs its own ADR instead.

## Resolve Live Bridge — Planned
One-click send-to-timeline for DaVinci Resolve, built on Resolve's scripting API
(not an Electron Workflow Integration plugin — keeps it working on free Resolve,
not just Studio). No raw drop-to-timeline: each Darkwave project first mirrors
its own folder/bin structure into Resolve's Media Pool automatically and
invisibly, on project creation. Only once that structure exists — and the
user/client has approved it — does one-click "send to timeline" appear.

## Premiere Bridge — Deferred
Adobe's UXP panel API can't reliably insert clips onto a timeline yet (beta,
reported crashes as of 2026); the older CEP framework works but is being phased
out. Revisit once UXP matures, or reconsider CEP sooner if it becomes worth it.

## Track prep — trim & normalize — Planned
Auto-trim silence and loudness-normalize to a target LUFS before sending, using
the audio-analysis pipeline that already exists. Pure DSP, no ML, no new
dependency.

## Track prep — EQ presets — Planned
A handful of hand-tuned presets ("Dialogue clarity," "Impact boost") applied
before export, same DSP layer as trim/normalize.

## Voice isolation / stem separation — Planned
Fork Demucs (Meta, MIT license) for voice/stem isolation, run as an isolated
subprocess — same pattern the GPL similarity-worker already uses. The biggest
build in this list; a real ML dependency, not just DSP.

## Auto-mixing — Not recommended yet
No clean open-source winner exists the way Demucs is for separation. Revisit
after trim/EQ/isolation ship and prove out.

## Background Activity integration — Planned
Every long-running action here (structure sync, track prep, stem separation,
send-to-timeline) shows up as its own row in the existing Background Activity
panel — same progress-bar/pause pattern as audio analysis, waveform, and
instrument detection use today.

## UI — Planned
Full redesign of the panel, consistent with the app's existing visual language
(motion, accent color, glass panels) — not a bolt-on. Replaces the current
Reveal/Copy/Send layout entirely.
