# Darkwave — CLAUDE.md

This file currently only documents the release process — nothing else about the project has been
written up here yet. Extend it as other conventions get established.

## Every release updates the self-served store too

The self-served store (`alan-design.com/#/store`) is managed by a separate project, **Store Manager** (`/Users/alan/_localDEV/_creative/_store_manager`), not by anything in this repo. It's a real, running system (TrueNAS, always-on) with its own watch → review → publish pipeline — see its own `CLAUDE.md` and `docs/runbook.md` before touching it.

**Whenever a direct-sale version ships, it must also be published through Store Manager**, or the storefront keeps selling a stale build under the same listing. Nothing syncs this automatically.

1. Run `bash apps/desktop/scripts/deploy_direct_macos.sh` as usual — it builds, signs, notarizes, and staples, same as before. It does **not** copy the DMG into `licensing-server/releases/` anymore; the script's own final output tells you the Store Manager ingest path to use instead.
2. Drop a `manifest.json` (see Store Manager's `docs/manifest-schema.md`) + the DMG into `/Volumes/Gaia/04_DEV/store-manager/ingest/Apps/darkwave/<version>/` — filename must be the fixed `Darkwave.dmg`, not version-suffixed, since `products.js`'s existing `darkwave` entry already points at that literal path and a republish of an already-registered slug leaves that file untouched (no-op), so the manifest's declared filename has to already match it.
3. Within ~30s it appears in Store Manager's dashboard Inbox (`http://192.168.178.146:5180`) as `pending_review`. **A human reviews it — Dry-run first, then Publish. Don't script around this step**; it's a deliberate gate since Publish touches live Stripe records and rewrites `web_three`'s registry files.
4. Confirm the dry-run says "reusing existing price" (Darkwave's existing Stripe product/price, `prod_V05OcMBlCmhAjL` / `price_1U05EpCsCSs3k4X1fr7x7aER`, is already seeded into Store Manager's database) — not creating a new one.
5. Follow the deploy checklist Store Manager prints after Publish. See Store Manager's `docs/runbook.md` § "TrueNAS (production)" for known gotchas (git LFS not on the TrueNAS host PATH, the staging clone's `safe.directory` requirement, the real deploy path being `store-manager/app/` not `store-manager/`, and backend deploys to `web_three` being rsync-from-the-Mac via `deploy.sh`, never git-on-TrueNAS).

Windows builds ship unsigned for now (deliberate, already-made call — see `docs/development/release-readiness.md` if that decision is ever revisited). The same Store Manager manifest can carry a `windows` file entry alongside `macos` in one drop if both platforms are ready together — see `docs/manifest-schema.md`.

## Apple release accounts & IDs

Every past "notarization 401" / "Transporter rejected" incident came from using the wrong Apple ID or a stale password. The facts, once:

- **Developer Apple ID: `alan.creative@icloud.com`** — the account for App Store Connect, notarization, and Transporter. **Not** the machine's personal login (`alanxalves@me.com`). The app-specific password for notarization must be generated at appleid.apple.com *while signed in as `alan.creative@icloud.com`*.
- **Team ID:** `RD7UU4Z3D2` (Nudson Alan Terrinha Alves). **App Store Connect app ID:** `6797313803`.
- **Bundle IDs:** `dev.darkwave.app` (MAS) · `dev.darkwave.app.direct` (direct-sale).
- **Notary keychain profile:** `darkwave-notary`. Apple revokes app-specific passwords periodically → `deploy_direct_macos.sh` fails at `[3/5]` with HTTP 401. Fix: regenerate the password, then `xcrun notarytool store-credentials darkwave-notary --apple-id alan.creative@icloud.com --team-id RD7UU4Z3D2` (omit `--password`, let it prompt), and confirm with `xcrun notarytool history --keychain-profile darkwave-notary` before retrying.
- **Signing certs** (login keychain): Developer ID Application `2FDD1878…` (direct, pinned by SHA-1 in the script) · `3rd Party Mac Developer Application` + `… Installer` (MAS).
- **Every App Store upload needs a fresh build number** — bump `CFBundleVersion` in `apps/desktop/src-tauri/Info.plist` (marketing version comes from `tauri.conf.json`). App Store Connect 409s a re-used `(CFBundleShortVersionString, CFBundleVersion)` pair, and `CFBundleShortVersionString` must exceed the last *approved* version.
- **App Store listing name:** `Darkwave — Sound Library` (plain "Darkwave" is taken — App Store names are globally unique).
