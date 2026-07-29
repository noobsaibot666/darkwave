# ADR 0017: Application Trash boundary

## Status

Accepted.

## Context

The plan requires every deletion path to go through application Trash first. Duplicate review and cleanup workflows must never automatically delete files.

## Decision

Darkwave owns non-destructive Trash state in the `trash` crate. Moving an asset to Trash records the original path, timestamp, reason, and restore plan while leaving file deletion false. Purge requires both retention age and an explicit request.

## Consequences

- Trash actions remain recoverable.
- Duplicate cleanup can offer “move to Trash” without deleting immediately.
- Future storage integration can persist Trash records and execute purge separately.
