# Steam offline/online switch

4 November 2026. Replaces the WFP block + Spacewar reconnect + `steam.exe` freeze approach.

## Why reconnects took ~2 minutes

`connection_log.txt` shows `No Connect Tokens left available, waiting N seconds`. Steam keeps a
bucket of about 10–12 connect tokens, refilled at roughly one per 220 s. Every connection attempt
spends one, including attempts that fail under a firewall block, so a block drains the bucket and
the next login waits for a refill. Reproduced: 12 rapid reconnects, then a 66 s throttle.

## What the app does now

Steam runs with `-cef-enable-debugging` (RBX Tools' Restart Steam adds it). RBX Tools uses the
loopback DevTools endpoint (`127.0.0.1:8080`, `SharedJSContext`) — see `src-tauri/src/steam_offline.rs`.

| Action | Control | Measured |
| --- | --- | --- |
| Disable the reconnect throttle | console `ClientConnectTokenBucketEnable 0` | 14/14 reconnects in 1.3–2.9 s right after the bucket was empty |
| Offline, no game running | `SteamClient.User.GoOffline()` (Offline Mode, no restart, persisted in `loginusers.vdf`) | 0.2–0.6 s |
| Offline, game running (GoOffline is refused) | console `net_fake_state NoLAN` | 0.5–2.3 s; zero server contact while offline |
| Online | `net_fake_state normal` + `SteamClient.User.GoOnline()` (+ `Connect()` fallback) | 0.8–1.8 s, including with a game running |
| Open a game in use by a family member | Pause, then Restart Steam: it starts in Offline Mode, a fresh session with no server contact | Starts offline with no connection attempts; UI ready in 7 s |

Tested with an AppID 480 Steamworks client as the running game: 20 rapid offline/online cycles,
no crash, no game API stall over 340 ms. Admin rights and WFP are no longer needed for this.

## Rejected

- Suspending `steam.exe`: every Steam API call in the game blocks until resume (10 s stall measured).
- `net_fake_state NoSteam`: still opens a CM connection and sends logon every retry. Switching
  `NoSteam` → `NoLAN` crashed Steam (HTTP client assert).
- `-net_fake_state NoLAN` at startup: breaks Steam's local UI socket; Steam never finishes loading.

## Limits

- The family-sharing lock itself was not reproducible here (needs another family member playing).
- The DevTools port lets any local process control Steam while it is open.
- Steam stays offline when RBX Tools exits, so a shared game session is not ended by accident.
