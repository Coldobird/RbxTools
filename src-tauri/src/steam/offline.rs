//! Instant Steam offline/online switching through Steam's own client controls.
//!
//! Steam exposes its client API to its UI over a loopback DevTools endpoint when
//! started with `-cef-enable-debugging`. Two controls are used:
//! - `SteamClient.User.GoOffline()` enters Offline Mode without a restart. Steam
//!   refuses it while a game runs, and it persists, so a restart starts offline.
//! - `net_fake_state NoLAN` makes Steam drop its server connection and stop all
//!   server contact while a game keeps running. `normal` reconnects in ~1 s.
//!
//! `ClientConnectTokenBucketEnable 0` disables Steam's reconnect throttle, which
//! otherwise delays reconnects by up to ~4 minutes after repeated connections.
use super::{devtools::DevTools, executable::SteamExecutable, login::LoginChecker};
use crate::win32::Registry;
use crate::{poll::poll_until, Error};
use std::{path::PathBuf, sync::Arc, time::Duration};
use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;

const STEAM_ENDPOINT: &str = "127.0.0.1:8080";
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(15);
const REFUSAL_TIMEOUT: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(50);

type Probe<T> = Box<dyn Fn() -> T + Send + Sync>;

/// Takes one Steam installation offline and back online.
pub struct OfflineSwitch {
    executable: PathBuf,
    endpoint: String,
    logged_on: Probe<Result<bool, Error>>,
    running_app: Probe<Option<u32>>,
}

impl OfflineSwitch {
    /// The running Steam client at `steam`, confirming each change with `login`.
    pub fn for_steam(steam: &SteamExecutable, login: Arc<LoginChecker>) -> Self {
        let observed = steam.clone();
        Self {
            executable: steam.path().to_path_buf(),
            endpoint: STEAM_ENDPOINT.into(),
            logged_on: Box::new(move || login.is_online(&observed)),
            running_app: Box::new(|| {
                Registry::dword(HKEY_CURRENT_USER, r"Software\Valve\Steam", "RunningAppID")
            }),
        }
    }

    /// A switch with explicit Steam boundaries, for a stand-in Steam client.
    #[cfg(test)]
    fn with_boundaries(
        executable: &std::path::Path,
        endpoint: String,
        logged_on: Probe<Result<bool, Error>>,
        running_app: Probe<Option<u32>>,
    ) -> Self {
        Self {
            executable: executable.to_path_buf(),
            endpoint,
            logged_on,
            running_app,
        }
    }

    /// Disconnects Steam from its servers and confirms it is offline.
    /// Uses Offline Mode when no game runs; otherwise simulates network loss,
    /// which keeps the game running. `go_online` undoes either.
    pub fn go_offline(&self) -> Result<(), Error> {
        let mut client = self.connect()?;
        client.console("ClientConnectTokenBucketEnable 0")?;
        if !self.game_running(&mut client)? {
            client.eval("SteamClient.User.GoOffline(), true")?;
            // Steam saves Offline Mode at once unless it refused (a game started).
            if poll_until(REFUSAL_TIMEOUT, POLL, || self.offline_mode_saved())
                && self.wait_for_login(false)
            {
                return Ok(());
            }
        }
        client.console("net_fake_state NoLAN")?;
        if !self.wait_for_login(false) {
            let _ = client.console("net_fake_state normal");
            return Err("Steam did not go offline. Try again.".into());
        }
        Ok(())
    }

    /// Before a Steam restart: switch to Offline Mode so the new session starts offline.
    pub fn keep_offline_through_restart(&self) -> Result<(), Error> {
        let mut client = self.connect()?;
        // Works under NoLAN too; the simulated network loss ends with the process.
        client.eval("SteamClient.User.GoOffline(), true")?;
        if poll_until(REFUSAL_TIMEOUT, POLL, || self.offline_mode_saved()) {
            Ok(())
        } else {
            Err("Close the running game before restarting Steam offline.".into())
        }
    }

    /// Reconnects Steam to its servers and confirms login.
    pub fn go_online(&self) -> Result<(), Error> {
        let mut client = self.connect()?;
        client.console("ClientConnectTokenBucketEnable 0")?;
        // Both are no-ops when not in that state.
        client.console("net_fake_state normal")?;
        client.eval("SteamClient.User.GoOnline(), true")?;
        if self.wait_for_login(true) {
            return Ok(());
        }
        // Last resort: ask for an immediate connection attempt.
        client.eval("SteamClient.User.Connect().then(() => true)")?;
        if self.wait_for_login(true) {
            Ok(())
        } else {
            Err("Steam did not confirm it is back online. Try again.".into())
        }
    }

    fn connect(&self) -> Result<DevTools, Error> {
        DevTools::connect(&self.endpoint)
    }

