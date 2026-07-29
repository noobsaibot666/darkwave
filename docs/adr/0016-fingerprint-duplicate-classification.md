# ADR 0016: Fingerprint duplicate classification

## Status

Accepted.

## Context

Duplicate detection needs more than exact content hashes. The plan distinguishes exact duplicates, equivalent duplicates, and related variants while forbidding automatic deletion.

## Decision

`fingerprint` owns lightweight fingerprint match classification. Exact duplicates still use content hash and file size. Fingerprint Hamming distance classifies likely matches as equivalent duplicates, related variants, or distinct assets.

Review actions are explicit and non-destructive by default: keep both, link as variants, merge metadata, replace lower-quality version, or move duplicate to Trash.

## Consequences

- Similar audio can be reviewed even when encoding or metadata differs.
- The duplicate workflow can present options without silently deleting files.
- Future acoustic fingerprints can replace the current bit representation without changing duplicate review actions.
