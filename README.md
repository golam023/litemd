# LiteMD

A Sumatra-PDF-style lightweight, portable Markdown (.md) viewer for Windows.
Single .exe, no installer, no Python/JS runtime — pure Rust + egui.

## Features
- Opens .md / .markdown / .mdown / .mkd / .mkdn / .txt files
- Drag & drop a file onto the window
- Open via CLI arg (`LiteMD.exe file.md`) — enables "Open with" / default-app association
- Recent files list (persisted)
- Live-reload: auto-refreshes when the file changes on disk
- Find in document (Ctrl+F)
- Zoom in/out/reset (Ctrl +/-/0)
- Dark / Light theme toggle
- One-click "Set as default .md app" (writes HKCU registry entries — no admin needed)
- Single portable exe, small binary (release profile stripped + LTO)

## Build locally (Windows, with Rust installed)
```
cargo build --release
```
Exe will be at `target\release\litemd.exe`.

## Build via GitHub Actions (no local Rust needed)
1. Create a new GitHub repo, push this folder to it.
2. GitHub Actions (`.github/workflows/build.yml`) builds automatically on every push to `main`.
3. Download the exe from the workflow run's **Artifacts** section, OR
4. Push a version tag (e.g. `git tag v0.1.0 && git push origin v0.1.0`) to also get it attached to a GitHub Release.

## Set as default .md app
Open a file in LiteMD, then File → "Set LiteMD as default .md app".
This registers LiteMD in Windows' "Open with" list and as a candidate default app
(HKEY_CURRENT_USER only — no elevation required). You may still need to confirm it
once in Windows Settings → Apps → Default apps → search ".md".
