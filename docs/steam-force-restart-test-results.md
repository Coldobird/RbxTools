# Steam force restart and connection status

Requested on 7 October 2026 (São Paulo).

## Spacewar recovery retest — 8 October 2026

The user rejected the passive run's 36.480-second recovery and explicitly
authorized restoring the Spacewar AppID 480 call while keeping Steam running.
Recovery now runs immediately after an observed offline restore, without a
five-minute threshold or three-second delay. The short-lived helper sends
`RequestUserStats`, tries a lobby request if needed, shuts down its game session,
and has cancellation, crash isolation, and a twelve-second watchdog. Normal
status polling still uses the game-free native observer. Errors are shown in the
app instead of silently claiming recovery.

| Retest | Short pause | Full 15-minute pause |
| --- | --- | --- |
| Actual blocked duration | 60.269 s | 900.153 s |
| Restore to confirmed native login | **2.218 s** | **3.571 s** |
| Stats request sent | Yes | Yes |
| Lobby fallback needed | No | No |
| Steam PID | 12020 unchanged | 12020 unchanged |
| Login stability after recovery | 10 s | 10 s |
| Owned filters after cleanup | 0 | 0 |

Both retests passed the ten-second restore-to-login target. The long run recorded
30 blocked checkpoints and 914 independent native samples. Immediately after
filter removal, both production and independent status were still offline;
online was reported only after login returned. The long run used the built
application executable's helper entry point, alongside the shared production
recovery, WFP, and status modules. Desktop button/IPC automation was not performed.

The new connection-log bytes name both a QoS transition and
`BYldTryToLoginAndWait (CAPIJobRequestUserStats)` as immediate reconnect reasons.
The stats request therefore reached the expected reconnect job; these trials do
not establish exclusive attribution or guarantee a future latency. The helper's
post-initialization session lasted 1.016 s and 1.270 s respectively. Friends'
visible playing-status lifetime was not measured.

Evidence folders under
`C:\Users\twues\AppData\Local\Packages\OpenAI.Codex_2p2nqsd0c76g0\LocalCache\Local\RBXTools\diagnostics`:
`steam-spacewar-20261008-short` and `steam-spacewar-20261008-15min`, including
events, independent observations, summaries, sanitized reconnect reasons, and
zero exit codes. GPT-6 Luna with high reasoning monitored both tests; GPT-6.1 Sol
implemented and reviewed the results. All test helpers exited, and Steam remained
running and online.

Frontend checks and production build passed. Rust: 21 tests passed, including
helper timeout, crash, and cancellation cleanup; one unrelated live GitHub test
was ignored. Tested portable SHA-256:
`2A249B21803A87EF8E935A4612FAF06813B724D743BC1D272DAF71EE14ACFB92`.

## Fixes

- Status now reads `ISteamUser::BLoggedOn` through an existing Steam client pipe. It does not initialize a game, require an installed game's runtime, or equate process existence with being online. Failed checks show an unknown connection.
- Removing a network block no longer immediately displays a successful connection. Polling updates the status when Steam actually logs back on.
- Both restart modes first remove the app's own temporary network block. Force restart closes the selected Steam installation's client and web helpers, waits for processes already terminating (including Windows error 5), and verifies closure and stable startup.
- Force restart is directly available with the existing confirmation dialog.
- The initial live run below used passive recovery. Its 36.480-second recovery
  failed the user's fast-reconnect requirement, despite passing restart and
  status correctness checks. On 8 October the user authorized the Spacewar call;
  the new recovery helper uses that short-lived session while status checks
  remain game-free. A fresh full-duration latency test is required.
- Native checks have a two-second deadline and allow only one outstanding worker. A stalled IPC call cannot leave an old online result visible or accumulate background workers.
- Network filters now exclude loopback connections so Steam can keep communicating with its local UI and helpers during an internet pause.

## Validation

- Frontend: 15 tests and 2 build-cache checks passed; production frontend build passed.
- Rust: 18 unit tests passed, including stalled IPC timeout and recovery; one unrelated live GitHub download test remained intentionally ignored. Native code check passed.
- The initial final live test passed restart/status functionality, but did not
  pass the subsequently clarified ten-second reconnect target. The opt-in helper
  calls shared production modules with a separate native observer; it does not
  automate the desktop button/IPC path.

| Final live check | Result |
| --- | --- |
| Force restart | Passed: Steam PID 4936 → 16296; restart returned in about 2.7 seconds. Native login then confirmed. |
| Full 15-minute pause | Passed: 900.117 seconds, four Steam-only filters. Steam PID 16296 was checked throughout and stayed unchanged. |
| Offline detection | Passed: production status was offline while blocked, with fresh independent offline samples. A separate production-reader probe also returned offline promptly. |
| Restore status | Correctly remained offline immediately after filter removal; did not confuse network access with login. |
| Recovery | Native login returned 36.480 seconds after restore, with PID 16296 unchanged. Production status then confirmed online. |
| Force restart while blocked | Passed: block removed, PID 16296 → 12020, native production status confirmed online. |
| Cleanup | Zero owned filters; diagnostic helpers exited; Steam left running and online. |

Final evidence: `C:\Users\twues\AppData\Local\Packages\OpenAI.Codex_2p2nqsd0c76g0\LocalCache\Local\RBXTools\diagnostics\steam-fix-20261007-final`, including `events.jsonl`, `events.observer.jsonl`, `bounded-blocked.jsonl`, and `summary.json`. There were 30 blocked checkpoints and 943 independent samples. The persistent observer stayed fresh throughout this successful run, so no reattachment was needed.

Portable build: `C:\Github\RbxTools\release\RBX-Tools.exe`. No version bump, commit, push, or distribution publish was requested or performed.

The first run completed a 900.219-second block and removed all four filters, but failed recovery. Steam shut down about three minutes into the block; the persistent observer then stopped delivering samples. That run is not credited as a pass. Its evidence remains in the `steam-fix-20261007` diagnostics folder. The reason for Steam's shutdown was not independently established. The retry excludes loopback traffic and checks the Steam PID throughout the block.

A subsequent startup-only retry exposed Windows error 5 during force close and was not credited. Force close now waits for the terminating process handle before reporting failure. Microsoft's [TerminateProcess documentation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess) describes asynchronous termination and error 5 for a terminated process. Its evidence is in `steam-fix-20261007-retry`.

## Repeat the live test

The Spacewar recovery variant keeps the existing Steam process and runs the full
requested pause. It checks actual login within ten seconds after restoration and
then requires ten seconds of stable login. Use `900 reconnect` in place of
`900 force` below. Set `RBX_RECONNECT_LAUNCHER` to the built app executable to
exercise its isolated helper entry point; otherwise the example dispatches the
same recovery module itself. AppID 480 is used only after restoration, when
Steam is observed offline. A game-free observer records the entire pause.

This really force-restarts Steam, pauses its traffic for the full 900 seconds, verifies recovery, and force-restarts again while a block is active. Run only for an explicitly requested live test, from an administrator terminal. The dynamic WFP session removes its filters when closed or if the helper exits.

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol --example steam_controls_smoke --example steam_native_reconnect_probe
& .\src-tauri\target\debug\examples\steam_controls_smoke.exe "C:\Program Files (x86)\Steam\steam.exe" "$env:TEMP\rbx-steam-force-test.jsonl" 900 force
```

The helper refuses a preexisting RBX Tools block or inherited game AppID environment variables. It requires a running Steam client and confirms login after restart before starting the timed block.
