# ADR 0014: Maintenance report boundary

## Status

Accepted.

## Context

The product needs maintenance actions for missing media, source/license review, duplicate review, waveform cache refresh, NAS validation, and general catalog health. These actions must avoid destructive automatic cleanup.

## Decision

Darkwave owns maintenance findings and report summaries in the `maintenance` crate. Findings include missing media, license review, stale waveform cache, and duplicate content. Duplicate findings recommend review instead of automatic deletion.

The same boundary owns preview cache eviction planning. Cache entries are sorted by least-recently-used metadata, then returned as deletion candidates until the configured cache limit can be restored. The planner does not remove files.

## Consequences

- Maintenance output is structured and can feed command palette, inspector status, and future repair workflows.
- Destructive cleanup remains outside automatic maintenance.
- Preview cache pressure can surface actionable candidates without bypassing Trash or user confirmation.
- Catalog-specific maintenance jobs can map storage state into the shared report model.
