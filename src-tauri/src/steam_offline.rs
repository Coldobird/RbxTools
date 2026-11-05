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
use serde_json::{json, Value};
use std::{
    net::TcpStream,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const STEAM_ENDPOINT: &str = "127.0.0.1:8080";
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(15);
const REFUSAL_TIMEOUT: Duration = Duration::from_secs(3);
const UNAVAILABLE: &str = "Steam's client controls are unavailable. Restart Steam from RBX Tools once so it starts with fast offline controls.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pause {
    /// Steam Offline Mode; survives a Steam restart.
    OfflineMode,
    /// Simulated network loss; used while a game runs, lost on Steam restart.
    NoNetwork,
}

type Probe<T> = Box<dyn Fn() -> T + Send + Sync>;

/// One Steam installation's offline/online controls.
pub struct SteamControls {
    executable: PathBuf,
    endpoint: String,
    logged_on: Probe<Result<bool, String>>,
    running_app: Probe<Option<u32>>,
}

impl SteamControls {
    /// The running Steam client for `executable`.
    pub fn local(executable: &Path) -> Self {
        let observed = executable.to_path_buf();
        Self {
            executable: executable.to_path_buf(),
            endpoint: STEAM_ENDPOINT.into(),
            logged_on: Box::new(move || crate::steam_connection::online(&observed)),
            running_app: Box::new(registry_running_app),
        }
    }

