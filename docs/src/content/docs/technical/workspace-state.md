---
title: Workspace State
description: Browser selection, keyboard navigation, and drag payload model.
---

The `workspace-state` crate owns interaction state for the listening browser:

- Focused row.
- Ordered visible asset ids.
- Replace selection for arrow navigation.
- Range selection for Shift interactions.
- Additive toggle selection for Command/Control interactions.
- Select all visible results.
- Drag payloads for tag, collection, project, favorite, trash, and external export targets.

Selection returns asset ids in visible result order so bulk tagging, project moves, and external export preserve the order the user acted on.

Rendering uses `viewport` to keep the number of mounted rows bounded while `workspace-state` preserves selection by asset id.
