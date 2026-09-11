# Re-publishing a Windows build through Store Manager

Applies when the **macOS** build of a version was already published and you now
ship (or re-ship) the **Windows** build for the *same* version number.

The `deploy_direct_windows.ps1` handoff drops `Darkwave.exe` + a merged
`manifest.json` into the Store Manager ingest. Store Manager's watcher upserts
the new file + checksum into the existing `product_versions` row **but never
changes its `status`** — so an already-`published` version stays hidden from the
dashboard Inbox and `POST /publish` refuses it with
`Version darkwave@<ver> is already published`.

Fix: flip the row back to `pending_review`, then Dry-run + Publish.

## On the Windows machine

```powershell
cd apps\desktop
powershell -File scripts\deploy_direct_windows.ps1
```

Note the SHA-256 the script prints for `Darkwave_<ver>_x64-setup.exe` (also in
`apps\desktop\builds\direct_distribution\windows\`). That is the value the DB row
and the dashboard must show for `Darkwave.exe`.

## On the TrueNAS host (Store Manager)

`product_versions.id` for Darkwave 0.2.1 is **380614** (`product_id = 3`). The
host shell does not reliably load `app/.env`, so run `psql` inside the container
and let it read the container's own environment:

```bash
DBX() { docker exec -i store-manager-db sh -c 'psql -U "$POSTGRES_USER" -d "$POSTGRES_DB"' ; }
```

Check the row (confirm `Darkwave.exe` checksum matches the installer SHA):

```bash
echo "select id,version,status,files,checksums from product_versions where id=380614;" | DBX
```

Flip it back to review:

```bash
echo "update product_versions set status='pending_review', published_at=null where id=380614;" | DBX
```

Confirm it is now in the Inbox:

```bash
curl -s http://localhost:4010/inbox
```

Dry-run — must say `reusing existing price price_1U05EpCsCSs3k4X1fr7x7aER`,
`products.js: entry already present — no change`, and list a copy of both
`Darkwave.dmg` and `Darkwave.exe` into `01_releases/actual/`:

```bash
curl -s -X POST http://localhost:4010/publish/380614 -H 'Content-Type: application/json' -d '{"dryRun":true}'
```

Publish (touches live Stripe + rewrites `web_three` registry files):

```bash
curl -s -X POST http://localhost:4010/publish/380614 -H 'Content-Type: application/json' -d '{"dryRun":false,"live":true}'
```

Then run the `deployCommands` the publish response prints (git diff/commit/push
in `web_three_staging`, `deploy.sh --backend-only`, `npm run deploy:fast` from
the Mac, and `ls /mnt/Gaia/04_DEV/01_releases/actual/` to confirm the installer
landed).

### If the watcher restarted and the checksum looks stale

```bash
docker restart store-manager-api && sleep 6 && docker logs --since 1m store-manager-api | grep -i darkwave
```

### Reject instead

```bash
curl -s -X POST http://localhost:4010/reject/380614 -H 'Content-Type: application/json' -d '{"reason":"superseded"}'
```

## Store Manager stack reference

| | |
|---|---|
| Compose project | `store-manager-prod` |
| Compose file | `/mnt/Gaia/04_DEV/store-manager/app/truenas-docker-compose.yml` |
| Containers | `store-manager-api` (watcher + API, port 4010), `store-manager-web` (dashboard, port 5180), `store-manager-db` (postgres 15) |
| Ingest root | `/mnt/Gaia/04_DEV/store-manager/ingest` |
| Ingest path (this app) | `Apps/darkwave/<version>/` — `Darkwave.dmg`, `Darkwave.exe`, `manifest.json` |
| Dashboard | `http://192.168.178.146:5180` |
| Stripe product / price | `prod_V05OcMBlCmhAjL` / `price_1U05EpCsCSs3k4X1fr7x7aER` |

There is no separate watcher container — restarting `store-manager-api` restarts
the watcher. A `docker restart` alone never re-queues an already-published
version; only the `status` flip above does.
