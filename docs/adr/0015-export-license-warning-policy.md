# ADR 0015: Export license warning policy

## Status

Accepted.

## Context

Source and license tracking must be first class, but the application should warn rather than block when exporting an asset with missing or uncertain license information.

## Decision

`export-pipeline` owns license export assessment. Active license context is clear. Missing source/license, missing source URL, uncertain license, and expired license states return warnings. Warnings do not block export planning.

## Consequences

- Editorial work is not blocked by incomplete metadata.
- Risk is visible before export.
- Project reports can still preserve the available source/license context.
