# ADR 0014: Maintenance report boundary

## Status

Accepted.

## Context

The product needs maintenance actions for missing media, source/license review, duplicate review, waveform cache refresh, NAS validation, and general catalog health. These actions must avoid destructive automatic cleanup.

## Decision

Darkwave owns maintenance findings and report summaries in the `maintenance` crate. Findings include missing media, license review, stale waveform cache, and duplicate content. Duplicate findings recommend review instead of automatic deletion.

The same boundary owns preview cache eviction planning and execution. Cache entries are sorted by least-recently-used metadata, then returned as deletion candidates until the configured cache limit can be restored. Execution removes each candidate through an injected remove operation and reports which paths were removed, which failed, and the bytes actually freed; one failed removal does not block the rest.

## Consequences

- Maintenance output is structured and can feed command palette, inspector status, and future repair workflows.
- Destructive cleanup of user-owned or original assets remains outside automatic maintenance and stays in Trash/review flows.
- Preview cache eviction can run automatically without Trash or user confirmation because cached previews are regenerable derived data, not original media.
- Catalog-specific maintenance jobs can map storage state into the shared report model.
