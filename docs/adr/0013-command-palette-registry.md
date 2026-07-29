# ADR 0013: Command palette registry

## Status

Accepted.

## Context

The plan requires a command palette that exposes major actions such as import, search, tagging, collection assignment, export, reveal, conversion, rescan, settings, and maintenance. Those actions need a shared registry so shortcuts, menus, and palette search do not drift apart.

## Decision

Darkwave owns palette actions in the `command-palette` crate. The registry stores command ids, titles, categories, and keywords. Search ranks title matches above keyword matches and category matches.

## Consequences

- Palette search can be tested independently from UI rendering.
- Native menus, shortcuts, and command palette entries can share command ids.
- Future command execution can attach handlers to this registry without changing search behavior.
