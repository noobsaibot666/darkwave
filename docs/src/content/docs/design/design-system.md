---
title: Design System
description: Initial visual direction and token source.
---

The interface is dark-first, calm, dense, and keyboard-friendly. It uses restrained material effects only where they help hierarchy: sidebar, inspector groups, floating transport, menus, popovers, and temporary overlays.

Shared design tokens live in `packages/design-tokens`.

Release polish requirements:

- Every interactive control needs a visible focus state.
- Reduced motion disables nonessential animation.
- Reduced transparency uses solid surfaces instead of blurred material.
- Compact panels use restrained headings and stable control dimensions.
- Status labels must distinguish passed, planned, and blocked states.
- Browser rows need stable height so virtualization math can keep scrolling smooth.
