//! Opt-in live test that drops Steam-only traffic while the production reconnect helper runs.
//! Requires administrator access. It uses AppID 480 through the existing production helper.
#[allow(dead_code)]
#[path = "../src/network.rs"]
mod network;
#[allow(dead_code)]
#[path = "../src/steam.rs"]
mod steam;
#[allow(dead_code)]
#[path = "../src/steam_connection.rs"]
mod steam_connection;
#[path = "../src/steam_reconnect.rs"]
mod steam_reconnect;
#[allow(dead_code)]
#[path = "../src/steamworks.rs"]
mod steamworks;

use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::HANDLE,
    NetworkManagement::WindowsFilteringPlatform::*,
    System::Rpc::RPC_C_AUTHN_WINNT,
};

const EXPECTED_RECONNECT_ERROR: &str =
    "Steam reconnect requests finished, but Steam is still offline.";
const CHILD_TIMEOUT: Duration = Duration::from_secs(240);
const ONLINE_TIMEOUT: Duration = Duration::from_secs(90);

fn check(code: u32, operation: &str) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("{operation} failed: Windows 0x{code:08X}"))
    }
}

/// Count only filters owned by the RBX Tools Steam sublayer.
fn owned_filters() -> Result<usize, String> {
    unsafe {
        let mut engine: HANDLE = std::ptr::null_mut();
        check(
            FwpmEngineOpen0(
                std::ptr::null(),
                RPC_C_AUTHN_WINNT,
                std::ptr::null(),
                std::ptr::null(),
                &mut engine,
            ),
            "Opening WFP for inspection",
        )?;
        struct Engine(HANDLE);
        impl Drop for Engine {
            fn drop(&mut self) {
                unsafe {
                    FwpmEngineClose0(self.0);
                }
            }
        }
        let _engine = Engine(engine);

        let mut enumeration: HANDLE = std::ptr::null_mut();
        check(
            FwpmFilterCreateEnumHandle0(engine, std::ptr::null(), &mut enumeration),
            "Creating WFP filter enumeration",
        )?;
        struct Enumeration(HANDLE, HANDLE);
        impl Drop for Enumeration {
            fn drop(&mut self) {
                unsafe {
                    FwpmFilterDestroyEnumHandle0(self.0, self.1);
                }
            }
        }
        let _enumeration = Enumeration(engine, enumeration);

        let mut count = 0usize;
        loop {
            let mut entries: *mut *mut FWPM_FILTER0 = std::ptr::null_mut();
            let mut returned = 0u32;
            check(
                FwpmFilterEnum0(engine, enumeration, 256, &mut entries, &mut returned),
                "Enumerating WFP filters",
            )?;
            for index in 0..returned as usize {
                let filter = &**entries.add(index);
                let key = filter.subLayerKey;
                if key.data1 == 0x68b17ba8
                    && key.data2 == 0xf37e
                    && key.data3 == 0x4d7e
                    && key.data4 == [0x93, 0x7b, 0xc2, 0x72, 0x78, 0x85, 0x43, 0x8b]
                {
                    count += 1;
                }
            }
            if !entries.is_null() {
                FwpmFreeMemory0((&mut entries as *mut *mut *mut FWPM_FILTER0).cast());
            }
            if returned < 256 {
                break;
            }
        }
        Ok(count)
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| format!("Could not write {}: {error}", path.display()))
}

fn reconnect_helper() -> Option<i32> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_none_or(|arg| arg != "--rbx-reconnect-helper") {
        return None;
    }
    let result = (|| -> Result<i32, String> {
        if !steam::is_elevated() {
            return Err("Reconnect dropout helper requires administrator access.".into());
        }
        let steam_path = args
            .get(2)
            .map(PathBuf::from)
            .ok_or("Reconnect helper did not provide the Steam path.")?;
        let marker = std::env::var_os("RBX_DROPOUT_MARKER")
            .map(PathBuf::from)
            .ok_or("RBX_DROPOUT_MARKER was not passed to the helper.")?;
        let steam_path = steam::validate_path(&steam_path)?;
        let pids = steam::matching_processes(&steam_path)?;
        if pids.len() != 1 {
            return Err("Expected the same single Steam client during reconnect dropout.".into());
        }

        // The production recovery helper is dispatched below while this dynamic,
        // Steam-executable-only block is active. Its drop restores traffic.
        let mut block = network::NetworkBlock::start(&steam_path)?;
        let mut evidence = json!({
            "phase": "blocked_before_production_helper",
            "utcMs": unix_ms(),
            "steamPids": pids,
            "steamExecutable": steam_path,
            "filtersWhileBlocked": owned_filters()?,
            "productionRecoveryAppId": 480
        });
        write_json(&marker, &evidence)?;

        let code = steam_reconnect::run_helper_from_args()
            .ok_or("Reconnect helper dispatch unexpectedly declined its arguments.")?;
        let close_result = block.close();
        evidence["phase"] = json!("production_helper_returned_block_released");
        evidence["helperExitCode"] = json!(code);
        evidence["blockReleasedUtcMs"] = json!(unix_ms());
        evidence["blockCloseSucceeded"] = json!(close_result.is_ok());
        evidence["blockCloseError"] = close_result
            .as_ref()
            .err()
            .map(|error| json!(error))
            .unwrap_or(Value::Null);
        evidence["filtersAfterBlock"] = json!(owned_filters()?);
        write_json(&marker, &evidence)?;
        close_result?;
        Ok(code)
    })();
    Some(match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Reconnect dropout helper failed: {error}");
            2
        }
    })
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn wait_for_smoke(child: &mut Child) -> Result<std::process::ExitStatus, String> {
    let deadline = Instant::now() + CHILD_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err("Steam controls smoke exceeded its four-minute limit.".into());
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn expected_dropout(events_path: &Path) -> Result<(), String> {
    let contents = fs::read_to_string(events_path)
        .map_err(|error| format!("Could not read smoke events: {error}"))?;
    let mut saw_expected = false;
    for line in contents.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if event["event"] == "failed"
            && event["details"]["message"] == EXPECTED_RECONNECT_ERROR
        {
            saw_expected = true;
        }
        if event["event"] == "complete" {
            return Err("Steam reconnect smoke unexpectedly passed during the injected dropout.".into());
        }
    }
    if saw_expected {
        Ok(())
    } else {
        Err("Smoke did not record the expected offline reconnect failure.".into())
    }
}

