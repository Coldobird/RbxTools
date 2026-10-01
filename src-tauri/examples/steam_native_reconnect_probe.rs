//! AppID-free observer and temporary WFP trial runner. Never initializes a game.
#[allow(dead_code)]
#[path = "../src/network.rs"]
mod network;

use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    env,
    ffi::{c_char, c_void},
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::LibraryLoader::{
        GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    },
};

type CreateInterface = unsafe extern "C" fn(*const c_char, *mut i32) -> *mut c_void;
type CreatePipe = unsafe extern "system" fn(*mut c_void) -> i32;
type ConnectUser = unsafe extern "system" fn(*mut c_void, i32) -> i32;
type GetUser = unsafe extern "system" fn(*mut c_void, i32, i32, *const c_char) -> *mut c_void;
type ReleaseUser = unsafe extern "system" fn(*mut c_void, i32, i32);
type ReleasePipe = unsafe extern "system" fn(*mut c_void, i32) -> u8;
type LoggedOn = unsafe extern "system" fn(*mut c_void) -> u8;

struct Observer {
    module: HMODULE,
    client: *mut c_void,
    pipe: i32,
    user: i32,
    interface: *mut c_void,
}

unsafe fn slot<T: Copy>(interface: *mut c_void, index: usize) -> T {
    let table = *(interface as *const *const *const c_void);
    std::mem::transmute_copy(&*table.add(index))
}

impl Observer {
    fn connect(steam: &Path) -> Result<Self, String> {
        use std::os::windows::ffi::OsStrExt;
        let dll = steam
            .parent()
            .ok_or("Steam directory missing")?
            .join("steamclient64.dll");
        let wide: Vec<u16> = dll.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            let module = LoadLibraryExW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            );
            if module.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let Some(address) = GetProcAddress(module, b"CreateInterface\0".as_ptr()) else {
                FreeLibrary(module);
                return Err("CreateInterface missing".into());
            };
            let create: CreateInterface = std::mem::transmute(address);
            let client = create(b"SteamClient020\0".as_ptr().cast(), std::ptr::null_mut());
            if client.is_null() {
                FreeLibrary(module);
                return Err("SteamClient020 missing".into());
            }
            let create_pipe: CreatePipe = slot(client, 0);
            let pipe = create_pipe(client);
            if pipe == 0 {
                FreeLibrary(module);
                return Err("Steam pipe unavailable".into());
            }
            let connect: ConnectUser = slot(client, 2);
            let user = connect(client, pipe);
            let get_user: GetUser = slot(client, 5);
            let interface = if user != 0 {
                get_user(client, user, pipe, b"SteamUser023\0".as_ptr().cast())
            } else {
                std::ptr::null_mut()
            };
            let observer = Self {
                module,
                client,
                pipe,
                user,
                interface,
            };
            if interface.is_null() {
                return Err("Existing Steam user unavailable".into());
            }
            Ok(observer)
        }
    }
    fn online(&self) -> bool {
        unsafe {
            let logged_on: LoggedOn = slot(self.interface, 1);
            logged_on(self.interface) != 0
        }
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        unsafe {
            if self.user != 0 {
                let release: ReleaseUser = slot(self.client, 4);
                release(self.client, self.pipe, self.user);
            }
            let release: ReleasePipe = slot(self.client, 1);
            release(self.client, self.pipe);
            FreeLibrary(self.module);
        }
    }
}

