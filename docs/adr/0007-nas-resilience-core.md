# ADR 0007: NAS resilience core

## Status

Accepted

## Context

Milestone 5 requires shared media locations, local cache behavior, portable manifests, single-writer leases, offline behavior, reconnect validation, and missing/moved asset relinking.

## Decision

Implement the first resilience core across `library-sync` and `storage`:

- `library-sync` owns portable manifest snapshots and writer lease state.
- Writer leases make active conflicting writers read-only until the lease expires.
- `storage` owns local availability validation and relinking.
- Missing originals update asset availability without removing catalog records or search index data.
- Relinking updates the referenced path and restores local availability.
- Reconnect validation jobs can produce reports that count checked manifest paths and identify missing files after a media root comes back online.

## Consequences

- The catalog remains searchable when shared media is offline.
- Shared-library coordination has a tested lease primitive before file-based lease I/O exists.
- Portable manifests can round-trip stable asset identifiers and relative media paths in memory and through manifest files.
- Cache eviction and shipped arbitrary-format decoder artifacts remain future Milestone 5 work.
