# ADR 0014: Maintenance report boundary

## Status

Accepted.

## Context

The product needs maintenance actions for missing media, source/license review, duplicate review, waveform cache refresh, NAS validation, and general catalog health. These actions must avoid destructive automatic cleanup.

## Decision

Darkwave owns maintenance findings and report summaries in the `maintenance` crate. Findings include missing media, license review, stale waveform cache, and duplicate content. Duplicate findings recommend review instead of automatic deletion.

## Consequences

- Maintenance output is structured and can feed command palette, inspector status, and future repair workflows.
- Destructive cleanup remains outside automatic maintenance.
- Catalog-specific maintenance jobs can map storage state into the shared report model.