    /// Controls with explicit Steam boundaries, for a stand-in Steam client.
    #[cfg(test)]
    fn with_boundaries(
        executable: &Path,
        endpoint: String,
        logged_on: Probe<Result<bool, String>>,
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
    pub fn go_offline(&self) -> Result<Pause, String> {
        let mut client = self.connect()?;
        client.console("ClientConnectTokenBucketEnable 0")?;
        if !self.game_running(&mut client)? {
            client.eval("SteamClient.User.GoOffline(), true")?;
            // Steam saves Offline Mode at once unless it refused (a game started).
            if wait_until(REFUSAL_TIMEOUT, || self.offline_mode_saved())
                && self.wait_for_login(false)
            {
                return Ok(Pause::OfflineMode);
            }
        }
        client.console("net_fake_state NoLAN")?;
        if !self.wait_for_login(false) {
            let _ = client.console("net_fake_state normal");
            return Err("Steam did not go offline. Try again.".into());
        }
        Ok(Pause::NoNetwork)
    }

    /// Before a Steam restart: switch to Offline Mode so the new session starts offline.
    pub fn keep_offline_through_restart(&self) -> Result<(), String> {
        let mut client = self.connect()?;
        // Works under NoLAN too; the simulated network loss ends with the process.
        client.eval("SteamClient.User.GoOffline(), true")?;
        if wait_until(REFUSAL_TIMEOUT, || self.offline_mode_saved()) {
            Ok(())
        } else {
            Err("Close the running game before restarting Steam offline.".into())
        }
    }

    /// Reconnects Steam to its servers and confirms login.
    pub fn go_online(&self) -> Result<(), String> {
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

    fn connect(&self) -> Result<DevTools, String> {
        DevTools::connect(&self.endpoint)
    }

    /// Whether a game may be running; Steam refuses Offline Mode then. The UI list
    /// can miss games after a reconnect and the registry lags after exit, so a game
    /// counts as running unless both sources agree that none is.
    fn game_running(&self, client: &mut DevTools) -> Result<bool, String> {
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
        wait_until(CONFIRM_TIMEOUT, || (self.logged_on)() == Ok(online))
    }
}

/// A DevTools session in Steam's shared UI context.
struct DevTools {
    socket: tungstenite::WebSocket<TcpStream>,
    next_id: u64,
}

impl DevTools {
    fn connect(endpoint: &str) -> Result<Self, String> {
        let pages: Value = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .map_err(|e| e.to_string())?
            .get(format!("http://{endpoint}/json"))
            .send()
            .and_then(|response| response.text())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .ok_or_else(|| UNAVAILABLE.to_string())?;
        let url = pages
            .as_array()
            .and_then(|pages| {
                pages
                    .iter()
                    .find(|page| page["title"] == "SharedJSContext")
                    .and_then(|page| page["webSocketDebuggerUrl"].as_str())
            })
            .ok_or("Steam is still starting. Try again in a few seconds.")?;
        // Only ever talk to the loopback endpoint that was asked for.
        if !url.starts_with(&format!("ws://{endpoint}/")) {
            return Err(UNAVAILABLE.into());
        }
        let stream = TcpStream::connect(endpoint).map_err(|_| UNAVAILABLE.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        let (socket, _) =
            tungstenite::client(url, stream).map_err(|e| format!("Steam control failed: {e}"))?;
        Ok(Self { socket, next_id: 0 })
    }

    fn eval(&mut self, expression: &str) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        let request = json!({
            "id": id,
            "method": "Runtime.evaluate",
            "params": { "expression": expression, "awaitPromise": true, "returnByValue": true },
        });
        self.socket
            .send(tungstenite::Message::text(request.to_string()))
            .map_err(|e| format!("Steam control failed: {e}"))?;
        loop {
            let message = self
                .socket
                .read()
                .map_err(|e| format!("Steam control failed: {e}"))?;
            let Ok(text) = message.to_text() else { continue };
            let Ok(reply) = serde_json::from_str::<Value>(text) else { continue };
            if reply["id"] != id {
                continue;
            }
            let result = &reply["result"];
            if let Some(details) = result.get("exceptionDetails") {
                return Err(format!("Steam control failed: {}", details["text"]));
            }
            return Ok(result["result"]["value"].clone());
        }
    }

    fn console(&mut self, command: &str) -> Result<(), String> {
        let command = serde_json::to_string(command).map_err(|e| e.to_string())?;
        self.eval(&format!("SteamClient.Console.ExecCommand({command}), true"))
            .map(|_| ())
    }
}

fn registry_running_app() -> Option<u32> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let key: Vec<u16> = r"Software\Valve\Steam".encode_utf16().chain(Some(0)).collect();
    let name: Vec<u16> = "RunningAppID".encode_utf16().chain(Some(0)).collect();
    let mut value = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    (status == 0).then_some(value)
}

fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    done()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
    };

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

        fn controls(&self, registry_app: Option<u32>) -> SteamControls {
            let state = self.state.clone();
            SteamControls::with_boundaries(
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
            self.state.lock().unwrap().commands.iter().any(|c| c == command)
        }
    }

    fn write_settings(file: &Path, offline: bool) {
        let value = if offline { "1" } else { "0" };
        std::fs::write(
            file,
            format!("\"users\"\n{{\n\t\"1\"\n\t{{\n\t\t\"WantsOfflineMode\"\t\t\"{value}\"\n\t}}\n}}\n"),
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
        let Ok(mut socket) = tungstenite::accept(stream) else { return };
        while let Ok(message) = socket.read() {
            let Ok(request) = serde_json::from_str::<Value>(message.to_text().unwrap_or("")) else {
                continue;
            };
            let expression = request["params"]["expression"].as_str().unwrap_or("").to_string();
            let value = evaluate(&expression, state, settings);
            let reply = json!({ "id": request["id"], "result": { "result": { "value": value } } });
            if socket.send(tungstenite::Message::text(reply.to_string())).is_err() {
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
        let pause = steam.controls(Some(0)).go_offline().unwrap();
        assert_eq!(pause, Pause::OfflineMode);
        assert!(!steam.is_logged_on());
        assert!(steam.ran("ClientConnectTokenBucketEnable 0"));
        assert!(!steam.ran("net_fake_state NoLAN"));
    }

    #[test]
    fn going_offline_during_a_game_simulates_network_loss() {
        let steam = FakeSteam::online(true);
        let pause = steam.controls(Some(480)).go_offline().unwrap();
        assert_eq!(pause, Pause::NoNetwork);
        assert!(!steam.is_logged_on());
    }

    #[test]
    fn refused_offline_mode_falls_back_to_network_loss() {
        let steam = FakeSteam::online(true).with_game_missing_from_ui();
        let started = Instant::now();
        let pause = steam.controls(Some(0)).go_offline().unwrap();
        assert_eq!(pause, Pause::NoNetwork);
        assert!(!steam.is_logged_on());
        assert!(started.elapsed() < CONFIRM_TIMEOUT, "fallback must not wait for the full timeout");
    }

    #[test]
    fn going_online_reconnects_from_either_pause() {
        for game_running in [false, true] {
            let steam = FakeSteam::online(game_running);
            let controls = steam.controls(Some(if game_running { 480 } else { 0 }));
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
        let controls = steam.controls(Some(480));
        controls.go_offline().unwrap();
        let error = controls.keep_offline_through_restart().unwrap_err();
        assert!(error.contains("Close the running game"), "{error}");
    }

    #[test]
    fn steam_without_debugging_reports_how_to_enable_controls() {
        let unused = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = unused.local_addr().unwrap().to_string();
        drop(unused);
        let controls = SteamControls::with_boundaries(
            Path::new(r"C:\Steam\steam.exe"),
            endpoint,
            Box::new(|| Ok(true)),
            Box::new(|| Some(0)),
        );
        assert_eq!(controls.go_offline().unwrap_err(), UNAVAILABLE);
    }
}
