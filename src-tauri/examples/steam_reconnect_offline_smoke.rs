#[allow(dead_code)]
#[path = "../src/network.rs"]
mod network;
#[allow(dead_code)]
#[path = "support/steamworks_noappid.rs"]
mod steamworks;

use std::{
    env,
    fs::{self, File},
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const BLOCK_DURATION: Duration = Duration::from_secs(15 * 60);

fn record(output: &mut File, started: Instant, event: &str) -> Result<(), String> {
    let utc = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    writeln!(
        output,
        "utc={utc} elapsed_ms={} {event}",
        started.elapsed().as_millis()
    )
    .map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())
}

fn run_candidate(mut command: Command) -> Result<(Output, bool), String> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut timed_out = false;
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            child.kill().map_err(|e| e.to_string())?;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    Ok((output, timed_out))
}

fn main() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let steam_path = args.next().ok_or("steam.exe path required")?;
    let output_path = args.next().ok_or("output path required")?;
    let mode = args.next().unwrap_or_default();
    let block_duration = args
        .next()
        .map(|value| value.parse::<u64>().map(Duration::from_secs))
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or(BLOCK_DURATION);
    let ugc_app_id = args.next().unwrap_or_else(|| "960090".into());
    if mode == "all" || mode == "all-blocked" {
        ugc_app_id.parse::<u32>().map_err(|e| e.to_string())?;
    }
    let mut output = File::create(output_path).map_err(|e| e.to_string())?;
    let steam_path = Path::new(&steam_path);
    let started = Instant::now();

    let probe = steamworks::Client::connect(steam_path, None)?;
    record(
        &mut output,
        started,
        &format!("initial_online={}", probe.is_logged_on()),
    )?;

    let mut block = network::NetworkBlock::start(steam_path)?;
    let blocked_at = Instant::now();
    record(&mut output, started, "network_blocked=true")?;
    let mut offline_seen = false;
    let mut next_sample = Duration::ZERO;
    while blocked_at.elapsed() < block_duration {
        if blocked_at.elapsed() >= next_sample {
            let online = probe.is_logged_on();
            offline_seen |= !online;
            record(&mut output, started, &format!("blocked_online={online}"))?;
            next_sample += Duration::from_secs(30);
        }
        thread::sleep(Duration::from_secs(1));
    }
    let offline_at_restore = !probe.is_logged_on();
    offline_seen |= offline_at_restore;
    record(
        &mut output,
        started,
        &format!("offline_seen={offline_seen} offline_at_restore={offline_at_restore}"),
    )?;
    if mode == "all-blocked" && offline_at_restore {
        let runtime = steamworks::discover_runtime(steam_path)?;
        let candidate_exe = env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("steam_reconnect_candidate.exe");
        let connection_log = steam_path
            .parent()
            .ok_or("Steam directory unavailable")?
            .join("logs")
            .join("connection_log.txt");
        for name in [
            "request-user-information",
            "enumerate-subscribed-files",
            "query-subscribed-ugc",
            "init-relay-network-access",
            "init-network-authentication",
            "request-user-stats",
            "request-lobby-list",
        ] {
            let log_before = fs::metadata(&connection_log).map(|m| m.len()).unwrap_or(0);
            record(
                &mut output,
                started,
                &format!("blocked_candidate_start={name}"),
            )?;
            let mut command = Command::new(&candidate_exe);
            command
                .arg(name)
                .arg(steam_path.parent().ok_or("Steam directory unavailable")?)
                .arg(&runtime)
                .arg(&ugc_app_id)
                .env_remove("SteamAppId")
                .env_remove("SteamGameId");
            let (request, timed_out) =
                run_candidate(command).map_err(|e| format!("Could not run {name}: {e}"))?;
            thread::sleep(Duration::from_secs(1));
            record(
                &mut output,
                started,
                &format!(
                    "blocked_candidate_result={name} exit={:?} timed_out={timed_out} stdout={:?} stderr={:?}",
                    request.status.code(),
                    String::from_utf8_lossy(&request.stdout).trim(),
                    String::from_utf8_lossy(&request.stderr).trim()
                ),
            )?;
            let log = fs::read(&connection_log).unwrap_or_default();
            let added = log.get(log_before as usize..).unwrap_or_default();
            for line in String::from_utf8_lossy(added).lines() {
                if let Some(reason) = line.split("ScheduleImmediateReconnect()").nth(1) {
                    record(
                        &mut output,
                        started,
                        &format!("blocked_immediate_reconnect={}", reason.trim()),
                    )?;
                }
            }
            record(
                &mut output,
                started,
                &format!(
                    "blocked_candidate_observed={name} online={}",
                    probe.is_logged_on()
                ),
            )?;
        }
    }
    drop(probe);

    block.close()?;
    let restored_at = Instant::now();
    record(&mut output, started, "network_restored=true")?;
    if !offline_at_restore {
        return Ok(());
    }

    thread::sleep(Duration::from_secs(3));
    let client = match steamworks::Client::connect(steam_path, None) {
        Ok(client) => client,
        Err(error) => {
            record(
                &mut output,
                started,
                &format!("client_connect_error={error}"),
            )?;
            return Err(error);
        }
    };
    let before_request = client.is_logged_on();
    record(
        &mut output,
        started,
        &format!("online_before_request={before_request}"),
    )?;
    if !before_request {
        if mode == "all" {
            let runtime = steamworks::discover_runtime(steam_path)?;
            let candidate_exe = env::current_exe()
                .map_err(|e| e.to_string())?
                .with_file_name("steam_reconnect_candidate.exe");
            let connection_log = steam_path
                .parent()
                .ok_or("Steam directory unavailable")?
                .join("logs")
                .join("connection_log.txt");
            for name in [
                "request-user-information",
                "enumerate-subscribed-files",
                "query-subscribed-ugc",
                "init-relay-network-access",
                "init-network-authentication",
                "request-user-stats",
                "request-lobby-list",
            ] {
                if client.is_logged_on() {
                    record(&mut output, started, "online_before_next_candidate=true")?;
                    break;
                }
                let log_before = fs::metadata(&connection_log).map(|m| m.len()).unwrap_or(0);
                record(&mut output, started, &format!("candidate_start={name}"))?;
                let mut command = Command::new(&candidate_exe);
                command
                    .arg(name)
                    .arg(steam_path.parent().ok_or("Steam directory unavailable")?)
                    .arg(&runtime)
                    .arg(&ugc_app_id)
                    .env_remove("SteamAppId")
                    .env_remove("SteamGameId");
                let (request, timed_out) =
                    run_candidate(command).map_err(|e| format!("Could not run {name}: {e}"))?;
                record(
                    &mut output,
                    started,
                    &format!(
                        "candidate_result={name} exit={:?} timed_out={timed_out} stdout={:?} stderr={:?}",
                        request.status.code(),
                        String::from_utf8_lossy(&request.stdout).trim(),
                        String::from_utf8_lossy(&request.stderr).trim()
                    ),
                )?;
                let online = client.wait_until_logged_on(Duration::from_secs(4));
                let log = fs::read(&connection_log).unwrap_or_default();
                let added = log.get(log_before as usize..).unwrap_or_default();
                for line in String::from_utf8_lossy(added).lines() {
                    if let Some(reason) = line.split("ScheduleImmediateReconnect()").nth(1) {
                        record(
                            &mut output,
                            started,
                            &format!("immediate_reconnect={}", reason.trim()),
                        )?;
                    }
                }
                record(
                    &mut output,
                    started,
                    &format!("candidate_observed={name} online={online}"),
                )?;
                if online {
                    break;
                }
            }
        } else if mode != "all-blocked" {
            let stats = client.request_user_stats();
            record(&mut output, started, &format!("stats_request={stats:?}"))?;
            if !client.wait_until_logged_on(Duration::from_secs(4)) {
                let lobby = client.request_lobby_list();
                record(&mut output, started, &format!("lobby_request={lobby:?}"))?;
            }
        }
    }
    let online_after_requests = client.wait_until_logged_on(Duration::from_secs(120));
    record(
        &mut output,
        started,
        &format!(
            "online_after_requests={online_after_requests} restored_for_ms={}",
            restored_at.elapsed().as_millis()
        ),
    )?;
    Ok(())
}
