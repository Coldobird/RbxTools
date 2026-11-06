# RBX Tools technical guide

For download instructions and a plain-language introduction, see the [README](../README.md).

A small Windows desktop utility with a dark, red GBA-inspired interface. Built with Tauri 2, React, TypeScript, Vite, Tailwind CSS, and Rust.

## Features

The tool library opens **Steamy Friends** for Steam controls:

- **Restart Steam:** ask Steam to shut down, wait up to eight seconds, then relaunch. If it stays open, ask before forcing it closed. If Steam is already closed, start it. Steam is launched with `-cef-enable-debugging` so the offline/online controls work. Paused Steam restarts in Offline Mode. Steam launches through the existing Windows Explorer shell, using Microsoft's documented unelevated-launch pattern, so Steam never inherits elevation even if RBX Tools is started as administrator.
- **Stop network / Restore and reconnect:** switch Steam offline and back online through Steam's own client controls, without a restart or administrator rights. See [Steam offline/online switch](steam-offline-switch.md). Restored access alone never means online; the label waits for Steam to confirm login.

The application runs as the current user without an administrator prompt and automatically finds the exact path of a running `steam.exe` before trying its registry entries and standard install folders. It also allows manually locating `steam.exe`. A single-instance guard prevents conflicting application sessions.

### UI test view

Press **Ctrl+Shift+T** in the desktop app to enter or exit the UI test view. For a browser preview, run `scripts/build.ps1 dev` and open `http://127.0.0.1:1420/?ui-test=1`. The test controls select Tool Library or Steamy Friends, online/offline/connecting/blocked/stopped/missing Steam states, every toast (including stacked error and update), and Settings, startup update, or force-restart modals. Busy and update-error states are also available. **Hide controls** clears the controls from the preview while keeping a small toggle visible.

Buttons simulate their actions in this view. No Steam, network, file picker, GitHub download, or installation commands run. Toasts stay visible until dismissed. **Exit test view** restores normal operation. The activation instructions are also saved in `AGENTS.md` for future UI work.

### GitHub updates

The desktop app checks the latest stable release in `Coldobird/RbxTools` on startup and every six hours. An available update at startup opens a dialog with **Update** and **Cancel**; six-hour checks show a toast with an **Update** button instead. Update toasts stay visible until dismissed or another action starts. Cancel keeps the update available in Settings, which also provides **Check for updates** and **Update and restart**. The sidebar button always says **Settings**. Checks use GitHub's public release API without a token or OneDrive access. Connection and rate-limit errors appear in Settings.

An update downloads only `RBX-Tools.exe`, verifies GitHub's SHA-256 digest, size, and Windows x64 executable header, then stages a helper beside the running app. The helper waits for the original process to exit, replaces that same executable (including renamed portable copies), and restarts it. The prior executable is restored if replacement or confirmed startup fails. Preferences stay in the existing per-user configuration folder. The portable build does not install WebView2; it uses the runtime already required by the running app.

GitHub downloads that only contain the old OneDrive updater need one manual download of a release containing this GitHub updater. Subsequent updates can use the in-app button. Existing users with the shared OneDrive folder can also transition through the locally published installer. GitHub releases must include an uploaded `RBX-Tools.exe` with its SHA-256 digest; drafts, prereleases, older versions, and incomplete uploads are not installed.

### Restore on exit

Closing RBX Tools brings a paused Steam back online. Steam restart is a deliberate, one-time action, not something that can be undone on app exit. The application remembers its Steam path preference in its per-user configuration folder.

Steam status checks run in a separate, game-free helper process with a two-second deadline. A timed-out helper is terminated before another check can start. Missing responses retain the previous connection label for 30 seconds before showing **Last confirmed online/offline**; without a previous reading, the label changes from **Connecting…** to **Steam connection unavailable**. Polling pauses while hidden and starts a fresh 30-second grace period on resume. Individual frontend requests have an eight-second deadline; a busy query is retried for up to 30 seconds before reporting a pending error. Restore keeps **Restoring connection…** until Steam confirms online or 30 seconds pass. Blocked, running Steam shows **Steam offline**. Routine successful actions do not show toasts; failures and available-update notices remain visible.

## Development

Requirements: Windows 10/11 x64, Node.js 20+, Rust stable with the MSVC target, Visual Studio C++ Build Tools and Windows SDK, and WebView2.

```powershell
npm ci
npm run desktop
```

`npm run desktop`, building and browser previews do not need elevation; the debug and release applications run as the current user.

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

Repeated builds reuse installed dependencies when the lockfile and Node ABI match, copy only changed source files, and regenerate icons only when the source image, Tauri CLI, or configured icon outputs change. Cargo uses its default CPU parallelism; set `CARGO_BUILD_JOBS` yourself to limit it on machines with less memory. The Windows launcher builds only the Rust library it uses, without companion DLL/static-library outputs.

`npm run build` also checks the frontend sources, assets, configuration, dependencies, environment, and output contents before reusing a successful build. Unchanged output timestamps let Cargo reuse the embedded frontend. Use `npm run build -- --force` to rebuild it explicitly.

CI runs native tests after building the app, using the same release profile and `tauri/custom-protocol` feature. This reuses release dependencies instead of compiling a separate debug dependency set; artifacts and releases are uploaded only after the tests pass. Distribution builds retain size-focused Fat LTO and one codegen unit: tested alternatives increased executable size and did not demonstrate a consistent compile-time benefit.

