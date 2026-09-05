# Bay Buddy

Desktop companion for [ThermalTrace](https://thermaltrace.dev) — glanceable freeze & flood moods for one garage, workshop, or cabin space.

**Not a second dashboard.** Devices, alerts, history, and claims stay on thermaltrace.dev. Bay Buddy shows the vibe: cozy, drafty, shiver, panic, offline, or hero.

## Connect flow

1. Click **Connect with Google / GitHub / email**
2. Sign in on thermaltrace.dev in your browser
3. The browser returns to a localhost handoff Bay Buddy is listening on
4. Live probe / freeze margin / time-to-freeze / door / leak moods replace demo data

Requires ThermalTrace server support for `/api/auth/companion/start` (deployed with the companion auth changes).

## Platforms

| OS | Artifacts |
| --- | --- |
| Linux | `.AppImage`, `.deb`, `.rpm` |
| Windows | `.msi`, NSIS installer |
| macOS | `.dmg`, `.app` |

Built with [Tauri 2](https://tauri.app/) + Vite + TypeScript.

## Develop

Requirements: Node 20+, Rust stable, and platform WebView deps ([Tauri prerequisites](https://tauri.app/start/prerequisites/)).

```bash
npm install
npm run tauri:dev
```

## Build locally

```bash
npm run tauri:build
```

Outputs land under `src-tauri/target/release/bundle/`.

On some Linux hosts AppImage packing needs `APPIMAGE_EXTRACT_AND_RUN=1` / `NO_STRIP=true` (already set in `npm run tauri:build`).

### Linux / NVIDIA note

On NVIDIA + Wayland, WebKit can abort with `Could not create GBM EGL display`. Bay Buddy sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` at startup so the AppImage should just work.

## Release

Push a version tag (`v0.2.0`). GitHub Actions builds Linux, Windows, and macOS and attaches installers to the release.

```bash
git tag v0.2.0
git push origin v0.2.0
```

## License

MIT — companion to the ThermalTrace project.
