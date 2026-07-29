---
title: Viewport
description: Virtualized browser row range model.
---

The `viewport` crate computes the render window for large browser result sets.

Inputs:

- Total row count.
- Stable row height.
- Viewport height.
- Scroll offset.
- Overscan row count.

Outputs:

- Start row.
- End row, exclusive.
- Top offset spacer.
- Bottom spacer.

The default desktop budget uses 52 px rows, a 520 px viewport, and 6 rows of overscan. For a 50,000-row catalog at the top of the list this renders 16 rows, not the full catalog.
