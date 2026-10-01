# Steam communication recovery plan

Prepared 30 September 2026. Investigation and proposed implementation. The diagnostic recorder, startup guards, four live five-minute disconnect/reconnect trials, and a Friends follow-up have been tested. No candidate has demonstrated a repeatable improvement over natural recovery. See `steam-reconnect-test-results.md` for timings, evidence, and limits.

## Objective

After RBX Tools removes its network block, recover Steam's connection to its servers promptly while keeping the existing Steam process running and without appearing to play Spacewar. Check the Friends/chat transport separately if Steam is logged on but those services remain disconnected.

### Required: zero game presence

The user's latest requirement supersedes the earlier acceptance of a short Spacewar session. AppID 480 is excluded from the implementation, fallbacks, observers, and new tests. Do not substitute another game's AppID or hide the effect by changing the user's persona state. Recovery must use the existing Steam client session without registering a playing game. The old Spacewar records below are historical evidence only.

The working interpretation of “communication tunnel” is Steam's existing client/server connection after Stop / Restore network. A game-specific multiplayer connection needs its own confirmation and cannot be inferred from Steam login alone.

## Findings and evidence

The chats **“Plan game utility tool”** and **“Fix Steam reconnect status”** contain the original experiments. Their recorded results were checked against the local diagnostic files.

| Route | Observed result | Confidence and limit |
| --- | --- | --- |
| `RequestUserStats()` with AppID 480 | Login 2.22 seconds after the call in the nominal 15-minute run and 1.38 seconds after it in the 30-minute run. Steam logged `ScheduleImmediateReconnect` with `CAPIJobRequestUserStats`. | Historical evidence, excluded by the zero-presence requirement. QoS recovery also occurred. |
| `RequestLobbyList()` with AppID 480 | Login 2.35 seconds after the call in the five-minute run, with a matching `CAPIJobMatchmakingRequestLobbyList` reconnect entry. | Historical evidence, excluded by the zero-presence requirement. This run also had a QoS reconnect reason. |
| Stats and lobby without a game AppID | Stats returned an accepted handle; lobby was rejected. In the 15-minute run Steam returned online 46.48 seconds after restoration through its scheduled retry. | Does not establish a forced reconnect. |
| Seven candidates without a game AppID | Friends information, subscribed-file enumeration, Workshop query, relay access, authentication, stats, and lobby produced no matching API-job immediate-reconnect entry in the blocked suite. Relay/authentication reported missing AppID. | Do not repeat this broad sweep as the next experiment. Four-second windows cannot exclude slower effects. |
| `SteamClient.User.Reconnect()` | Five-minute live trial recovered 15.179 seconds after restoration, 11.683 seconds after the call; control recovered in 3.433 seconds. | One run, no demonstrated speed improvement or explicit causal reconnect reason. Internal UI bridge, not a public Steamworks function. |
| `SteamClient.User.Connect()` | Five-minute live trial recovered 5.650 seconds after restoration, 2.202 seconds after the call; control recovered in 3.433 seconds. | One run, with natural-recovery confounding; the call-to-login interval alone is not proof of acceleration. |
| `steam://open/goonline` / `SteamClient.User.GoOnline()` | The URI was dispatched once after a five-minute block; no recovery dialog appeared and Steam stayed offline through the 90-second post-grace observation. | The existing Steam process was preserved. The native GoOnline method was not separately invoked; intentional Offline Mode remains a separate untested case. |
| Friends UI reconnect | An uncontrolled post-timeout follow-up returned native login 44.919 seconds after the call; Friends subsequently reported connected/logged on. | No explicit immediate-reconnect reason establishes attribution. This does not demonstrate prompt or reliable recovery. |

Source files for the new candidates:

- `C:\Program Files (x86)\Steam\steamui\chunk~2dcc5aaf7.js`: native login recovery call, Go Online URI/dialog action, and Friends UI application/transport.
- `C:\Program Files (x86)\Steam\steamui\library.js`: native `User.Connect()` call.
- Installed `steam.exe` file version at inspection: `10.96.30.42`. Internal hooks must be checked again after Steam updates.

Historical records:

