# RBX Tools

A small Windows desktop utility with a dark, red GBA-inspired interface. Built with Tauri 2, React, TypeScript, Vite, Tailwind CSS, and Rust.

## Features

The tool library opens **Steamy Friends** for Steam controls. **Settings** contains the automatic Spacewar privacy action:

- **Restart Steam:** ask Steam to shut down, wait up to eight seconds, then relaunch. If it stays open, ask before forcing it closed. If Steam is already closed, start it. Steam launches through the existing Windows Explorer shell, using Microsoft's documented unelevated-launch pattern instead of inheriting RBX Tools' administrator privileges.
- **Stop / Restore network:** temporarily block only the detected `steam.exe`. Both inbound and outbound IPv4/IPv6 internet traffic are covered; loopback connections remain available. After restoration, a game-free observer checks actual Steam login. If offline, a short-lived Spacewar helper sends user-statistics and, if needed, lobby-list requests without restarting Steam. This runs after short and long pauses, with no fixed three-second delay. Failed or timed-out requests are shown as errors; restored network access alone never means online.
- **Make Spacewar private:** automatically reads Steam's account privacy list, marks only AppID 480 private if needed, and reads the list again before confirming success. A separate Steam web sign-in window may appear the first time; complete sign-in and Steam Guard there using the account currently open in the Steam client. The action does not launch Spacewar. Valve's [Private Games documentation](https://help.steampowered.com/en/faqs/view/1150-C06F-4D62-4966) says this hides the game's in-game status from friends. Steam must be running and network access restored. The setting persists on your Steam account for future launches; run the button again to verify it after switching accounts or changing privacy in Steam.

The privacy action uses Valve's authenticated `AccountPrivateApps` web service, as used by Steam's own interface. It requires no developer API key. Steam authentication stays in a dedicated WebView2 profile; the temporary access token stays inside Steam's web origin and is not sent to the app's native bridge, logged, or saved in app preferences. The Steam window has no application IPC capabilities, navigation is restricted to Steam's own HTTPS hosts, and wrong-account sign-ins are rejected. This service is not a documented Steamworks SDK contract; if Valve changes it, the action reports an error instead of claiming that privacy was verified.

The application requests administrator access on launch and automatically finds the exact path of a running `steam.exe` before trying its registry entries and standard install folders. It also allows manually locating `steam.exe`. A single-instance guard prevents conflicting application sessions.

### UI test view

Press **Ctrl+Shift+T** in the desktop app to enter or exit the UI test view. For a browser preview, run `scripts/build.ps1 dev` and open `http://127.0.0.1:1420/?ui-test=1`. The test controls select Tool Library or Steamy Friends, online/offline/connecting/blocked/stopped/missing Steam states, every toast (including stacked error and update), and Settings, startup update, or force-restart modals. Busy and update-error states are also available. **Hide controls** clears the controls from the preview while keeping a small toggle visible.

Buttons simulate their actions in this view. No Steam, traffic, account privacy, file picker, GitHub download, or installation commands run. Toasts stay visible until dismissed. **Exit test view** restores normal operation. The activation instructions are also saved in `AGENTS.md` for future UI work.

### GitHub updates

The desktop app checks the latest stable release in `Coldobird/RbxTools` on startup and every six hours. An available update at startup opens a dialog with **Update** and **Cancel**; six-hour checks show a toast with an **Update** button instead. Update toasts stay visible until dismissed or another action starts. Cancel keeps the update available in Settings, which also provides **Check for updates** and **Update and restart**. The sidebar button always says **Settings**. Checks use GitHub's public release API without a token or OneDrive access. Connection and rate-limit errors appear in Settings.

An update downloads only `RBX-Tools.exe`, verifies GitHub's SHA-256 digest, size, and Windows x64 executable header, then stages a helper beside the running app. The helper waits for the original process to exit, replaces that same executable (including renamed portable copies), and restarts it. The prior executable is restored if replacement or confirmed startup fails. Preferences stay in the existing per-user configuration folder. The portable build does not install WebView2; it uses the runtime already required by the running app.

GitHub downloads that only contain the old OneDrive updater need one manual download of a release containing this GitHub updater. Subsequent updates can use the in-app button. Existing users with the shared OneDrive folder can also transition through the locally published installer. GitHub releases must include an uploaded `RBX-Tools.exe` with its SHA-256 digest; drafts, prereleases, older versions, and incomplete uploads are not installed.

### Restore on exit

Network blocking uses Windows Filtering Platform (WFP) with a **dynamic session**. All four filters and their sublayer are owned by that session. Normal exit closes the session; Windows also removes the session's objects after process termination, including a crash or forced close. No persistent Windows Firewall rules are created. Existing firewall policy is preserved.

"Restore network" means remove RBX Tools' block; it cannot override a block created by another application. Cleanup after abnormal termination follows Windows' session rundown and is not a promise of zero latency. If Steam is offline after restoration, reconnect assistance opens a short Steamworks session using Valve's test AppID 480 (Spacewar), sends a stats request and, if needed, a lobby request, then shuts the session down. It runs in a separate helper with a twelve-second deadline and cancels when Steam restarts, the path changes, traffic is blocked again, or the app closes. Spacewar may briefly appear as the running game while that session is active, and friends' displays may take longer to clear. Normal status checks never initialize a game.

Steam restart is a deliberate, one-time action, not something that can be undone on app exit. The application remembers its Steam path preference in its per-user configuration folder. Steam's separate privacy browser profile retains its own sign-in cookies; the app never saves a local private-status flag as proof of the account setting.

## Development

Requirements: Windows 10/11 x64, Node.js 20+, Rust stable with the MSVC target, Visual Studio C++ Build Tools and Windows SDK, and WebView2.

```powershell
npm ci
npm run desktop
```

Run the development terminal as administrator for `npm run desktop`: the debug application uses the same administrator manifest as the release. Building and browser previews do not need elevation. Double-clicking the finished release executable requests elevation through Windows normally.

### Fifteen-minute Steam reconnect test

Build the diagnostic example, then run it from an elevated PowerShell terminal with Steam open. It blocks only `steam.exe` for fifteen minutes using a dynamic WFP session, restores access, sends stats and lobby requests without a game AppID as a comparison to the app, and records the timing and online state. Closing the test process removes the block.

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --example steam_reconnect_offline_smoke
& .\src-tauri\target\debug\examples\steam_reconnect_offline_smoke.exe "C:\Program Files (x86)\Steam\steam.exe" "$env:TEMP\rbx-steam-reconnect-15min.txt"
```

To try the seven experimental calls without registering a playing game, build both examples and pass `all`. These examples use a separate no-AppID probe. The final argument is a Workshop content filter AppID (960090 is Bloons TD 6), not a SteamAPI game context. Each request runs in a separate helper process; the suite records its result, login state, and any immediate-reconnect log reason. It stops when Steam logs in.

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --example steam_reconnect_candidate --example steam_reconnect_offline_smoke
& .\src-tauri\target\debug\examples\steam_reconnect_offline_smoke.exe "C:\Program Files (x86)\Steam\steam.exe" "$env:TEMP\rbx-steam-all-15min.txt" all 900 960090
```

`all-blocked 300 960090` sends every candidate while Steam is confirmed offline but still blocked, then restores traffic. This distinguishes accepted requests from calls that actually schedule an immediate reconnect. Both modes remove the dynamic network block on exit.

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

GitHub measurements with identical application source reduced the complete job from 10m55s to 6m38s (39.2%) and the Rust cache from 1,379 MiB to 390 MiB. See [the measured build comparison](docs/build-performance.md) for step timings and run links. Manual workflow runs can select `benchmark_only` to build and test without publishing a release.

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
cargo test --release --manifest-path src-tauri/Cargo.toml --lib github_update::tests::live_github_download_verifies_without_installing -- --ignored --nocapture
```

To check the actual WebView2 sign-in handoff without entering credentials or changing game privacy:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol --example spacewar_privacy_smoke
```

This uses a fresh test browser profile, waits for Steam's sign-in page, and closes it. The unit tests exercise the fixed AppID request, account matching, idempotence, and privacy readback failure. A successful authenticated account change still requires a live Steam sign-in.
Add `-- --callback` to exercise the native result bridge with a simulated service failure, without requesting or changing account privacy.

The automated tests check the browser/desktop boundary and Steam path handling. Native network behavior requires an elevated runtime check; compilation alone does not verify packet filtering. Do not test by interrupting a user's active game or Steam session.

For an explicitly requested live restart and network cycle, build the repeatable smoke test and its AppID-free observer, then run the executable from an administrator terminal:

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol --example steam_controls_smoke --example steam_native_reconnect_probe
& .\src-tauri\target\debug\examples\steam_controls_smoke.exe "C:\Program Files (x86)\Steam\steam.exe" "$env:TEMP\rbx-steam-controls.jsonl" 90
```

This calls the production graceful-restart and WFP implementations. It checks that Steam gets a new PID, becomes online, has four owned filters while blocked, reports offline, has no owned filters after restore, and returns online without a second restart. The block ends after observing offline (at least ten seconds) or the requested maximum, limited to two minutes. It never force-closes Steam or initializes a game. JSONL evidence includes timings and cleanup; failures also record the remaining owned-filter count. The companion `.observer.jsonl` records independent native login samples. This exercises the native implementations; it does not automate the desktop buttons or cover the five-minute reconnect-assistance branch.

## Source map

- `src/App.tsx`: dashboard, tool controls, settings, field guide, activity and feedback.
- `src/styles.css`: responsive dark palette, pixel illustration styling, focus and motion states.
- `src/api.ts`: narrow typed desktop bridge and read-only browser preview.
- `src-tauri/src/steam.rs`: detection, exact-path process matching, restart, normal-user launch.
- `src-tauri/src/spacewar_privacy.rs` and `.js`: isolated Steam sign-in, automatic account privacy request, and verification.
- `src-tauri/src/network.rs`: transactional, session-owned WFP block and cleanup.
- `src-tauri/src/lib.rs`: app state, settings, commands, single-instance and shutdown handling.
- `src-tauri/app.manifest`: administrator request at application launch.
- `scripts/build.ps1`: safe staging build for OneDrive/Documents folders protected by Windows.

The landscape is stored as lossless WebP, preserving the original pixels. Dashboard artwork and fonts are bundled locally. The app has no telemetry; the optional privacy action loads Steam's official sign-in and account pages in a separate window.
