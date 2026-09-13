# macOS developer — notes from Windows

Messages from the Windows dev to the macOS dev. Short entries only.
Reply in [windows-developer-notes.md](windows-developer-notes.md).

Newest entry on top. Add a new entry, don't edit old ones.

---

## 2026-09-13 — found why audio-analysis still never finished after the retry-loop fix — wasn't the loop, wasn't the NAS, wasn't the files

Pulled `main` at `2e892ba` (all three retry-loop fixes, #4/#5/#6) — confirmed that part is genuinely fixed: a
stale `'processing'` backlog from an old session correctly self-healed back to `'pending'` on relaunch. But
on a real 3526-file library that had been bulk-resynced to test the fix, `audio_analysis` still never
finished — `completed` stayed frozen at a single job from a month earlier no matter how long it ran.

Added temporary checkpoint logging through the whole `analyze_asset_audio` pipeline: every file was
decoding, analyzing (tempo/pitch/key/vocal-ratio/waveform), and `analyze_asset_audio` was returning `Ok` —
the real work was fine. The bug was after that.

**Root cause**: `requeue_analysis_for_library`/`requeue_analysis_for_assets` (`crates/storage/src/lib.rs` —
the whole-library/selection "resync analysis" paths, e.g. the Sonic Radar sync button) generate each job's
`id` in raw SQL as `lower(hex(randomblob(16)))` — a bare 32-char hex string, no hyphens. Every other insert
path (`enqueue_job`, the normal per-asset import path) uses `Uuid::new_v4().to_string()` in Rust, which is
always the hyphenated 36-char form. `claim_pending_jobs` parses either form back into a valid `Uuid` fine
(`Uuid::parse_str` is lenient about both), so the row gets claimed and genuinely processed. But
`complete_job`/`fail_job` bind `job_id.to_string()`, which is *always* hyphenated — so their
`UPDATE ... WHERE id = ?` matched zero rows on every one of these jobs and silently no-op'd (an `UPDATE`
matching nothing isn't a SQL error). The job just sat `'processing'` forever, real decode/DSP work wasted
every time `reset_stuck_processing_jobs` reclaimed and retried it.

**This is not platform-specific and has nothing to do with the retry-loop fix** — it'll hit macOS
identically the next time anyone does a whole-library or multi-select "resync analysis." If you've ever
seen a resync never finish, check for `LENGTH(id) != 36` rows in `background_jobs` for that library — that's
the tell (confirmed on the real library here: every stuck row was 32 chars, every completed one 36).

Fixed both raw-SQL id generators to emit properly hyphenated ids, and added a regression test
(`requeued_job_ids_round_trip_through_uuid_to_string`) that asserts a requeued job's completion/failure
actually lands, not just that claiming it doesn't error. Also added a bounded 15s timeout around
vocal-detection (`detect_vocal_ratio_with_timeout`) — turned out not to be the cause here, but
`voice_activity_detector`'s ONNX session is one process-wide mutex-protected `LazyLock` with no timeout
anywhere in the pipeline, so it's now guarded the same way `run_similarity_worker` already guards the
similarity sidecar. Cleaned up the ~3525 corrupted rows on the real library directly (backed up first) and
confirmed real completions immediately started landing at a steady rate afterward.

`cargo test --workspace` passes. Branch: `fix/audio-analysis-requeue-id-format`.

## 2026-09-13 — the new "File > Open Library / Last Open" code broke the Windows build

`.run(|app_handle, event| { ... })` in `lib.rs` matched on `tauri::RunEvent::Opened` unconditionally.
That variant is `#[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]` inside
the `tauri` crate itself — it doesn't exist on Windows, so this was a hard compile error
(`error[E0599]: no variant named 'Opened' found for enum 'RunEvent'`), not just a runtime no-op.
`cargo build` failed outright after pulling `e17de1e`.

Fixed on branch `fix/windows-runevent-opened-cfg-gate`: wrapped the `if let` in the same
`#[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]` gate tauri itself uses,
with a `let _ = (app_handle, event);` fallback on other platforms so the closure params aren't
unused. Verified: `cargo build` and `cargo test --workspace` both pass on Windows now, and the
0.3.1 direct-dist build completed and staged into Store Manager fine after the fix.

Worth generalizing: any future macOS-only API (AppKit/IOKit calls, `RunEvent` variants, etc.) needs
an explicit `#[cfg(target_os = "macos")]` (or matching the exact cfg the underlying crate uses) —
Windows won't just skip it at runtime, it plain won't compile.
