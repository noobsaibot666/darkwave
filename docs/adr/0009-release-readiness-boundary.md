# ADR 0009: Release readiness boundary

## Status

Accepted.

## Context

Milestone 7 includes platform polish, accessibility, crash recovery, onboarding, updates, signing, notarization, and documentation. Some of these are code-level behaviors and some are release operations that cannot be truthfully completed by static source changes alone.

## Decision

Darkwave tracks release readiness as an explicit gate model in `release-readiness`. Code-owned gates include accessibility preferences, recovery prompts, performance profiling status, onboarding/documentation coverage, validation for configured update channel metadata, validation for signing identity metadata, and validation that packaged codec coverage and license review metadata exist. Distribution-owned gates still include actual update channel provisioning, codec bundle shipping, and signing/notarization until certificates, decoder artifacts, and platform release credentials are configured.

## Consequences

- Release blockers are visible instead of implied.
- The desktop shell can expose planned distribution work without presenting it as complete.
- Manual platform audits remain required before a release candidate.
