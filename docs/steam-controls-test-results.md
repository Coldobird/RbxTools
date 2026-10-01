# Steam controls live test

Run on 2026-10-01 UTC (2026-09-30 in São Paulo), at the user's request.
The Settings text now reads **Automatically hide its status from friends.**

## Results

| Check | Result |
| --- | --- |
| Graceful Steam restart | Passed: PID 20128 → 21960, shutdown/relaunch observed in 3.629 seconds. |
| Steam online after restart | Confirmed by the AppID-free native observer. |
| Stop network | Passed: four Steam-only WFP filters installed and native login became offline. |
| Restore network | Passed: all four owned filters removed after a 10.247-second block. |
| Steam online after restoration | Confirmed 6.774 seconds after restore; PID stayed 21960. |
| Cleanup | Zero owned filters, test helper and observer exited, Steam remained running normally. |

The live smoke test calls the same `steam::restart(..., false)` and
`network::NetworkBlock` implementations used by the desktop commands. It
enumerates the actual Windows WFP filters and uses the independent
`steam_native_reconnect_probe` login observer. It never force-closes Steam or
initializes a game. It does not test the desktop button/IPC path or the
five-minute reconnect-assistance branch.

Evidence is saved in
`C:\Users\twues\AppData\Local\RBXTools\diagnostics\steam-controls-20261001\events.jsonl`
and the adjacent `events.observer.jsonl`.

## Written tests

- `src/api.test.ts`: browser preview boundary and executable names.
- `src/statusPolling.test.ts`: overlapping reads, stale results, busy periods and error recovery.
- `src/spacewarPrivacy.test.mjs`: fixed 480-only mutation, account matching, authentication, idempotence and verified readback.
- Rust library unit tests: Steam path/process matching, updater version comparison and privacy result/navigation checks.
- `src-tauri/examples/steam_controls_smoke.rs`: repeatable opt-in real Steam restart and stop/restore cycle with cleanup and native online verification.
- Existing native reconnect and privacy smoke examples cover other diagnostic flows.

Validation in this run: 14 frontend tests passed, 6 Rust unit tests passed,
production frontend build passed, and the live controls smoke test passed.

## Repeat the live check

Build the helpers, then run the test from an administrator terminal. This really
restarts Steam and interrupts its connection; use it only for an explicitly
requested live test.

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol --example steam_controls_smoke --example steam_native_reconnect_probe
& .\src-tauri\target\debug\examples\steam_controls_smoke.exe "C:\Program Files (x86)\Steam\steam.exe" "$env:TEMP\rbx-steam-controls.jsonl" 90
```

The maximum block is 90 seconds in this command, but restoration occurs sooner
after an offline transition and at least ten seconds of blocking. The helper
rejects a preexisting RBX Tools block, inherited game AppID environment variables
and non-administrator execution. Dynamic WFP ownership also removes its filters
if the helper terminates unexpectedly.