- `%LOCALAPPDATA%\RBX Tools\diagnostics\steam-experiment-5min-20260919-023625.json`
- `%LOCALAPPDATA%\RBX Tools\diagnostics\steam-experiment-15min-20260919-035139.json` — actual block lasted about 17 minutes 28 seconds.
- `%LOCALAPPDATA%\RBX Tools\diagnostics\steam-experiment-30min-20260919-025008.json`
- `%TEMP%\rbx-steam-reconnect-15min.txt`
- `%TEMP%\rbx-steam-reconnect-all-candidates-15min.txt`
- `%TEMP%\rbx-steam-reconnect-all-blocked-5min.txt`

Valve documents [BLoggedOn and automatic reconnection](https://partner.steamgames.com/doc/api/ISteamUser?l=english#BLoggedOn), [asynchronous user stats requests](https://partner.steamgames.com/doc/api/ISteamUserStats?l=english#RequestUserStats), and [lobby searches](https://partner.steamgames.com/doc/api/ISteamMatchmaking?l=english#RequestLobbyList). These request APIs have no documented promise to force a reconnect. The matching internal jobs above are experimental evidence.

## Current application gaps

The working tree is version `0.2.12`; it includes uncommitted reconnect and performance work. Preserve that work during implementation.

- `src-tauri/src/lib.rs` starts assistance only after a block lasting at least 300 seconds, then waits three seconds. A shorter block can also leave Steam disconnected.
- `src-tauri/src/steamworks.rs` sets AppID 480 and initializes Steamworks before checking `BLoggedOn()`. This violates the revised requirement and must be removed from the recovery path before any new app-based live test. The current executable still has this behavior; editing this plan does not change it.
- The worker silently returns when runtime loading fails, discards request errors, and exposes no confirmed recovery result.
- `Status` reports process presence and whether RBX Tools owns a block. Neither indicates Steam server login or Friends transport health.
- The existing long-offline example uses the no-AppID probe. Keep that distinction explicit while replacing the production game-initializing helper.
- The README feature summary still describes a no-AppID production path, while its later section correctly describes Spacewar.

## 1. Establish an observable baseline

Extend the standalone diagnostic harness before changing the application strategy.

Record the exact Steam path, PID and process start time, client/runtime versions, actual block duration, block removal time, candidate invocation time, login transitions, API acceptance/completion, reconnect log reason, and presence lifetime. Use monotonic timing for durations and UTC for log correlation. Read new log bytes only; handle truncation and rotation. Sanitize user identifiers and credentials from exports.

Use the existing no-AppID pipe only as a read-only login observer, with no stats/lobby/network request. Confirm that this observer adds neither game presence nor an immediate reconnect job. Keep observation identical in control and candidate trials. Track Friends readiness independently; a socket opening or API call handle alone is insufficient evidence of recovered service.

## 2. Test the native recovery route

First establish whether Steam's own login recovery action or internal `User.Reconnect()` call can accelerate recovery after a long block. Test `User.Connect()` separately if the first candidate fails.

The `steam://open/goonline` external UI route failed the initial ordinary-disconnect screening: it opened no recovery dialog and did not produce login during the observation window. Do not choose it as an automatic recovery action on that evidence. Test intentional Offline Mode separately if that use case is needed; do not enter it as a prerequisite for every reconnect attempt. The current UI also refuses an Offline Mode transition while a game is running, making that distinction important.

These methods exist inside Steam's embedded UI. A Tauri page or ordinary browser page cannot simply call them. Investigate an existing client UI action or a suitable client bridge. If local page debugging is needed to prove the behavior, use it only for the diagnostic experiment, target the correct Steam page, and verify the method exists before one invocation. Any setup that requires a Steam restart happens before the trial, with the new PID recorded as its baseline.

Do not turn a debugging endpoint into a production dependency. Ship the native route only if there is a usable, bounded integration path. Feature detection and client-version checks must fail safely to passive observation or an explicit manual recovery action. Do not guess private DLL vtable slots or URI commands absent from the inspected client code. The observed Go Online URI is a candidate, not a guarantee of forced login.

If native Steam login recovers but Friends stays disconnected, test the existing Friends reconnect action or `g_FriendsUIApp.Reconnect()` in that UI's own context. Keep that result separate from native login recovery. Do not send chat messages as a connectivity test.

## 3. Run controlled comparisons

Each trial uses one candidate, with its own fresh disconnect cycle:

1. Confirm native Steam login and the relevant service are initially available.
2. Block only the same `steam.exe` with the existing dynamic WFP session.
3. Confirm logged-off state and allow the selected 5-, 15-, or 30-minute block to complete.
4. Remove the block and wait the same three-second grace in every arm.
5. If recovery already happened, record natural recovery; do not credit a candidate that was never invoked.
6. Invoke one candidate, or perform no action for the control. Observe for up to 90 seconds, then retain a short stability recording.
7. Verify the Steam process is unchanged and all temporary filters are gone.

Start with matched five-minute no-action control and native reconnect trials. Evaluate native Connect and the Go Online UI route in separate trials if needed. Advance useful candidates to 15 and 30 minutes; run at least three trials per shortlisted arm/duration and alternate the order. Include a short block below five minutes to cover the current threshold gap. Do not run the existing application helper until its AppID 480 initialization has been removed or disabled for the test build.

A useful result requires a real logged-on transition, matching server-login/service evidence, and repeatable improvement over the control. Classify recovery attributed to QoS, clan-list jobs, or scheduled automatic retries as natural/ambiguous. Report invocation-to-login and restoration-to-login separately. Target recovery within ten seconds after invocation; measure failures and slower results instead of promising that deadline.

Require no new game registration or game-presence transition caused by the recorder or candidate, across setup, invocation, timeout, and cleanup. Record the local game-process log and available presence state. Any claim about what friends see requires separate observation; local logs alone do not establish the remote display. Preserve the user's existing game and persona state.

## 4. Implement the winning sequence

Remove the AppID 480 Steamworks initialization from the application recovery path. After traffic restoration, observe login without registering a game. If offline, allow the grace period, invoke the verified native route if it has a viable integration, and verify the result. End assistance as soon as login is confirmed; report timeout or helper failure explicitly. If no automatic candidate passes, expose Steam's tested recovery UI action or let the existing deliberate Restart Steam control remain a user-selected last resort. Never start a game context as a fallback.

If DLL loading remains necessary for the read-only observer or an AppID-free request, put it in a disposable helper process. Earlier probes crashed inside Steam's DLL; an isolated helper keeps the application alive. Remove inherited SteamAppId/SteamGameId values from that helper and never initialize a game context. Give it structured results and a hard deadline, with proper pipe/interface cleanup on normal completion.

Cancel work when the network is blocked again, the path or Steam process changes, a restart begins, or RBX Tools closes. Check those conditions during waits and before every request. Keep one active attempt and make later restore requests observable rather than silently losing them. Base assistance on observed offline state instead of block duration alone.

Expose separate facts for network access, Steam login, and assistance outcome. Keep visual changes minimal and use the existing pixel icons for any new status indication. If login observation is unavailable, report unknown; do not infer online from a running process.

## 5. Validate and prepare delivery

Add focused tests for cancellation, overlapping restore attempts, helper timeouts/crashes, missing runtimes/exports, an already-online client, Steam restart/path changes, and blocks shorter than five minutes. Verify no recovery strategy or fallback initializes a game AppID. Test the replacement production path after long blocks and confirm zero added game presence throughout each run.

Verify dynamic WFP cleanup on normal exit and forced process termination, and confirm the chosen native route fails safely after capability changes. Correct the README's conflicting descriptions. Store trial results with the chosen strategy and its limitations.

When a release is requested, follow `AGENTS.md`: bump the application patch version, build locally, publish the matching executable, NSIS installer and `latest.json` to the shared OneDrive Builds folder, verify version/installer names and file matches, and commit/push to `origin/main` when requested. Confirm the first main build creates the matching `v<version>` GitHub Release.

## Recommended next step

The initial screening did not identify a winning strategy. Keep AppID 480 excluded, and do not ship the diagnostic debugger or infer an acceleration from the short `Connect()` call-to-login interval. The next useful experiment would be alternating, repeated control/`Connect()` trials under a confirmed longer retry backoff, together with a usable integration that does not require diagnostic mode. Longer 15-/30-minute trials, abrupt-termination cleanup, and production integration remain unvalidated. Remove the existing AppID 480 path before any production-path testing.

Steam was initially closed. The first administrator launch was canceled; the authorized retry succeeded. Four dynamic Steam-only WFP blocks and the AppID-free candidates were tested using a loopback-only CEF endpoint. Every block was closed, the helpers and coordinator exited, and Steam/Friends recovered with the trial PID unchanged. Cleanup restored a normal Steam launch without diagnostic mode and verified native login again. Production recovery code, application version, commits, and published builds have not been changed by this investigation.
