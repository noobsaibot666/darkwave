# Windows — next steps

Staging folder for Windows store assets (icons, screenshots) before they move
to their final place. Direct-sale only — no Microsoft Store submission.

Do this on the Windows machine, after `git pull`.

## Build

- [ ] `git pull` the latest code
- [ ] `npx tauri build --features direct-dist --config src-tauri/tauri.direct.conf.json`
- [ ] Confirm the NSIS installer is produced under `target/release/bundle/nsis/`
- [ ] No code signing — ships unsigned by decision, expect a SmartScreen warning

## License / update wiring

- [ ] Sign the installer with the updater key: `npx tauri signer sign -f secrets/darkwave-updater.key -p "" <installer path>`
- [ ] Add a `windows-x86_64` entry to `web_three/licensing-server/releases-meta/darkwave.json` (filename + signature)
- [ ] Copy the installer to the licensing server's release mount as `actual/Darkwave.exe` (currently missing — a Windows purchase would 404 today)

## Testing

- [ ] Install on a real Windows machine, confirm first-run trial starts
- [ ] Confirm license key activation works (HWID check)
- [ ] Confirm self-update check works against the live manifest endpoint
- [ ] Log any issues in `bug-log.md` per the existing pattern

## Store assets

- [ ] Icons/screenshots here if/when needed for a future Windows-specific listing (none required right now — the website storefront already uses the same Darkwave assets cross-platform)
