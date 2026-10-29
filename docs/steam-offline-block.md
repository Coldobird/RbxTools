# Steam Offline Mode block diagnostic

This one-shot diagnostic measures a manual Steam Offline Mode / Go Online
sequence around one 180-second production WFP Steam block. It requires an
administrator terminal, exactly one running Steam client, no inherited
`SteamAppId`/`SteamGameId`, and a new evidence path. It refuses preexisting
RBX Tools filters and requires a read-only baseline showing Steam offline.
The offline-mode confirmation is an operator attestation; `BLoggedOn == false`
alone cannot distinguish intentional Offline Mode from a network failure.

Select **Steam → Go Offline…** manually before running it. The diagnostic does
not change Steam's UI mode, invoke a reconnect API, or restart Steam. It is
noninteractive so it can run hidden/elevated. After restoring the WFP session,
it writes and flushes a `block_restored` event followed by
`manual_go_online_ready`, then starts a read-only online observation window.
Watch the evidence file for `manual_go_online_ready`; manually select **Go
Online** in Steam after seeing it. The first online sample is timed from when
post-restore observation starts, so that measurement includes the operator's
reaction time.

Build and run from an elevated PowerShell terminal:

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --example steam_offline_block
$events = Join-Path $env:TEMP 'steam-offline-block.jsonl'
& .\src-tauri\target\debug\examples\steam_offline_block.exe `
  'C:\Program Files (x86)\Steam\steam.exe' $events --offline-confirmed
```

The example creates the JSONL file exclusively and never overwrites it. It
records the Steam PID, offline baseline result, WFP filter counts, block and
restore result, manual-Go-Online-ready event, online transitions, and timeout
or completion. It uses the existing `NetworkBlock` RAII cleanup and verifies
zero owned filters after restoration. It does not read Steam logs or record
account identifiers.

## 27 October 2026 trial

Steam entered Offline Mode without changing its process ID. A three-minute
block was applied and removed with zero owned filters remaining. Steam's log
showed no automatic reconnect attempts between the deliberate logoff and Go
Online; after the manual Go Online action it recorded login within the next
second. The read-only observer confirmed online, 42.361 seconds after the
network restore; this includes the delay before the manual UI action. This is
one trial and does not establish reliable behavior across repeated cycles or a
safe automatic integration. Evidence:
`output/network-timing/offline-block-20261027-retry.jsonl`.
