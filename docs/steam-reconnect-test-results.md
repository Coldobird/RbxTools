# Steam reconnect test results

30 September 2026 (São Paulo). Requirement: recover the existing Steam connection without registering a playing game, including Spacewar.

## Live screening outcome

The administrator retry succeeded. Four separate five-minute Steam-only blocks were completed using a dynamic Windows Filtering Platform session and the same AppID-free observer. Steam PID `10440` remained unchanged throughout the trials and follow-up observations. Installed executable version: `10.96.30.42`.

| Arm | Restore to native login | Call to native login | Observed outcome |
| --- | --- | --- | --- |
| No-action control | 3.433 s | — | Recovered through Steam's own connectivity test. The log names `ScheduleImmediateReconnect() Connectivity test result`. |
| `SteamClient.User.Reconnect()` | 15.179 s | 11.683 s | Call accepted; recovery was slower than the control. No matching explicit immediate-reconnect reason establishes causality. |
| `SteamClient.User.Connect()` | 5.650 s | 2.202 s | Call accepted; total recovery was slower than the control. The short call-to-login interval alone does not establish that this call caused recovery. |
| `steam://open/goonline` | No login during the observation window | No login during the observation window | URI dispatched once, no recovery dialog appeared, and the main UI retained `NO CONNECTION`. Steam stayed offline for the 90-second observation after the three-second grace. |

All four blocks were restored successfully. The privileged helper completed and exited. The first three recovered arms retained at least ten seconds of online stability before the next block.

There was one run per arm, in fixed order. Steam's connectivity checks, endpoint selection, and randomized retry delays confound attribution. These measurements do not establish a repeatable speed improvement. No candidate currently qualifies as a verified fast recovery strategy, so no candidate was advanced to longer-duration trials or production integration.

## Friends follow-up

During the final block, inspected Friends transport fields showed disconnected, logged off, and connection failed. After the Go Online timeout, a separate `g_FriendsUIApp.Reconnect()` call was tried with traffic already restored. Native login returned 44.919 seconds after that call; both Friends connected and logged-on fields were subsequently confirmed true. This was an uncontrolled follow-up, with no matching explicit immediate-reconnect reason, so it is not credited as forced recovery.

A later isolated native `Connect()` call occurred after Steam was already online and is excluded from recovery evidence. The diagnostic invocation tool was then tightened to require a current observer sample and skip an already-connected client; that guard was verified to skip the call.

## Zero-game-presence and guard checks

- Every recorded local game ID was `0`. There were zero added game-process log entries before cleanup, and no Spacewar registration. No game API was initialized by the diagnostic helper or candidates. Remote friends' displays were not independently observed.
- The independent observer built successfully and recorded native login transitions without a game AppID. The repeated initial 20-second baseline recorded 20 online samples.
- Inherited `SteamAppId` and `SteamGameId` each caused exit code 1 before observer-file creation.
- A preexisting cancellation file prevented block creation. This covers cancellation before a block, not forced termination during an active block.
- Normal WFP session closure succeeded for all four live trials, followed by helper exit. Abrupt-termination cleanup was not separately tested.
- Both diagnostic JavaScript files passed syntax checks. Application recovery code was not launched or modified: its preexisting AppID 480 path still needs replacement before production testing under this requirement.

## Evidence and cleanup

Live diagnostic directory: `C:\Users\twues\AppData\Local\RBX Tools\diagnostics\steam-native-20260930-retry`.

- `screening-events.jsonl`, `actions.jsonl`, and `screening-trials.json`: actual block/restore times, native transitions, invocations, cleanup results, and recipe.
- `screening-results.json`: sanitized result snapshot before cleanup, including zero added game-process entries.
- `presence.jsonl`: local game ID and, for later observations, separate Friends connection fields.
- `followup-actions.jsonl`, `friends-followup-observer.jsonl`, and `connect-followup-observer.jsonl`: exploratory follow-up evidence.
- `guard-actions.jsonl` and `already-online-guard.jsonl`: confirmed skip of an already-connected client.
- `session.json` and `log-offsets.json`: process/version baseline and initial log positions.
- `results.json`: regenerated sanitized log report; the final report also includes administrative startup lines from cleanup.

Initial read-only checks and the canceled first administrator launch remain recorded in `C:\Users\twues\AppData\Local\RBX Tools\diagnostics\steam-native-20260930`.

Cleanup: the trial coordinator and diagnostic helpers have exited. Steam and Friends recovered before cleanup. Steam was gracefully restarted normally, the loopback debugging endpoint is closed, and the final native observer completed online. `cleanup-session.json` records the checks. No production change, version bump, commit, or publish was made.
