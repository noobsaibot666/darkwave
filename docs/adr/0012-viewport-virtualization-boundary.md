# ADR 0012: Viewport virtualization boundary

## Status

Accepted.

## Context

The MVP requires smooth browser rows with 50,000 indexed assets and bounded memory with 100,000+ assets. The frontend should not render every row or derive spacer math ad hoc in multiple components.

## Decision

Darkwave owns row virtualization math in the `viewport` crate. The crate computes visible start/end rows, top offset, and bottom spacer height from total rows, row height, viewport height, scroll offset, and overscan.

## Consequences

- Browser rendering can stay bounded regardless of catalog size.
- Spacer values remain deterministic and testable.
- React can focus on rendering the returned range rather than owning scroll math.
