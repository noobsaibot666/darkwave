---
title: Maintenance
description: Catalog health report model.
---

The `maintenance` crate defines non-destructive health findings:

- Missing media.
- License review required.
- Stale waveform cache.
- Duplicate content.

Reports include total findings, counts by kind, severity, detailed findings, and recommended actions.

Preview cache eviction is planned separately from destructive cleanup. The maintenance boundary accepts cache entries with size and last-accessed metadata, then returns least-recently-used candidates until the configured cache limit can be restored.

Duplicate content recommends review, not deletion. File removal must still go through explicit user action and application Trash.

Likely duplicate groups use the `fingerprint` classification model before being surfaced as maintenance findings.
