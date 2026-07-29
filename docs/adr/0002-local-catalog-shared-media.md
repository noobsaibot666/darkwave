# ADR 0002: Local catalog with shared media support

## Status

Accepted

## Context

The library may store media on local disks, external disks, or NAS shares. The product plan explicitly warns against placing the live SQLite database on a network filesystem.

## Decision

Keep the active catalog, search index, waveform cache, preview cache, preferences, and session state on the local device. Store original media and portable library manifests in the chosen library media location.

## Consequences

- Searching and browsing can continue while NAS media is offline.
- The application must track availability state for each asset.
- Shared-library behavior needs a portable manifest and single-writer lease.
- Reconnection validates existing records instead of forcing a full rescan.
