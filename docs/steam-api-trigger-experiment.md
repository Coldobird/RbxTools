# Steam API trigger experiment

This opt-in diagnostic compares three randomized arms after temporary, three-minute WFP blocks targeting the Steam executable:

- `control_no_call`: no Steamworks API call after restore.
- `appid480_stats_only`: one AppID 480 `RequestUserStats` call in a disposable helper process.
- `appid480_lobby_only`: one AppID 480 `RequestLobbyList` call in a disposable helper process.

It does not call production recovery code, restart Steam, or automate Steam UI. The AppID 480 arms can temporarily present as Spacewar. Use them only when that side effect is acceptable. The test requires no game to be active and stops if the online baseline, Steam process identity, or filter cleanup checks fail.

## Build

From an elevated PowerShell terminal in the repository:

```powershell
Set-Location C:\Github\RbxTools\src-tauri
cargo build --example steam_api_trigger_experiment --example steam_native_reconnect_probe
```

The native observer executable must be beside the experiment executable, as produced by this command. The experiment also requires Node.js and the Steam CEF debugger endpoint used by `scripts/steam-native-reconnect-test.mjs inspect`.

## Run

Create a new evidence directory for every run. Replace the Steam path if it is installed elsewhere:

```powershell
$steam = 'C:\Program Files (x86)\Steam\steam.exe'
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$evidence = "C:\Github\RbxTools\output\steam-api-trigger-$stamp"
New-Item -ItemType Directory -Path $evidence
& 'C:\Github\RbxTools\src-tauri\target\debug\examples\steam_api_trigger_experiment.exe' `
  $steam `
  (Join-Path $evidence 'events.jsonl') `
  (Join-Path $evidence 'observer.jsonl') `
  --run-live --allow-spacewar
```

By default, the experiment stops if Steam has not returned to a stable online baseline by the end of a trial's original 60-second restore window. To opt into continuing the schedule, add `--continue-after-slow`:

```powershell
  --run-live --allow-spacewar --continue-after-slow
```

This preserves the 60-second `trial_result` as-is. It then makes no further API calls and starts no new network block while waiting up to five additional minutes for ten seconds of stable native-online status. The cooldown and any late login are written as separate events, including actual extra dwell. A treatment counts toward the replicate total only when its API call was attempted; an attempted treatment that missed the original 60-second window remains explicitly counted as an outside-window result. If the cooldown does not reach a stable baseline, the experiment stops. This mode creates unequal dwell times and possible carryover between randomized arms, so analyze its later trials separately and treat it as exploratory. Without the flag, the original stop-at-60-seconds behavior is unchanged.

The process must be elevated. Both evidence files must be new. Keep Steam running and do not start a game while the experiment runs. Typical duration is several cycles: each valid replicate uses a 180-second block plus a 60-second restored observation. The randomized schedule permits up to four assignments per arm to obtain at least two valid replicates for each arm; a failed precondition or unstable recovery stops the run early.

## Operator-attested game check

The default run uses the strict CEF game-ID check and requires the Steam CEF debugger. An alternate mode avoids CEF only when the operator explicitly attests that no game is active:

```powershell
& 'C:\Github\RbxTools\src-tauri\target\debug\examples\steam_api_trigger_experiment.exe' `
  $steam `
  (Join-Path $evidence 'events.jsonl') `
  (Join-Path $evidence 'observer.jsonl') `
  --run-live --allow-spacewar --attest-no-game
```

This mode requires a readable `Steam\logs\gameprocess_log.txt` with a startup marker newer than the current Steam PID. It starts tracking after the latest startup marker, rejects active tracked AppID/PID entries, monitors the log during each block and before candidate requests, rejects unknown games-list changes, and aborts if the log is truncated, replaced, or becomes unreadable. After a treatment helper exits, it waits for the helper PID’s tracked-game entry to clear before continuing.

The operator attestation and log checks are diagnostic safeguards, not a complete game detector. Steam’s process log is an internal, undocumented format and cannot reliably detect every game or launch path. Use this option only when the CEF debugger is unavailable and the operator has independently confirmed that no game is active. The evidence records which guard mode was selected and the latest startup marker used.

## Evidence and interpretation

`events.jsonl` records the randomized assignment, baseline, block and restore results, API helper start/finish/error/timeout, native login observation, per-trial result, and final counts. `observer.jsonl` contains one-second read-only native login samples. A fresh sibling `observer.stderr.txt` captures observer startup errors; the coordinator waits up to 15 seconds for a fresh first sample and records early observer exits or startup timeouts. The stats and lobby calls execute in separate child processes with a ten-second deadline, and each child makes at most one candidate call. Candidate calls occur only after WFP restoration and a three-second natural-recovery grace period; if Steam is already online, the call is skipped.

The coordinator requires exactly one Steam process and verifies that its PID remains unchanged. By default, it checks Steam game presence through a read-only CEF inspection before starting and before each candidate request, requiring game ID `0`. The optional operator-attested mode uses the limitations described above. Both modes refuse preexisting RBX Tools WFP filters, check the expected four filters while blocked, and explicitly close the dynamic WFP session before proceeding. RAII cleanup also closes the session on early return; process termination removes filters from the dynamic session.

The ten-second slow target is a reporting threshold, not an API guarantee. A successful API call means Steam accepted a call handle; it does not prove the backend completed the request or that the call caused a reconnect. Stats may use an offline cache, and lobby search is game matchmaking traffic. Interpret treatment timing against randomized no-call controls as an experiment result, not as proof of a supported reconnect command.

## 27 October 2026 comparison

A continued run completed two valid trials per arm with the same Steam PID.
Each trial blocked for three minutes; treatment requests were made only after
restoration and a three-second natural-recovery grace period. When Steam did
not regain a stable login during the restored minute, the diagnostic left
network access restored and waited without sending another request.

| Arm | Restore-to-login times |
| --- | --- |
| No request | 205.6 s, 18.8 s |
| Stats only | 18.7 s, 205.3 s |
| Lobby only | 206.7 s, 17.8 s |

All four treatment requests were accepted, and all six trials exceeded the
ten-second target. Each arm had one login outside the first minute. These small
samples and unequal cooldowns do not establish causality; they show neither
request reliably bypassed the long wait in this run. Every block closed, zero
owned filters remained, and Steam returned online without restarting. Evidence:
`output/network-timing/api-comparison-20261027-continued/events.jsonl`.
