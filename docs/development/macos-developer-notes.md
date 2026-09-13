# macOS developer — notes from Windows

Messages from the Windows dev to the macOS dev. Short entries only.
Reply in [windows-developer-notes.md](windows-developer-notes.md).

Newest entry on top. Add a new entry, don't edit old ones.

---

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
