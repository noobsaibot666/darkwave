# ADR 0003: Import core and catalog boundary

## Status

Accepted

## Context

Milestone 1 needs files to appear in the catalog immediately while deeper metadata, hashing, waveform, and analysis work continues in the background.

## Decision

The import pipeline performs only lightweight synchronous work:

- Ignore incomplete browser download files.
- Wait for watched file size stabilization.
- Read immediate filesystem metadata.
- Register a managed or referenced asset in the local SQLite catalog.
- Suppress duplicate asset rows when the same library, content hash, and file size are already known.
- Enqueue persistent background jobs for metadata extraction, hashing, and waveform generation.

The storage crate owns SQLite persistence and job queue records. Import orchestration depends on the storage interface instead of writing SQL directly.

## Consequences

- Imported files can become visible before expensive audio decoding exists.
- Restart can recover pending jobs from SQLite.
- The content hash used in this milestone is intentionally lightweight and will be replaced by the dedicated hashing/fingerprinting pipeline.
- Managed import currently records the intended managed path; physical copy policy will be implemented with recoverable file operations in a later Milestone 1 refinement.
