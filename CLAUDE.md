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
