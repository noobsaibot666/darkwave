# ADR 0007: NAS resilience core

## Status

Accepted

## Context

Milestone 5 requires shared media locations, local cache behavior, portable manifests, single-writer leases, offline behavior, reconnect validation, and missing/moved asset relinking.

## Decision

Implement the first resilience core across `library-sync` and `storage`:

- `library-sync` owns portable manifest snapshots and writer lease state.
- Writer leases make active conflicting writers read-only until the lease expires.
- Writer leases persist to a JSON lease file beside the shared media so any device opening the library can see who currently holds it. Acquiring renews the file when the caller is allowed to write; releasing only removes a lease the caller still owns.
- `storage` owns local availability validation and relinking.
- Missing originals update asset availability without removing catalog records or search index data.
- Relinking updates the referenced path and restores local availability.
- Reconnect validation jobs can produce reports that count checked manifest paths and identify missing files after a media root comes back online.

## Consequences

- The catalog remains searchable when shared media is offline.
- Shared-library coordination has a tested lease primitive, now backed by file-based lease I/O for acquire, renew, and release.
- Portable manifests can round-trip stable asset identifiers and relative media paths in memory and through manifest files.
- Preview cache eviction plans least-recently-used candidates and can execute their removal, since previews are regenerable derived data rather than original media.
- Shipped arbitrary-format decoder artifacts remain future Milestone 5 work.
