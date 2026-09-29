# ADR 0035: One send, one project — resolving the multi-project export target

## Status

Accepted.

## Context

A track can belong to any number of projects. Projects are `collections`
rows with `type = 'project'`, and membership lives in `collection_assets`,
which has no uniqueness constraint per asset — by design. The same music
cue genuinely does get used across a trailer, a director's cut, and a
social edit, and Darkwave is supposed to let one library serve all three.

That makes "send this track to its project folder" a question with no
single answer, and the per-row send button answered it by doing all of
them. `handleSendAssetToItsProjects` fanned out:

```ts
const targets = inCurrentProject ? [inCurrentProject] : memberships.filter(hasAnyFolder);
Promise.all(targets.map((target) => invoke("export_asset_to_project", ...)))
```

Browsing a specific project picked that project; anything else — All
Sounds, Favorites, a tag view, a search result, which is where most
sending actually happens — copied the file into *every* project's folder
at once. One click, three copies, into three editors' live working
folders.

Two things were wrong with this beyond the wasted copies:

1. **It writes into folders the user isn't working in.** These are watch
   folders for Resolve and Premiere. Dropping an unexpected file into a
   project someone else is cutting is not a recoverable "just delete it"
   — it may already be on a timeline by the time anyone notices.
2. **It destroyed the meaning of the per-project sent state.** The data
   model was always right: `usage_events` carries a `project_id`, so the
   catalog knows precisely which project a file was sent to, and
   `project_memberships_for_library` already returns `exported` per
   membership. But the UI collapsed it with
   `memberships.some((m) => m.exported)` — so sending to one project lit
   the badge green in all of them. The information was there; the
   interface threw it away.

## Decision

**Every send names exactly one project, and when the app can't work out
which one, it asks instead of guessing.**

Three parts:

### 1. A single funnel

`sendAssetsToProject(assetIds, project)` is now the only caller of
`export_asset_to_project` in the frontend. It takes a project — never a
list — so there is one place where a destination is chosen and it is
structurally incapable of fanning out. The per-row button, the sidebar
bulk button, the picker, and the project-view bulk actions all route
through it.

### 2. Explicit precedence, then ask

`resolveSendTarget(memberships)` resolves a target by precedence, most
specific first:

1. **The project being browsed** — you're in its view, a send means here.
2. **The active project** — a standing "what I'm cutting today" target,
   set from the crosshair on any project row in the sidebar.
3. **The only candidate**, when exactly one of the track's projects has
   an export folder configured.

Anything else returns `choose`, and the button opens a picker listing the
candidate projects with each one's own sent/not-sent state. The fan-out
survives *only* as an explicitly labelled "Send to all N projects" item
in that picker: it's a real thing to want occasionally, and the bug was
never that it existed — it was that it was the silent default.

The active project is stored in `AppPreferences::active_project_by_library`
(keyed by library id), not in the `.darkwave` file. It's per-user working
state, not library data that should travel to another machine. A stale id
— a project from another library, or one since removed — resolves to
`null` rather than lingering, because `activeProject` looks the id up in
the live `collections` list rather than trusting it.

### 3. Per-project state, shown per project

The row badge now reflects the *resolved target's* `exported` flag, not
`some()`. It has three states rather than two: pending, exported, and a
third "choose" state for the ambiguous case — which deliberately claims
neither, because until a project is named there is no sent-or-unsent fact
to report.

Inside a project view, where "this project" is unambiguous, the selection
bar gains the management actions that were previously impossible to offer
safely: **Send unsent (N)**, **Re-send all**, and **Remove from project**
(membership only, undoable, leaving both the library asset and any
already-exported file on disk alone — `remove_assets_from_collection`).

## Consequences

- A send from a non-project view now targets one project or asks. Users
  who were relying on the old fan-out get it as one extra, labelled click.
- Setting an active project is the one-time setup that makes row buttons
  unambiguous for a whole session; with none set, a track in several
  projects always asks.
- `usage_events`' per-project export history is now actually surfaced.
  Nothing about the data model changed — this ADR is mostly the UI
  catching up to what the catalog already recorded.
- `remove_assets_from_collection` adds a `readd_collection_assets` undo
  kind (and its redo counterpart), mirroring `readd_asset_tags`.
- Deleting a project should call
  `AppPreferences::forget_active_project` so a dangling id can't keep
  claiming to be an export target. No project-deletion command exists
  yet; the hook is in place for when one lands.