GitHub measurements with identical application source reduced the complete job from 10m55s to 6m38s (39.2%) and the Rust cache from 1,379 MiB to 390 MiB. See [the measured build comparison](build-performance.md) for step timings and run links. Manual workflow runs can select `benchmark_only` to build and test without publishing a release.

Rust on this machine was installed to `%USERPROFILE%\.cargo\bin` without changing the system PATH. The staging script adds it for its own session.

## Build and share

To build only the portable executable:

```powershell
npm run desktop:build:portable
```

This creates `src-tauri/target/release/rbx-tools.exe` without packaging an installer or publishing to OneDrive. GitHub Actions uses this command and uploads only `RBX-Tools.exe` to its workflow artifact and GitHub Release. New versions still receive an automatic release on their first successful `main` build; ordinary commits upload an artifact without creating another release.

To also build an installer and publish the local build files to OneDrive:

```powershell
npm run desktop:build
```

Tauri generates the application at `src-tauri/target/release/rbx-tools.exe` and the installer under `src-tauri/target/release/bundle/nsis/`. The staging script also collects both into its cache's `release` directory as `RBX-Tools.exe` and `RBX-Tools-Setup.exe`.

The locally built installer handles WebView2 setup if needed. GitHub distributes the portable executable, which requires WebView2 already installed. The desktop app updates from GitHub regardless of whether it was installed or launched as a portable copy.

## Checks

```powershell
npm test
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

The native updater tests cover version and asset selection, corrupt downloads, rollback, and a Windows process test that replaces and restarts a renamed executable. To additionally check the real public GitHub download without installing or launching it:

```powershell
cargo test --release --manifest-path src-tauri/Cargo.toml --lib update::release::tests::live_github_download_verifies_without_installing -- --ignored --nocapture
```

The automated tests check the browser/desktop boundary, Steam path handling and the offline/online controls against a fake Steam. Do not test by interrupting a user's active game or Steam session.

Opt-in live checks against the running Steam client (build with `cargo build --manifest-path src-tauri/Cargo.toml --example <name>`):

- `steam_offline_smoke <steam.exe> [cycles=3] [offline_s=20]`: cycles Steam offline and online through the production controls.
- `steam_connection_smoke <steam.exe> <events.jsonl> <seconds>`: read-only check of the status helper's timeout.
- `steam_ui_ready_check <steam.exe>`: read-only check of the UI-readiness detector used after a restart.

## Source map

Frontend (`src/`). Imports only point down: `ui` → `app` → `platform` and `testview` → `domain`. `main.tsx` is the composition root that builds every long-lived object once.

- `ui/`: everything React. `App` composes the window from `layout/Sidebar`, `pages/` (Tool Library, Steamy Friends), `modals/` (Settings, update, force restart), shared `components/` (pixel icons, status icon, toasts, modal frame) and `testview/` (the UI test view panel and its hooks). Components receive view models or plain values, plus only the controller actions they use.
- `app/`: application logic without React or browser globals. `AppController` owns the state in a `Store` and decides which actions may run; `actions/` carries them out (`LiveActions` on the desktop, `SimulatedActions` in the browser preview and UI test view). `views/` holds the view models (`SteamToolView`, `UpdateView`, `SettingsView`, `ToastView`, on a shared `StateView`). `live/` holds the live session (`StatusMonitor`, `StatusPoller`, `UpdateWatcher`, `StartupUpdate`). `testview/TestViewControls` drives the UI test view.
- `platform/`: the outside world. `Desktop` is the interface to the backend and `DesktopBridge` its Tauri implementation and the only caller of Tauri commands, failing with a typed `DesktopError`. Also `PageVisibility` and `Timings`.
- `testview/`: the UI test view's switch (`TestViewMode`), panel settings, simulated data (`TestViewFixtures`) and `SelectionPresets`.
- `domain/`: plain immutable classes without display text: `SteamStatus` (a reading and its merge rules), `RunningActions` (which actions run and may start) and `ConnectionGrace`.
- `styles/`: palette, layout, typography and pixel-alignment CSS.

UI tests (`tests/ui/`) drive the app through `support/` page objects (`SteamyFriendsPage`, `TestViewPanel`), a `MockDesktop` backend, and an `InkMeter` that measures drawn pixels.

Backend (`src-tauri/src/`). Every operation fails with one `Error` type (`error.rs`): a kind the interface reacts to, plus a message for the user.

- `app/`: the Tauri builder and commands; `AppService` (status, network pause, restart, update) over the `SteamControl` trait, with `LocalSteam` as the real Steam; and `PreferencesStore`.
- `steam/`: `SteamExecutable` (a validated `steam.exe`), `SteamInstallation` (finding Steam), `WindowsPath`, `SteamProcesses`, `SteamUi` (window readiness), `SteamRestarter` with an unelevated Explorer launcher, `OfflineSwitch` over Steam's `DevTools` endpoint, and `LoginChecker` with its isolated `LoginHelper` process. Only the diagnostics examples reach it from outside, through `rbx_tools_lib::diagnostics`.
- `update/`: `UpdateCheck`, `GitHubReleases`, `ExpectedFile` verification, `Installer`, the `StagingFolder` handoff, `UpdateHelper`, and the post-update `complete_startup`.
- `win32.rs`, `busy_flag.rs`, `poll.rs`, `timings.rs`: shared Win32 wrappers, the "already running" guard, waiting with a deadline, and the timing log.
- `src-tauri/app.manifest`: runs as the current user (`asInvoker`), DPI awareness and Common Controls v6.
- `scripts/build.ps1`: safe staging build for OneDrive/Documents folders protected by Windows.

Dashboard artwork and fonts are bundled locally. The app has no telemetry.
