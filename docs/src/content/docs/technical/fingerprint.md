---
title: Fingerprint
description: Exact and likely duplicate detection model.
---

The `fingerprint` crate provides duplicate detection primitives:

- Exact duplicate key from content hash and file size.
- Audio fingerprint Hamming distance.
- Equivalent duplicate classification for close fingerprints.
- Related variant classification for wider but still nearby matches.
- Distinct classification for distant fingerprints.

Duplicate review actions are non-destructive by default: keep both, link as variants, merge metadata, replace lower-quality version, or move duplicate to Trash.
