# Repeated Steam reconnect soak

The opt-in live test exercises the production Steam-only network block and one
production reconnect attempt per restore. It blocks for three minutes, restores
for one minute, and repeats for one hour. It stops after a second reconnect
exceeds ten seconds, or earlier if Steam cannot reconnect during a restore
window and therefore cannot provide a valid baseline for the next cycle. A
successful run requires the full hour, no recovery errors, and fewer than two
slow reconnects. This measures Steam server login, not Friends or game services.

Build the example, then run it in an administrator PowerShell terminal with
exactly one logged-in Steam client and no existing RBX Tools block. It really
interrupts Steam's connection and briefly initializes Spacewar during recovery.
The evidence path must not already exist.

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol --example steam_reconnect_soak
& .\src-tauri\target\debug\examples\steam_reconnect_soak.exe "C:\Program Files (x86)\Steam\steam.exe" "$env:TEMP\rbx-reconnect-soak.jsonl"
```

The test records the Steam process identity, filter counts, observed login,
recovery result and timing in flushed JSONL events. Dynamic WFP ownership removes
the test's filters on process exit, including errors. A fatal error is recorded
in the event file or a `.failure.json` sidecar if startup fails.

## 26 October 2026 run

The first three reconnects took 6.662, 4.230 and 5.291 seconds. On the fourth
cycle, all four filters were removed, but Steam remained offline for the entire
60-second restore window and the production helper reported failure. Steam's
connection log reported that it had no connection tokens available and would
wait 200 seconds before retrying. This is evidence of Steam's retry throttling;
the current request sequence did not overcome it. The run stopped after four
cycles with zero owned filters remaining and did not pass the one-hour test.
Evidence: `output/network-timing/soak-20261026.jsonl`.
An independent read-only follow-up first observed Steam online roughly 145
seconds after the soak ended, about 205 seconds after network restoration.
