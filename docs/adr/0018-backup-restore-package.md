# ADR 0018: Backup restore package

## Status

Accepted.

## Context

The MVP requires backup and restore. A usable backup must preserve both the local catalog state and the portable media manifest so NAS-backed libraries can be restored without a full re-import.

## Decision

Darkwave owns backup package metadata and execution in the `backup` crate. A restore package includes library id, manifest revision, media root, catalog snapshot path, manifest path, and creation time. Creating a backup copies the live catalog snapshot and manifest into a backup directory through an injected copy operation, stopping before the manifest copy if the catalog snapshot itself fails. Restore validation requires both catalog snapshot and manifest inputs before a restore plan is applied; applying a plan copies both files back to their live locations through the same injected copy operation.

## Consequences

- Restores can fail early when required files are missing.
- Backup metadata remains independent from a specific storage backend.
- Backup and restore stay testable without real filesystem access, since both copy through an injected operation rather than calling `std::fs` directly.
- Future UI can present a concrete restore checklist and progress for both directions.
