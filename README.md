# RBX Tools

A small Windows desktop utility with a dark, red GBA-inspired interface. Built with Tauri 2, React, TypeScript, Vite, Tailwind CSS, and Rust.

## Version 0.2.1

The tool library opens **Steamy Friends**, with two controls:

- **Restart Steam:** ask Steam to shut down, wait up to eight seconds, then relaunch. If it stays open, ask before forcing it closed. If Steam is already closed, start it. Steam launches through the existing Windows Explorer shell, using Microsoft's documented unelevated-launch pattern instead of inheriting RBX Tools' administrator privileges.
- **Stop / Restore network:** temporarily block only the detected `steam.exe`, matching the supplied NetLimiter screenshot. Both inbound and outbound IPv4/IPv6 traffic are covered. Steam helpers and games are not added to the target. After a pause of at least five minutes, restoring waits three seconds and checks Steam's login state through Steamworks. If Steam is still offline, RBX Tools requests the current user's stats, then requests a lobby list only if login has not recovered four seconds later.

The application requests administrator access on launch and automatically finds the exact path of a running `steam.exe` before trying its registry entries and standard install folders. It also allows manually locating `steam.exe`. A single-instance guard prevents conflicting application sessions.

### Restore on exit

Network blocking uses Windows Filtering Platform (WFP) with a **dynamic session**. All four filters and their sublayer are owned by that session. Normal exit closes the session; Windows also removes the session's objects after process termination, including a crash or forced close. No persistent Windows Firewall rules are created. Existing firewall policy is preserved.

"Restore network" means remove RBX Tools' block; it cannot override a block created by another application. Cleanup after abnormal termination follows Windows' session rundown and is not a promise of zero latency. The reconnect assistant uses public Steamworks requests that repeatedly triggered `ScheduleImmediateReconnect()` during five-, fifteen-, and thirty-minute local tests. It never closes Steam or the running game.

Steam restart is a deliberate, one-time action, not something that can be undone on app exit. The application only remembers its Steam path preference in its per-user configuration folder.

## Development

Requirements: Windows 10/11 x64, Node.js 20+, Rust stable with the MSVC target, Visual Studio C++ Build Tools and Windows SDK, and WebView2.

```powershell
npm ci
npm run desktop
```

Run the development terminal as administrator for `npm run desktop`: the debug application uses the same administrator manifest as the release. Building and browser previews do not need elevation. Double-clicking the finished release executable requests elevation through Windows normally.

For a browser-only interface preview:

```powershell
npm run dev
```

Browser preview clearly identifies itself and cannot restart Steam or change networking.

### Protected Documents / OneDrive folders

On this machine Windows Controlled Folder Access prevents build tools from creating files under Documents. Use the included staging script; it copies source into `%LOCALAPPDATA%\RBXTools-build` and builds there without changing Windows security settings:

```powershell
./scripts/build.ps1 -Mode build
./scripts/build.ps1 -Mode check
./scripts/build.ps1 -Mode dev
```

The script prints the real build/output path, including any Windows packaged-app path redirection. `dev` stages a snapshot: edit the working source and re-run the script to stage updates, or use a normal writable development checkout for live hot reload.

Rust on this machine was installed to `%USERPROFILE%\.cargo\bin` without changing the system PATH. The staging script adds it for its own session.

## Build and share

```powershell
npm run desktop:build
```

Tauri generates the application at `src-tauri/target/release/rbx-tools.exe` and the installer under `src-tauri/target/release/bundle/nsis/`. The staging script also collects both into its cache's `release` directory as `RBX-Tools.exe` and `RBX-Tools-Setup.exe`.

Share the installer with friends. It handles WebView2 setup if needed. The standalone executable requires WebView2 already installed. This first private build is unsigned, so its publisher is not verified by Windows.

## Checks

```powershell
npm test
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

The automated tests check the browser/desktop boundary and Steam path handling. Native network behavior requires an elevated runtime check; compilation alone does not verify packet filtering. Do not test by interrupting a user's active game or Steam session.

## Source map

- `src/App.tsx`: dashboard, tool controls, settings, field guide, activity and feedback.
- `src/styles.css`: responsive dark palette, pixel illustration styling, focus and motion states.
- `src/api.ts`: narrow typed desktop bridge and read-only browser preview.
- `src-tauri/src/steam.rs`: detection, exact-path process matching, restart, normal-user launch.
- `src-tauri/src/network.rs`: transactional, session-owned WFP block and cleanup.
- `src-tauri/src/lib.rs`: app state, settings, commands, single-instance and shutdown handling.
- `src-tauri/app.manifest`: administrator request at application launch.
- `scripts/build.ps1`: safe staging build for OneDrive/Documents folders protected by Windows.

The pixel artwork is original SVG. Fonts are bundled locally through Fontsource; the app has no accounts, telemetry, or remote UI assets.