    /// Whether a game may be running; Steam refuses Offline Mode then. The UI list
    /// can miss games after a reconnect and the registry lags after exit, so a game
    /// counts as running unless both sources agree that none is.
    fn game_running(&self, client: &mut DevTools) -> Result<bool, Error> {
        let ui = client
            .eval("window.SteamUIStore?.RunningApps?.length ?? null")?
            .as_u64();
        Ok(ui != Some(0) || (self.running_app)() != Some(0))
    }

    /// Offline Mode is persisted in loginusers.vdf, which a restart reads.
    fn offline_mode_saved(&self) -> bool {
        self.executable
            .parent()
            .map(|folder| folder.join("config").join("loginusers.vdf"))
            .and_then(|file| std::fs::read_to_string(file).ok())
            .is_some_and(|text| {
                text.lines().any(|line| {
                    let parts: Vec<&str> =
                        line.split('"').filter(|p| !p.trim().is_empty()).collect();
                    parts == ["WantsOfflineMode", "1"]
                })
            })
    }

    fn wait_for_login(&self, online: bool) -> bool {
        poll_until(CONFIRM_TIMEOUT, POLL, || (self.logged_on)() == Ok(online))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam::devtools::UNAVAILABLE;
    use serde_json::{json, Value};
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        path::Path,
        sync::{Arc, Mutex},
    };
    use std::{thread, time::Instant};

    /// What the stand-in Steam client currently believes.
    struct ClientState {
        logged_on: bool,
        offline_mode: bool,
        simulated_network_loss: bool,
        game_running: bool,
        ui_lists_game: bool,
        commands: Vec<String>,
    }

    /// A stand-in Steam client: serves the DevTools endpoint and reacts to the
    /// same client calls and console commands as the real one.
    struct FakeSteam {
        folder: tempfile::TempDir,
        endpoint: String,
        state: Arc<Mutex<ClientState>>,
    }