#[derive(Deserialize)]
struct Trial {
    id: String,
    candidate: String,
    block_seconds: u64,
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

struct Recorder {
    output: File,
    started: Instant,
}
impl Recorder {
    fn event(&mut self, id: &str, event: &str, details: Value) -> Result<(), String> {
        writeln!(self.output, "{}", json!({"id":id,"event":event,"elapsedMs":self.started.elapsed().as_millis(),"utcMs":unix_ms(),"details":details})).map_err(|e|e.to_string())?;
        self.output.flush().map_err(|e| e.to_string())
    }
}

fn check_stop(control: &Path) -> Result<(), String> {
    if control.join("stop").exists() {
        Err("Trial cancelled by coordinator; dynamic block is released on exit".into())
    } else {
        Ok(())
    }
}

fn run_trial(
    steam: &Path,
    observer: &Observer,
    control: &Path,
    recorder: &mut Recorder,
    trial: &Trial,
) -> Result<(), String> {
    check_stop(control)?;
    let online_deadline = Instant::now() + Duration::from_secs(90);
    while !observer.online() && Instant::now() < online_deadline {
        check_stop(control)?;
        thread::sleep(Duration::from_millis(250));
    }
    if !observer.online() {
        return Err("Steam did not recover before next trial".into());
    }
    recorder.event(
        &trial.id,
        "trial_start",
        json!({"candidate":trial.candidate,"blockSeconds":trial.block_seconds,"online":true}),
    )?;
    let mut block = network::NetworkBlock::start(steam)?;
    let blocked_at = Instant::now();
    recorder.event(&trial.id, "blocked", json!({}))?;
    let mut offline_seen = false;
    let mut next_sample = Duration::ZERO;
    while blocked_at.elapsed() < Duration::from_secs(trial.block_seconds) {
        check_stop(control)?;
        let online = observer.online();
        if !online && !offline_seen {
            offline_seen = true;
            recorder.event(
                &trial.id,
                "offline",
                json!({"blockedMs":blocked_at.elapsed().as_millis()}),
            )?;
        }
        if blocked_at.elapsed() >= next_sample {
            recorder.event(
                &trial.id,
                "blocked_sample",
                json!({"online":online,"blockedMs":blocked_at.elapsed().as_millis()}),
            )?;
            next_sample += Duration::from_secs(30);
        }
        thread::sleep(Duration::from_millis(250));
    }
    let offline_at_restore = !observer.online();
    block.close()?;
    let restored_at = Instant::now();
    recorder.event(&trial.id,"restored",json!({"actualBlockedMs":blocked_at.elapsed().as_millis(),"offlineSeen":offline_seen,"offlineAtRestore":offline_at_restore}))?;
    if !offline_seen || !offline_at_restore {
        return Err("No valid offline baseline at restore".into());
    }
    let mut first_online_utc = None;
    while restored_at.elapsed() < Duration::from_secs(3) {
        check_stop(control)?;
        if observer.online() && first_online_utc.is_none() {
            first_online_utc = Some(unix_ms());
            recorder.event(
                &trial.id,
                "natural_recovery_before_candidate",
                json!({"restoredMs":restored_at.elapsed().as_millis()}),
            )?;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if first_online_utc.is_none() {
        recorder.event(
            &trial.id,
            "candidate_ready",
            json!({"candidate":trial.candidate,"restoredMs":restored_at.elapsed().as_millis()}),
        )?;
        fs::write(
            control.join("ready.json"),
            json!({"id":trial.id,"candidate":trial.candidate,"utcMs":unix_ms()}).to_string(),
        )
        .map_err(|e| e.to_string())?;
    }
    let observe_deadline = Instant::now() + Duration::from_secs(90);
    let mut next_sample = Duration::from_secs(5);
    let mut stable_since = None;
    let mut ack_recorded = false;
    while Instant::now() < observe_deadline {
        check_stop(control)?;
        if !ack_recorded {
            if let Ok(text) = fs::read_to_string(control.join("ack.json")) {
                if let Ok(value) = serde_json::from_str::<Value>(&text) {
                    if value["id"].as_str() == Some(&trial.id) {
                        recorder.event(&trial.id, "candidate_ack", value)?;
                        ack_recorded = true;
                    }
                }
            }
        }
        let online = observer.online();
        if online {
            if first_online_utc.is_none() {
                first_online_utc = Some(unix_ms());
                recorder.event(
                    &trial.id,
                    "online",
                    json!({"restoredMs":restored_at.elapsed().as_millis()}),
                )?;
            }
            let stable = stable_since.get_or_insert_with(Instant::now);
            if stable.elapsed() >= Duration::from_secs(10) {
                break;
            }
        } else {
            stable_since = None;
        }
        if restored_at.elapsed() >= next_sample {
            recorder.event(
                &trial.id,
                "recovery_sample",
                json!({"online":online,"restoredMs":restored_at.elapsed().as_millis()}),
            )?;
            next_sample += Duration::from_secs(5);
        }
        thread::sleep(Duration::from_millis(100));
    }
    recorder.event(&trial.id,"trial_complete",json!({"online":observer.online(),"firstOnlineUtcMs":first_online_utc,"ackRecorded":ack_recorded,"filtersClosed":true}))?;
    thread::sleep(Duration::from_secs(10));
    Ok(())
}

fn main() -> Result<(), String> {
    if env::var_os("SteamAppId").is_some() || env::var_os("SteamGameId").is_some() {
        return Err("Refusing inherited game AppID environment".into());
    }
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 4 {
        return Err("Usage: observe <steam.exe> <events.jsonl> <seconds> OR suite <steam.exe> <events.jsonl> <control-dir> <trials.json>".into());
    }
    let steam = PathBuf::from(&args[1]);
    let mut recorder = Recorder {
        output: File::create(&args[2]).map_err(|e| e.to_string())?,
        started: Instant::now(),
    };
    let observer = Observer::connect(&steam)?;
    recorder.event(
        "session",
        "observer_connected",
        json!({"online":observer.online(),"gameInitialized":false}),
    )?;
    match args[0].as_str() {
        "observe" => {
            let seconds = args[3].parse::<u64>().map_err(|e| e.to_string())?;
            let deadline = Instant::now() + Duration::from_secs(seconds);
            while Instant::now() < deadline {
                recorder.event("baseline", "sample", json!({"online":observer.online()}))?;
                thread::sleep(Duration::from_secs(1));
            }
        }
        "suite" => {
            let specs = args.get(4).ok_or("Trials JSON required")?;
            let trials: Vec<Trial> =
                serde_json::from_str(&fs::read_to_string(specs).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            for trial in &trials {
                if ![
                    "control",
                    "native-reconnect",
                    "native-connect",
                    "goonline",
                    "friends-reconnect",
                ]
                .contains(&trial.candidate.as_str())
                    || trial.block_seconds == 0
                    || trial.block_seconds > 1800
                {
                    return Err("Invalid trial configuration".into());
                }
            }
            for trial in trials {
                if let Err(error) = run_trial(
                    &steam,
                    &observer,
                    Path::new(&args[3]),
                    &mut recorder,
                    &trial,
                ) {
                    recorder.event(
                        &trial.id,
                        "error",
                        json!({"message":error,"dynamicBlockReleased":true}),
                    )?;
                    return Err(error);
                }
            }
            fs::write(Path::new(&args[3]).join("done"), "complete").map_err(|e| e.to_string())?;
        }
        _ => return Err("Unknown mode".into()),
    }
    recorder.event("session", "complete", json!({"online":observer.online()}))?;
    Ok(())
}
