# Darkwave

Darkwave is the working repository for Resonant, a local-first desktop audio asset library for editors and sound-driven creatives.

The product plan lives in [docs/genesis/audio_library_application_plan.md](docs/genesis/audio_library_application_plan.md).

## Workspace

- `apps/desktop`: Tauri 2 desktop shell with React, TypeScript, and Vite.
- `crates`: Rust core modules for library, storage, search, audio, import, export, and sync concerns.
- `packages/design-tokens`: shared design tokens for the app and documentation.
- `docs`: Astro/Starlight documentation site plus ADRs.
- `db/migrations`: SQLite schema migrations.

## Development

```sh
npm install
npm run check
npm run build
```

Rust-only checks:

```sh
cargo test --workspace
```