    impl FakeSteam {
        fn online(game_running: bool) -> Self {
            let folder = tempfile::tempdir().unwrap();
            std::fs::create_dir(folder.path().join("config")).unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = listener.local_addr().unwrap().to_string();
            let steam = Self {
                folder,
                endpoint: endpoint.clone(),
                state: Arc::new(Mutex::new(ClientState {
                    logged_on: true,
                    offline_mode: false,
                    simulated_network_loss: false,
                    game_running,
                    ui_lists_game: game_running,
                    commands: Vec::new(),
                })),
            };
            steam.save_offline_mode(false);
            let state = steam.state.clone();
            let settings = steam.login_settings();
            thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let (state, settings, endpoint) =
                        (state.clone(), settings.clone(), endpoint.clone());
                    thread::spawn(move || serve(stream, &endpoint, &state, &settings));
                }
            });
            steam
        }

        /// Steam's UI game list can be empty right after a reconnect.
        fn with_game_missing_from_ui(self) -> Self {
            self.state.lock().unwrap().ui_lists_game = false;
            self
        }

        fn executable(&self) -> PathBuf {
            self.folder.path().join("steam.exe")
        }

        fn login_settings(&self) -> PathBuf {
            self.folder.path().join("config").join("loginusers.vdf")
        }

        fn save_offline_mode(&self, offline: bool) {
            write_settings(&self.login_settings(), offline);
        }

        fn controls(&self, registry_app: Option<u32>) -> OfflineSwitch {
            let state = self.state.clone();
            OfflineSwitch::with_boundaries(
                &self.executable(),
                self.endpoint.clone(),
                Box::new(move || Ok(state.lock().unwrap().logged_on)),
                Box::new(move || registry_app),
            )
        }

        fn is_logged_on(&self) -> bool {
            self.state.lock().unwrap().logged_on
        }

        fn ran(&self, command: &str) -> bool {
            self.state
                .lock()
                .unwrap()
                .commands
                .iter()
                .any(|c| c == command)
        }
    }

    fn write_settings(file: &Path, offline: bool) {
        let value = if offline { "1" } else { "0" };
        std::fs::write(
            file,
            format!(
                "\"users\"\n{{\n\t\"1\"\n\t{{\n\t\t\"WantsOfflineMode\"\t\t\"{value}\"\n\t}}\n}}\n"
            ),
        )
        .unwrap();
    }

    fn serve(mut stream: TcpStream, endpoint: &str, state: &Mutex<ClientState>, settings: &Path) {
        let mut peek = [0u8; 2048];
        let read = stream.peek(&mut peek).unwrap_or(0);
        if !String::from_utf8_lossy(&peek[..read]).contains("Upgrade: websocket") {
            let _ = stream.read(&mut peek);
            let body = json!([{
                "title": "SharedJSContext",
                "webSocketDebuggerUrl": format!("ws://{endpoint}/devtools/page/shared"),
            }])
            .to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            return;
        }
        let Ok(mut socket) = tungstenite::accept(stream) else {
            return;
        };
        while let Ok(message) = socket.read() {
            let Ok(request) = serde_json::from_str::<Value>(message.to_text().unwrap_or("")) else {
                continue;
            };
            let expression = request["params"]["expression"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let value = evaluate(&expression, state, settings);
            let reply = json!({ "id": request["id"], "result": { "result": { "value": value } } });
            if socket
                .send(tungstenite::Message::text(reply.to_string()))
                .is_err()
            {
                return;
            }
        }
    }

    /// Steam's behavior for each call, as observed on the live client.
    fn evaluate(expression: &str, state: &Mutex<ClientState>, settings: &Path) -> Value {
        let mut steam = state.lock().unwrap();
        if let Some(command) = expression
            .strip_prefix("SteamClient.Console.ExecCommand(")
            .and_then(|rest| rest.strip_suffix("), true"))
        {
            let command: String = serde_json::from_str(command).unwrap();
            match command.as_str() {
                "net_fake_state NoLAN" => {
                    steam.simulated_network_loss = true;
                    steam.logged_on = false;
                }
                "net_fake_state normal" => {
                    steam.simulated_network_loss = false;
                    steam.logged_on = !steam.offline_mode;
                }
                _ => {}
            }
            steam.commands.push(command);
            return json!(true);
        }
        if expression.contains("RunningApps") {
            return json!(if steam.ui_lists_game { 1 } else { 0 });
        }
        if expression.contains("GoOffline()") && !steam.game_running {
            steam.offline_mode = true;
            steam.logged_on = false;
            write_settings(settings, true);
        }
        if expression.contains("GoOnline()") && steam.offline_mode {
            steam.offline_mode = false;
            steam.logged_on = !steam.simulated_network_loss;
            write_settings(settings, false);
        }
        json!(true)
    }

    #[test]
    fn going_offline_without_a_game_uses_offline_mode() {
        let steam = FakeSteam::online(false);
        steam.controls(Some(0)).go_offline().unwrap();
        assert!(!steam.is_logged_on());
        assert!(steam.ran("ClientConnectTokenBucketEnable 0"));
        assert!(!steam.ran("net_fake_state NoLAN"));
    }

    #[test]
    fn going_offline_during_a_game_simulates_network_loss() {
        let steam = FakeSteam::online(true);
        steam.controls(Some(4000)).go_offline().unwrap();
        assert!(steam.ran("net_fake_state NoLAN"));
        assert!(!steam.is_logged_on());
    }

    #[test]
    fn refused_offline_mode_falls_back_to_network_loss() {
        let steam = FakeSteam::online(true).with_game_missing_from_ui();
        let started = Instant::now();
        steam.controls(Some(0)).go_offline().unwrap();
        assert!(steam.ran("net_fake_state NoLAN"));
        assert!(!steam.is_logged_on());
        assert!(
            started.elapsed() < CONFIRM_TIMEOUT,
            "fallback must not wait for the full timeout"
        );
    }

    #[test]
    fn going_online_reconnects_from_either_pause() {
        for game_running in [false, true] {
            let steam = FakeSteam::online(game_running);
            let controls = steam.controls(Some(if game_running { 4000 } else { 0 }));
            controls.go_offline().unwrap();
            controls.go_online().unwrap();
            assert!(steam.is_logged_on(), "game running: {game_running}");
            assert!(steam.ran("net_fake_state normal"));
        }
    }

    #[test]
    fn restart_keeps_steam_in_offline_mode() {
        let steam = FakeSteam::online(false);
        let controls = steam.controls(Some(0));
        controls.go_offline().unwrap();
        controls.keep_offline_through_restart().unwrap();
        assert!(!steam.is_logged_on());
    }

    #[test]
    fn restart_with_a_game_running_is_refused() {
        let steam = FakeSteam::online(true);
        let controls = steam.controls(Some(4000));
        controls.go_offline().unwrap();
        let error = controls.keep_offline_through_restart().unwrap_err();
        assert!(
            error.message().contains("Close the running game"),
            "{error}"
        );
    }

    #[test]
    fn steam_without_debugging_reports_how_to_enable_controls() {
        let unused = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = unused.local_addr().unwrap().to_string();
        drop(unused);
        let controls = OfflineSwitch::with_boundaries(
            Path::new(r"C:\Steam\steam.exe"),
            endpoint,
            Box::new(|| Ok(true)),
            Box::new(|| Some(0)),
        );
        assert_eq!(controls.go_offline().unwrap_err().message(), UNAVAILABLE);
    }
}