fn wait_until_online(path: &Path, expected_pids: &[u32]) -> Result<(), String> {
    let deadline = Instant::now() + ONLINE_TIMEOUT;
    while Instant::now() < deadline {
        let current = steam::matching_processes(path)?;
        if current != expected_pids {
            return Err(format!(
                "Steam process changed during cleanup/recovery: expected {expected_pids:?}, saw {current:?}."
            ));
        }
        if steam_connection::online(path).unwrap_or(false) {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }
    Err("Steam did not recover online within 90 seconds after the injected dropout.".into())
}

fn run_parent(steam_arg: &str, events_arg: &str) -> Result<(), String> {
    if !steam::is_elevated() {
        return Err("Run this live test from an administrator terminal.".into());
    }
    if std::env::var_os("SteamAppId").is_some() || std::env::var_os("SteamGameId").is_some() {
        return Err("Refusing inherited game AppID environment.".into());
    }
    let steam_path = steam::validate_path(Path::new(steam_arg))?;
    let initial_pids = steam::matching_processes(&steam_path)?;
    if initial_pids.len() != 1 {
        return Err("Exactly one running Steam client is required.".into());
    }
    let filters_before = owned_filters()?;
    if filters_before != 0 {
        return Err("Restore any existing RBX Tools network block before testing.".into());
    }

    let events_path = PathBuf::from(events_arg);
    let marker_path = events_path.with_extension("dropout-marker.json");
    let summary_path = events_path.with_extension("dropout-summary.json");
    for path in [&events_path, &marker_path, &summary_path] {
        if path.exists() {
            return Err(format!("Refusing to overwrite evidence file {}.", path.display()));
        }
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let smoke_exe = executable.with_file_name("steam_controls_smoke.exe");
    if !smoke_exe.is_file() {
        return Err(format!(
            "Build steam_controls_smoke.exe beside {} first.",
            executable.display()
        ));
    }

    let mut command = Command::new(&smoke_exe);
    command
        .arg(&steam_path)
        .arg(&events_path)
        .arg("60")
        .arg("reconnect")
        .env("RBX_RECONNECT_LAUNCHER", &executable)
        .env("RBX_DROPOUT_MARKER", &marker_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = ChildGuard(command.spawn().map_err(|error| error.to_string())?);
    let smoke_status = wait_for_smoke(&mut child.0)?;
    drop(child);

    let marker: Value = serde_json::from_slice(
        &fs::read(&marker_path).map_err(|error| format!("Dropout marker missing: {error}"))?,
    )
    .map_err(|error| format!("Dropout marker is invalid: {error}"))?;
    if marker["phase"] != "production_helper_returned_block_released"
        || marker["helperExitCode"] != 0
        || marker["filtersWhileBlocked"] != 4
        || marker["filtersAfterBlock"] != 0
        || marker["blockCloseSucceeded"] != true
    {
        return Err(format!(
            "The production helper did not complete the expected blocked attempt: {marker}"
        ));
    }
    expected_dropout(&events_path)?;
    if smoke_status.success() {
        return Err("Smoke reported success despite the required reconnect dropout.".into());
    }

    let filters_after_smoke = owned_filters()?;
    if filters_after_smoke != 0 {
        return Err(format!("RBX Tools WFP filters remain after smoke cleanup: {filters_after_smoke}."));
    }
    wait_until_online(&steam_path, &initial_pids)?;
    let filters_final = owned_filters()?;
    if filters_final != 0 {
        return Err(format!("RBX Tools WFP filters remain after recovery: {filters_final}."));
    }
    let summary = json!({
        "passed": true,
        "startedUtcMs": unix_ms(),
        "steamPidsBeforeAndAfter": initial_pids,
        "smokeExitCode": smoke_status.code(),
        "expectedReconnectFailure": EXPECTED_RECONNECT_ERROR,
        "dropout": marker,
        "ownedFiltersAfterSmoke": filters_after_smoke,
        "ownedFiltersFinal": filters_final,
        "nativeOnlineAfterRecovery": true,
        "observerGameInitialized": false,
        "productionRecoveryAppId": 480,
        "evidence": events_path
    });
    write_json(&summary_path, &summary)?;
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    Ok(())
}

fn main() -> Result<(), String> {
    if let Some(code) = steam_connection::run_helper_from_args() {
        std::process::exit(code);
    }
    if let Some(code) = reconnect_helper() {
        std::process::exit(code);
    }

    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "Usage: steam_reconnect_dropout_smoke <steam.exe> <new-events.jsonl>".into(),
        );
    }
    let result = run_parent(&args[0], &args[1]);
    if let Err(error) = &result {
        // Keep errors observable when the elevated test has no console, without
        // overwriting any existing evidence.
        let failure_path = Path::new(&args[1]).with_extension("failure.json");
        if let Ok(mut file) = fs::OpenOptions::new().write(true).create_new(true).open(failure_path) {
            use std::io::Write;
            let details = json!({"passed": false, "error": error, "utcMs": unix_ms(),
                "ownedFilters": owned_filters().ok()});
            let _ = file.write_all(details.to_string().as_bytes());
        }
    }
    result
}
