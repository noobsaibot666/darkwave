---
title: Data Model
description: Initial catalog entities.
---

The initial SQLite migration defines:

- Libraries
- Assets
- Tags
- Asset tags with suggestion origin and approval state
- Source records
- Collections and projects
- Collection membership
- Usage events
- Library sync records
- FTS index for asset text search
- Background jobs for restart-safe import and analysis work
- Undo actions for recoverable metadata relationship changes
- Smart collection query definitions
- Suggested tag approval state and origin
- Portable manifest asset snapshots
- Writer lease state for shared-library coordination
- Usage events for exported, dragged, copied, played, and used assets
- Source records for project license reports, including attribution, restrictions, and receipt paths

Migrations live in `db/migrations`.

The first catalog implementation stores job state in SQLite with `pending` jobs ordered by priority and creation time. This keeps import registration separate from expensive analysis work.

Project source reports join usage events with source records so exported assets keep their provider, URL, license status, attribution text, usage restrictions, and stored receipt location together.
