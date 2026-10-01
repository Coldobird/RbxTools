# Automatic Spacewar privacy

The **Make Spacewar private** button in **Settings** replaces the old Steam library shortcut.
It uses Valve's account privacy web service and changes only AppID **480**.
It never initializes Steamworks or launches a game while changing privacy.

Steam must be running with a signed-in account and RBX Tools' network block must
be inactive. A dedicated Steam web window handles sign-in and Steam Guard if
needed. The browser session can persist between uses. The short-lived web API
token stays in Steam's web origin; native code receives only a result code.

The action compares the web token's account identifier with Steam's public
`ActiveUser` registry value before issuing any privacy request. It reads
`AccountPrivateApps.GetPrivateAppList`, skips a mutation if 480 is already
private, otherwise calls `AccountPrivateApps.ToggleAppPrivacy` with
`{appids: [480], private: true}`, then reads the list again. It confirms success
only when a successful response lists 480 as private. Other games are untouched.

The web window has its own WebView2 profile, no Tauri application capabilities,
and navigation restricted to exact Steam HTTPS hosts. It cannot open another
window or navigate to the dashboard. Native result notifications are intercepted
before navigation and accepted only from the Steam page where the privacy script
runs. Closing the window cancels the pending action. Restart, network and path
changes are held while the action is pending. The existing reconnect helper is
also prevented from starting during this action.

Valve's account setting persists for subsequent launches. A verified result
does not guarantee privacy for a different account or after a later manual
privacy change. RBX Tools stores no local privacy flag as evidence. The existing
Spacewar reconnect helper is unchanged and can show game status if its account
has not been made private.

## Sources

- [Valve's Private Games FAQ](https://help.steampowered.com/en/faqs/view/1150-C06F-4D62-4966): private games hide ownership, playtime, activity and in-game status from other users.
- Installed Valve `steamui/chunk~2dcc5aaf7.js`: request schema, authenticated service names, HTTP transport (`IAccountPrivateAppsService`), JSON input, `x-eresult` response checking and readback behavior.
- [Steam's Points Shop](https://store.steampowered.com/points/shop/): official sign-in redirect and same-origin account bootstrap endpoint `pointssummary/ajaxgetasyncconfig`.

This account web service is used by Steam's interface but has no documented
Steamworks SDK contract. Unexpected responses report an error; they never
silently fall back to a game-scoped API or claim privacy was confirmed.

## Validation on 2026-09-30

- Production frontend build: passed.
- Desktop executable build with `tauri/custom-protocol`: passed.
- Vitest: 14 passed, including fixed 480-only mutation, already-private behavior,
  account mismatch, missing authentication, failed Steam result, unsuccessful
  readback and malformed responses.
- Rust library tests: 6 passed, including exact-host navigation restrictions and
  rejection of unverified result codes.
- Build-cache checks: 2 passed.
- Browser UI: new label and description, busy state, honest preview notice,
  blocked-network disabling and re-enabling after restore verified.
- Real WebView2 smoke check with a fresh profile: reached Steam sign-in, submitted
  no credentials, cancelled cleanly. The diagnostic example requires Common
  Controls v6; its build now embeds that dependency without requiring elevation.
- Native result bridge smoke check: an intentional service-error result from the
  Steam-origin page was intercepted and returned as an error. No privacy API
  mutation or credential submission was performed.
- An authenticated mutation has **not** been tested against the user's account.
  Complete the first Steam web sign-in to run that final live check.

Use `cargo run --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol
--example spacewar_privacy_smoke` for the unauthenticated handoff check. It uses a
separate profile and closes before credentials can be submitted.
Add `-- --callback` to exercise the native result bridge with a failure code.
