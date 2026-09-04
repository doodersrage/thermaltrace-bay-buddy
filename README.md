# Bay Buddy

Desktop companion for [ThermalTrace](https://thermaltrace.dev) — glanceable freeze & flood moods for one garage, workshop, or cabin space.

**Not a second dashboard.** Devices, alerts, history, and claims stay on thermaltrace.dev. Bay Buddy shows the vibe: cozy, drafty, shiver, panic, offline, or hero.

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

Demo mode cycles moods without a ThermalTrace link. Connect opens thermaltrace.dev (API hookup comes next).

## Build locally

```bash
npm run tauri:build
```

Outputs land under `src-tauri/target/release/bundle/` (AppImage / deb / rpm on Linux).

On some Linux hosts AppImage packing needs `APPIMAGE_EXTRACT_AND_RUN=1` (already set in `npm run tauri:build`).

## Release

Push a version tag (`v0.1.0`). GitHub Actions builds Linux, Windows, and macOS and attaches installers to the release.

```bash
git tag v0.1.0
git push origin v0.1.0
```

## License

MIT — companion to the ThermalTrace project.
