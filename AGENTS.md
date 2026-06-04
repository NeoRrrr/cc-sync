# Agent Notes

This repository is a local desktop GUI for syncing agent docs, skills, and merged Markdown files. It combines a React/Tauri UI with a Python sync engine.

## Project Conventions

- Treat `sync_agents.py` as the canonical sync engine.
- Keep `sync_from_claude.py` as a compatibility wrapper only.
- Do not commit local runtime state:
  - `cc-sync.config.json`
  - `cc-sync.state.json`
  - `.codegraph/`
  - `dist/`
  - `src-tauri/target/`
  - `portable-dist/`
  - `release-assets/`
  - `node_modules/`
  - `__pycache__/`
  - `*.tsbuildinfo`
- Prefer PowerShell-safe commands and paths. This project is normally edited on Windows.

## Directory Notes

- `sync_agents.py`: canonical sync engine.
- `sync_from_claude.py`: legacy compatibility wrapper only.
- `sync_config.py`: default config and config migration logic.
- `cc-sync.config.example.json`: public example config.
- `cc-sync.config.json`: local real config; do not commit.
- `cc-sync.state.json`: local sync state; do not commit.
- `src/`: React UI.
- `src-tauri/`: Tauri desktop shell.
- `build-portable.ps1`: builds the Windows portable directory and zip.
- `build-macos.sh`: builds macOS `.dmg`, `.app.zip`, and checksum assets.

## Sync Model

- Current config shape is v3:
  - `sources.md_files`
  - `sources.skills_dirs`
  - `sources.docs_dirs`
  - `targets`
- Markdown sources are merged in configured order.
- Skill sources are direct child folders that contain a valid `SKILL.md`.
- Docs sources are direct child folders/files under each configured docs root.
- Skills and docs are deduped by folder/item name. If names collide, keep the first source and warn.
- Safety checks must continue to block:
  - source path equals target path
  - target path inside source path
  - source path inside target path

## UI Expectations

- This is a tool UI, not a landing page. Keep it dense, predictable, and operational.
- Input source rows should use consistent button placement and sizing.
- Skill display should prefer metadata from `SKILL.md`: display name first, folder name fallback, with a short description when present.
- Keep Chinese and English strings in `src/i18n.ts` in sync when changing visible UI text.

## Verification

Before committing behavior changes, run the relevant checks:

```powershell
python -m py_compile sync_config.py sync_agents.py sync_from_claude.py
npm run build
cd src-tauri
cargo check
```

When a local config exists, also run a dry-run sync check:

```powershell
python .\sync_agents.py --config .\cc-sync.config.json --scope all --dry-run --json
```

## Codegraph

The local `.codegraph/` index is useful for code navigation but must stay ignored. Rebuild it after broad structural changes:

```powershell
codegraph index
```
