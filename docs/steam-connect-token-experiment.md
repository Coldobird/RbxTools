# Steam Connect Token recovery experiment

This diagnostic tries to reproduce the native Steam client log message
`No Connect Tokens left available` through the production three-minute WFP
block / one-minute restore cycle. A matching line is actionable only if its
scheduled retry still has at least 30 seconds remaining after network restore.
Short waits, including waits that expire while Steam is blocked, are recorded
and ignored; normal recovery and later cycles continue. On the first actionable
wait, the example uses the existing CEF diagnostic to call
`SteamClient.User.Connect()` once, then compares the scheduled deadline with
read-only `BLoggedOn` samples taken after the call.
The one-hour cycle limit and five-minute post-trigger observation make the run
bounded.

The experiment uses `steam_reconnect::recover` on restores before the token
message appears, so it also uses the existing short-lived AppID 480 (Spacewar)
helper. Close all games first. The diagnostic refuses an active or unknown
Steam game-presence value. Do not use it during a game session.

## Build and run

The run requires administrator PowerShell, exactly one logged-in Steam
process, no preexisting RBX Tools WFP filters, a currently enabled Steam CEF
debugger on `127.0.0.1:8080`, and Node.js on `PATH`. Node 20.17 requires the
`--experimental-websocket` flag used by the coordinator. It does not enable the
debugger, restart Steam, or launch a privileged test itself. The built
`steam_native_reconnect_probe.exe` must be beside the experiment executable;
the supplied evidence paths must not already exist.

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --example steam_connect_token_experiment --example steam_native_reconnect_probe
$events = Join-Path $env:TEMP 'steam-connect-token-events.jsonl'
$observer = Join-Path $env:TEMP 'steam-connect-token-observer.jsonl'
& .\src-tauri\target\debug\examples\steam_connect_token_experiment.exe `
  'C:\Program Files (x86)\Steam\steam.exe' $events $observer
```

The example refuses to overwrite either evidence path. It samples the current
connection-log byte position at startup, then watches only new bytes. It handles
truncation/replacement by skipping preexisting bytes from the replacement log.
It never writes or exports raw Steam log lines or account identifiers; JSONL
evidence contains only the parsed retry delay/attempt, timestamps, process PID,
filter counts, recovery results, and online transitions. The native-connect
script's detailed before/after state is written to an automatically deleted
temporary file after reducing it to a small event record. Its debugger
WebSocket closes after each evaluation.

Every WFP `NetworkBlock` is explicitly closed and also has its existing RAII
drop cleanup. The read-only observer child is terminated on normal exit or
error. If the token message never appears within one hour, the run fails with
`tokenEventFound=false`; if Steam does not return online within five minutes,
the coordinator stops without starting another block. A candidate skipped
because Steam recovered before invocation is recorded as skipped and does not
count as a completed intervention.

## Interpreting results

The `connect_token_wait` event records each new log line's scheduled seconds.
`token_wait_ignored` records waits that expired or fell below 30 seconds before
the coordinator could safely act. `native_connect_invoked` records the single
candidate call. It is deferred until traffic has been restored, even if the
token message appears during the block. The evidence records the remaining
scheduled wait at invocation, the first online observer sample after invocation,
and whether login beat the still-pending deadline. A login before that deadline is only a promising result; repeat
matched control and candidate episodes before attributing causality. The test
does not measure Friends/chat readiness, and a skipped call cannot establish
that `Connect()` reset the token wait.

## 27 October 2026 trial

After three cycles, Steam reported a two-second token wait while blocked. The
diagnostic invoked `Connect()` after restoring the network; Steam then reported
a new 183-second token wait. The first online sample arrived about 185.5 seconds
after that wait was reported. This single trial shows no bypass of the token
wait; it does not establish that the request caused the eventual login. All
filters were removed, the helpers exited, and Steam was relaunched normally
with the local debugger closed. Evidence:
`output/network-timing/connect-token-20261027.jsonl`.
