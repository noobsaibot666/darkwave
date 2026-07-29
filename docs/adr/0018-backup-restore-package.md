# ADR 0018: Backup restore package

## Status

Accepted.

## Context

The MVP requires backup and restore. A usable backup must preserve both the local catalog state and the portable media manifest so NAS-backed libraries can be restored without a full re-import.

## Decision

Darkwave owns backup package metadata in the `backup` crate. A restore package includes library id, manifest revision, media root, catalog snapshot path, manifest path, and creation time. Restore validation requires both catalog snapshot and manifest inputs before producing a restore plan.

## Consequences

- Restores can fail early when required files are missing.
- Backup metadata remains independent from a specific storage backend.
- Future UI can present a concrete restore checklist.
