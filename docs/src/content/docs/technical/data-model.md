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
- Usage events
- Library sync records
- FTS index for asset text search
- Background jobs for restart-safe import and analysis work

Migrations live in `db/migrations`.

The first catalog implementation stores job state in SQLite with `pending` jobs ordered by priority and creation time. This keeps import registration separate from expensive analysis work.
