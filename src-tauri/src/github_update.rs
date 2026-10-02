use reqwest::blocking::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

const RELEASE_API: &str = "https://api.github.com/repos/Coldobird/RbxTools/releases/latest";
const RELEASE_DOWNLOADS: &str = "https://github.com/Coldobird/RbxTools/releases/download/";
const ASSET_NAME: &str = "RBX-Tools.exe";
const MAX_DOWNLOAD: u64 = 100 * 1024 * 1024;
const HELPER_FLAG: &str = "--rbx-apply-update";
const STARTED_FLAG: &str = "--rbx-updated";
const STAGING_PREFIX: &str = ".rbx-update-";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize)]
struct Asset {
    name: String,
    state: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    pub version: String,
}

#[derive(Serialize)]
pub struct UpdateCheck {
    pub update: Option<AvailableUpdate>,
}

#[derive(Debug)]
struct Download {
    version: String,
    asset: Asset,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
struct ApplyPlan {
    target: PathBuf,
    parent_pid: u32,
    size: u64,
    sha256: String,
}

fn client() -> Result<Client, String> {
    Client::builder()
        .user_agent(concat!("RBX-Tools/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| format!("Could not connect to GitHub: {e}"))
}

fn latest(client: &Client, current: &str) -> Result<Option<Download>, String> {
    let response = client
        .get(RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2026-03-10")
        .timeout(Duration::from_secs(20))
        .send()
        .map_err(|e| format!("Could not check GitHub for updates: {e}"))?;
    if matches!(response.status().as_u16(), 403 | 429) {
        return Err("GitHub is temporarily limiting update checks. Try again later.".into());
    }
    let response = response
        .error_for_status()
        .map_err(|e| format!("Could not check GitHub for updates: {e}"))?;
    let mut bytes = Vec::new();
    response
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Could not read the GitHub release: {e}"))?;
    if bytes.len() > 1024 * 1024 {
        return Err("The GitHub release response is too large.".into());
    }
    let release: Release = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Could not read the GitHub release: {e}"))?;
    select_release(release, current)
}

fn select_release(release: Release, current: &str) -> Result<Option<Download>, String> {
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = release
        .tag_name
        .strip_prefix('v')
        .and_then(|value| Version::parse(value).ok())
        .filter(|version| version.pre.is_empty() && version.build.is_empty())
        .ok_or("GitHub's release tag is not a stable application version.")?;
    let current = Version::parse(current).map_err(|_| "The application version is invalid.")?;
    if version <= current {
        return Ok(None);
    }
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == ASSET_NAME && asset.state == "uploaded")
        .ok_or("The latest release's Windows download is not ready yet. Try again shortly.")?;
    if asset.size == 0 || asset.size > MAX_DOWNLOAD {
        return Err("The GitHub update has an invalid download size.".into());
    }
    let expected_url = format!("{RELEASE_DOWNLOADS}{}/{ASSET_NAME}", release.tag_name);
    if asset.browser_download_url != expected_url {
        return Err("The update download is not from this application's GitHub release.".into());
    }
    let sha256 = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|digest| valid_hash(digest))
        .ok_or("GitHub has not supplied a SHA-256 checksum for this update.")?
        .to_ascii_lowercase();
    Ok(Some(Download {
        version: version.to_string(),
        asset,
        sha256,
    }))
}

pub fn check() -> Result<UpdateCheck, String> {
    let update = latest(&client()?, env!("CARGO_PKG_VERSION"))?.map(|download| AvailableUpdate {
        version: download.version,
    });
    Ok(UpdateCheck { update })
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn verified_copy(
    mut input: impl Read,
    mut output: impl Write,
    size: u64,
    hash: &str,
) -> Result<(), String> {
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|e| format!("Could not read the update: {e}"))?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > size || total > MAX_DOWNLOAD {
            return Err("The update download is larger than GitHub reported.".into());
        }
        digest.update(&buffer[..count]);
        output
            .write_all(&buffer[..count])
            .map_err(|e| format!("Could not save the update: {e}"))?;
    }
    if total != size || format!("{:x}", digest.finalize()) != hash {
        return Err(
            "The update download failed its SHA-256 verification. Please try again.".into(),
        );
    }
    Ok(())
}

pub fn install() -> Result<(), String> {
    let client = client()?;
    let download =
        latest(&client, env!("CARGO_PKG_VERSION"))?.ok_or("RBX Tools is already up to date.")?;
    let target = fs::canonicalize(env::current_exe().map_err(|e| e.to_string())?)
        .map_err(|e| format!("Could not locate this copy of RBX Tools: {e}"))?;
    let directory = tempfile::Builder::new()
        .prefix(STAGING_PREFIX)
        .tempdir_in(
            target
                .parent()
                .ok_or("The application's folder is unavailable.")?,
        )
        .map_err(|e| format!("Could not save an update in the application's folder: {e}"))?;
    let staged = directory.path().join("new.exe");
    let response = client
        .get(&download.asset.browser_download_url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|e| format!("Could not download the GitHub update: {e}"))?;
    let mut file = fs::File::create(&staged).map_err(|e| e.to_string())?;
    verified_copy(response, &mut file, download.asset.size, &download.sha256)?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    verify_executable(&staged)?;
    let helper = directory.path().join("helper.exe");
    fs::copy(&target, &helper).map_err(|e| format!("Could not prepare the update helper: {e}"))?;
    let plan = ApplyPlan {
        target,
        parent_pid: std::process::id(),
        size: download.asset.size,
        sha256: download.sha256,
    };
    let plan_path = directory.path().join("plan.json");
    fs::write(
        &plan_path,
        serde_json::to_vec(&plan).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut command = Command::new(helper);
    command.arg(HELPER_FLAG).arg(&plan_path);
    hide_console(&mut command);
    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not start the update helper: {e}"))?;
    let started = Instant::now();
    while !directory.path().join("ready").is_file() {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            let error = fs::read_to_string(directory.path().join("error.txt"))
                .unwrap_or_else(|_| "The update helper could not start.".into());
            return Err(error);
        }
        if started.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("The update helper did not become ready. Please try again.".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
    // The helper now owns these files. The caller exits only after this handshake.
    let _ = directory.keep();
    Ok(())
}

fn verify_executable(path: &Path) -> Result<(), String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut header = [0u8; 64];
    file.read_exact(&mut header)
        .map_err(|_| "The update is not a Windows executable.")?;
    if &header[..2] != b"MZ" {
        return Err("The update is not a Windows executable.".into());
    }
    let offset = u32::from_le_bytes(header[60..64].try_into().unwrap()) as u64;
    if offset > 1024 * 1024 {
        return Err("The update has an invalid executable header.".into());
    }
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut pe = [0u8; 6];
    file.read_exact(&mut pe)
        .map_err(|_| "The update has an invalid executable header.")?;
    if &pe[..4] != b"PE\0\0" || u16::from_le_bytes([pe[4], pe[5]]) != 0x8664 {
        return Err("The update is not a Windows x64 executable.".into());
    }
    Ok(())
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_: &mut Command) {}

fn read_plan(directory: &Path) -> Result<ApplyPlan, String> {
    let plan: ApplyPlan =
        serde_json::from_slice(&fs::read(directory.join("plan.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let directory = fs::canonicalize(directory).map_err(|e| e.to_string())?;
    let target = fs::canonicalize(&plan.target).map_err(|e| e.to_string())?;
    if directory.parent() != target.parent()
        || !directory
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(STAGING_PREFIX))
        || plan.target != target
        || !valid_hash(&plan.sha256)
        || plan.size == 0
        || plan.size > MAX_DOWNLOAD
    {
        return Err("The update helper received an invalid update location.".into());
    }
    Ok(plan)
}

#[cfg(windows)]
fn wait_for_parent(plan: &ApplyPlan, directory: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        },
    };
    if plan.parent_pid == std::process::id() {
        return Err("The update helper cannot replace itself.".into());
    }
    unsafe {
        let handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            plan.parent_pid,
        );
        if handle.is_null() {
            return Err("Could not open the application process for updating.".into());
        }
        let result = (|| {
            let mut path = vec![0u16; 32768];
            let mut length = path.len() as u32;
            if QueryFullProcessImageNameW(handle, 0, path.as_mut_ptr(), &mut length) == 0 {
                return Err("Could not verify the application process for updating.".into());
            }
            let process_path =
                PathBuf::from(std::ffi::OsString::from_wide(&path[..length as usize]));
            if fs::canonicalize(process_path).map_err(|e| e.to_string())? != plan.target {
                return Err("The update target does not match the running application.".into());
            }
            fs::write(directory.join("ready"), b"").map_err(|e| e.to_string())?;
            if WaitForSingleObject(handle, 60_000) != WAIT_OBJECT_0 {
                return Err("RBX Tools did not close in time to apply its update.".into());
            }
            fs::write(directory.join("exited"), b"").map_err(|e| e.to_string())?;
            Ok(())
        })();
        CloseHandle(handle);
        result
    }
}

#[cfg(not(windows))]
fn wait_for_parent(_: &ApplyPlan, _: &Path) -> Result<(), String> {
    Err("Portable updates are supported on Windows only.".into())
}

fn rename_retry(from: &Path, to: &Path) -> Result<(), String> {
    let started = Instant::now();
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error)
                if started.elapsed() >= Duration::from_secs(5)
                    || !matches!(error.raw_os_error(), Some(5 | 32 | 33)) =>
            {
                return Err(error.to_string())
            }
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn replace_and_restart(
    target: &Path,
    staged: &Path,
    backup: &Path,
    restart: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    rename_retry(target, backup).map_err(|e| format!("Could not replace the application: {e}"))?;
    let result = rename_retry(staged, target).and_then(|_| restart(target));
    if let Err(error) = result {
        if target.exists() {
            rename_retry(target, staged).map_err(|e| format!("Update failed ({error}); rollback could not move the new file: {e}. The previous executable is at {}.", backup.display()))?;
        }
        rename_retry(backup, target).map_err(|e| {
            format!(
                "Update failed ({error}); rollback failed: {e}. The previous executable is at {}.",
                backup.display()
            )
        })?;
        return Err(format!(
            "Update failed; the previous version was restored. {error}"
        ));
    }
    Ok(())
}

fn restart_and_confirm(target: &Path, directory: &Path) -> Result<(), String> {
    let mut child = Command::new(target)
        .arg(STARTED_FLAG)
        .arg(directory)
        .spawn()
        .map_err(|e| format!("Could not restart RBX Tools: {e}"))?;
    let started = Instant::now();
    while !directory.join("started").is_file() {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!(
                "The updated application closed during startup ({status})."
            ));
        }
        if started.elapsed() >= Duration::from_secs(30) {
            child.kill().map_err(|e| {
                format!(
                    "The updated application did not finish starting and could not be stopped: {e}"
                )
            })?;
            child.wait().map_err(|e| e.to_string())?;
            return Err("The updated application did not finish starting.".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = fs::write(directory.join("done"), b"");
    Ok(())
}

pub fn run_helper_from_args() -> Option<i32> {
    let mut args = env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(HELPER_FLAG)) {
        return None;
    }
    let result = (|| {
        let plan_path = PathBuf::from(args.next().ok_or("The update plan is missing.")?);
        if args.next().is_some() || plan_path.file_name() != Some(std::ffi::OsStr::new("plan.json"))
        {
            return Err("The update helper arguments are invalid.".to_string());
        }
        let directory =
            fs::canonicalize(plan_path.parent().ok_or("The update folder is missing.")?)
                .map_err(|e| e.to_string())?;
        let helper = fs::canonicalize(env::current_exe().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if helper != directory.join("helper.exe") {
            return Err("The update helper is outside its update folder.".into());
        }
        let plan = read_plan(&directory)?;
        let result = (|| {
            let staged = directory.join("new.exe");
            verified_copy(
                fs::File::open(&staged).map_err(|e| e.to_string())?,
                std::io::sink(),
                plan.size,
                &plan.sha256,
            )?;
            verify_executable(&staged)?;
            wait_for_parent(&plan, &directory)?;
            replace_and_restart(
                &plan.target,
                &staged,
                &directory.join("previous.exe"),
                |target| restart_and_confirm(target, &directory),
            )
        })();
        if let Err(error) = &result {
            let _ = fs::write(directory.join("error.txt"), error);
            if directory.join("exited").is_file() {
                show_update_error(error);
                if !directory.join("previous.exe").exists() {
                    let _ = Command::new(&plan.target).spawn();
                }
            }
        }
        result
    })();
    Some(if result.is_ok() { 0 } else { 1 })
}

#[cfg(windows)]
fn show_update_error(error: &str) {
    use std::os::windows::ffi::OsStrExt;
    let message: Vec<u16> = std::ffi::OsStr::new(error)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let title: Vec<u16> = "RBX Tools update".encode_utf16().chain(Some(0)).collect();
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            0,
        );
    }
}

#[cfg(not(windows))]
fn show_update_error(_: &str) {}

pub fn complete_startup() {
    let mut args = env::args_os().skip(1);
    if args.next().as_deref() == Some(std::ffi::OsStr::new(STARTED_FLAG)) {
        if let Some(directory) = args.next().map(PathBuf::from) {
            if let Ok(plan) = read_plan(&directory) {
                if env::current_exe()
                    .ok()
                    .and_then(|path| fs::canonicalize(path).ok())
                    == Some(plan.target)
                {
                    let _ = fs::write(directory.join("started"), b"");
                }
            }
        }
    }
    // Give the helper time to acknowledge startup and release its executable.
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(3));
        cleanup_completed();
    });
}

fn cleanup_completed() {
    let Ok(target) = env::current_exe().and_then(fs::canonicalize) else {
        return;
    };
    let Some(parent) = target.parent() else {
        return;
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let directory = entry.path();
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(STAGING_PREFIX)
            || !entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
            || !directory.join("done").is_file()
            || !read_plan(&directory).is_ok_and(|plan| plan.target == target)
        {
            continue;
        }
        // Only known files in a verified sibling staging folder; never recurse.
        let mut removed = true;
        for name in [
            "helper.exe",
            "new.exe",
            "previous.exe",
            "ready",
            "exited",
            "started",
            "error.txt",
        ] {
            if fs::remove_file(directory.join(name)).is_err() && directory.join(name).exists() {
                removed = false;
                break;
            }
        }
        if !removed {
            continue;
        }
        let _ = fs::remove_file(directory.join("done"));
        let _ = fs::remove_file(directory.join("plan.json"));
        let _ = fs::remove_dir(directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str) -> Release {
        Release {
            tag_name: format!("v{version}"),
            draft: false,
            prerelease: false,
            assets: vec![Asset {
                name: ASSET_NAME.into(),
                state: "uploaded".into(),
                size: 128,
                digest: Some(format!("sha256:{}", "a".repeat(64))),
                browser_download_url: format!("{RELEASE_DOWNLOADS}v{version}/{ASSET_NAME}"),
            }],
        }
    }

    #[test]
    fn only_newer_stable_versions_are_offered() {
        assert_eq!(
            select_release(release("0.2.10"), "0.2.9")
                .unwrap()
                .unwrap()
                .version,
            "0.2.10"
        );
        assert!(select_release(release("0.2.14"), "0.2.14")
            .unwrap()
            .is_none());
        assert!(select_release(release("0.2.13"), "0.2.14")
            .unwrap()
            .is_none());
        let mut draft = release("0.3.0");
        draft.draft = true;
        assert!(select_release(draft, "0.2.14").unwrap().is_none());
        let mut preview = release("0.3.0-beta.1");
        preview.prerelease = true;
        assert!(select_release(preview, "0.2.14").unwrap().is_none());
        for tag in ["latest", "0.3.0", "v0.3", "v0.3.0-beta.1", "v0.3.0+build"] {
            let mut invalid = release("0.3.0");
            invalid.tag_name = tag.into();
            assert!(select_release(invalid, "0.2.14").is_err(), "{tag}");
        }
    }

    #[test]
    fn missing_or_unverified_assets_are_not_offered() {
        let mut missing = release("0.3.0");
        missing.assets.clear();
        assert!(select_release(missing, "0.2.14")
            .unwrap_err()
            .contains("not ready"));
        for url in [
            "http://github.com/Coldobird/RbxTools/releases/download/v0.3.0/RBX-Tools.exe",
            "https://github.com/another/repo/releases/download/v0.3.0/RBX-Tools.exe",
            "https://github.com.evil.example/Coldobird/RbxTools/releases/download/v0.3.0/RBX-Tools.exe",
            "https://github.com/Coldobird/RbxTools/releases/download/v0.2.14/RBX-Tools.exe",
        ] {
            let mut invalid = release("0.3.0"); invalid.assets[0].browser_download_url = url.into();
            assert!(select_release(invalid, "0.2.14").is_err());
        }
        for digest in [
            None,
            Some("sha256:wrong".into()),
            Some(format!("sha1:{}", "a".repeat(64))),
        ] {
            let mut invalid = release("0.3.0");
            invalid.assets[0].digest = digest;
            assert!(select_release(invalid, "0.2.14").is_err());
        }
        let mut oversized = release("0.3.0");
        oversized.assets[0].size = MAX_DOWNLOAD + 1;
        assert!(select_release(oversized, "0.2.14").is_err());
    }

    #[test]
    fn downloads_must_match_the_reported_size_and_checksum() {
        let contents = b"a verified executable";
        let hash = format!("{:x}", Sha256::digest(contents));
        let mut copied = Vec::new();
        verified_copy(&contents[..], &mut copied, contents.len() as u64, &hash).unwrap();
        assert_eq!(copied, contents);
        assert!(verified_copy(
            &contents[..contents.len() - 1],
            std::io::sink(),
            contents.len() as u64,
            &hash
        )
        .is_err());
        assert!(verified_copy(
            &contents[..],
            std::io::sink(),
            contents.len() as u64 - 1,
            &hash
        )
        .is_err());
        assert!(verified_copy(
            &contents[..],
            std::io::sink(),
            contents.len() as u64,
            &"0".repeat(64)
        )
        .is_err());
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("lost connection"))
            }
        }
        assert!(verified_copy(Broken, std::io::sink(), 1, &hash)
            .unwrap_err()
            .contains("lost connection"));
    }

    #[test]
    fn update_must_be_a_windows_x64_executable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("new.exe");
        fs::write(&path, b"<html>download failed</html>").unwrap();
        assert!(verify_executable(&path).is_err());
        let mut executable = vec![0u8; 128];
        executable[..2].copy_from_slice(b"MZ");
        executable[60..64].copy_from_slice(&64u32.to_le_bytes());
        executable[64..70].copy_from_slice(b"PE\0\0\x64\x86");
        fs::write(&path, &executable).unwrap();
        verify_executable(&path).unwrap();
        executable[68..70].copy_from_slice(&0x14cu16.to_le_bytes());
        fs::write(&path, &executable).unwrap();
        assert!(verify_executable(&path).is_err());
    }

    #[test]
    fn replacement_keeps_the_same_path_and_a_backup() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("RBX Tools — portable.exe");
        let staged = directory.path().join("new.exe");
        let backup = directory.path().join("previous.exe");
        fs::write(&target, b"previous").unwrap();
        fs::write(&staged, b"updated").unwrap();
        replace_and_restart(&target, &staged, &backup, |path| {
            assert_eq!(path, target);
            assert_eq!(fs::read(path).unwrap(), b"updated");
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"updated");
        assert_eq!(fs::read(&backup).unwrap(), b"previous");
    }

    #[test]
    fn failed_restart_restores_the_previous_executable() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("RBX-Tools.exe");
        let staged = directory.path().join("new.exe");
        let backup = directory.path().join("previous.exe");
        fs::write(&target, b"previous").unwrap();
        fs::write(&staged, b"updated").unwrap();
        let error =
            replace_and_restart(&target, &staged, &backup, |_| Err("startup failed".into()))
                .unwrap_err();
        assert!(error.contains("previous version was restored"));
        assert_eq!(fs::read(&target).unwrap(), b"previous");
        assert_eq!(fs::read(&staged).unwrap(), b"updated");
        assert!(!backup.exists());
    }

    #[test]
    fn failed_replacement_restores_the_previous_executable() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("RBX-Tools.exe");
        let backup = directory.path().join("previous.exe");
        fs::write(&target, b"previous").unwrap();
        assert!(replace_and_restart(
            &target,
            &directory.path().join("missing.exe"),
            &backup,
            |_| panic!("must not restart a missing update")
        )
        .is_err());
        assert_eq!(fs::read(&target).unwrap(), b"previous");
        assert!(!backup.exists());
    }

    #[test]
    fn helper_rejects_a_target_outside_its_sibling_folder() {
        let directory = tempfile::tempdir().unwrap();
        let target = fs::canonicalize(env::current_exe().unwrap()).unwrap();
        let plan = ApplyPlan {
            target,
            parent_pid: std::process::id(),
            size: 1,
            sha256: "a".repeat(64),
        };
        fs::write(
            directory.path().join("plan.json"),
            serde_json::to_vec(&plan).unwrap(),
        )
        .unwrap();
        assert!(read_plan(directory.path()).is_err());
    }

    #[cfg(windows)]
    fn fixture_command(executable: &Path, role: &str, directory: &Path) -> Command {
        let mut command = Command::new(executable);
        command
            .args([
                "--exact",
                "github_update::tests::process_fixture",
                "--nocapture",
            ])
            .env("RBX_UPDATE_PROCESS_FIXTURE", role)
            .env("RBX_UPDATE_PROCESS_DIRECTORY", directory);
        hide_console(&mut command);
        command
    }

    #[test]
    #[cfg(windows)]
    fn process_fixture() {
        let Ok(role) = env::var("RBX_UPDATE_PROCESS_FIXTURE") else {
            return;
        };
        let directory = PathBuf::from(env::var_os("RBX_UPDATE_PROCESS_DIRECTORY").unwrap());
        match role.as_str() {
            "old" => {
                fs::write(directory.join("old-started"), b"").unwrap();
                let started = Instant::now();
                while !directory.join("ready").is_file() {
                    assert!(
                        started.elapsed() < Duration::from_secs(10),
                        "helper did not become ready"
                    );
                    thread::sleep(Duration::from_millis(10));
                }
            }
            "new" => {
                let plan = read_plan(&directory).unwrap();
                assert_eq!(
                    fs::canonicalize(env::current_exe().unwrap()).unwrap(),
                    plan.target
                );
                fs::write(directory.join("started"), b"").unwrap();
            }
            "helper" => {
                let plan = read_plan(&directory).unwrap();
                let staged = directory.join("new.exe");
                verified_copy(
                    fs::File::open(&staged).unwrap(),
                    std::io::sink(),
                    plan.size,
                    &plan.sha256,
                )
                .unwrap();
                verify_executable(&staged).unwrap();
                wait_for_parent(&plan, &directory).unwrap();
                replace_and_restart(
                    &plan.target,
                    &staged,
                    &directory.join("previous.exe"),
                    |target| {
                        let output = fixture_command(target, "new", &directory)
                            .output()
                            .map_err(|e| e.to_string())?;
                        if !output.status.success() || !directory.join("started").is_file() {
                            return Err(String::from_utf8_lossy(&output.stdout).into_owned());
                        }
                        Ok(())
                    },
                )
                .unwrap();
            }
            _ => panic!("unknown process fixture role"),
        }
    }

    #[test]
    #[cfg(windows)]
    fn windows_helper_waits_for_exit_replaces_and_restarts_a_renamed_copy() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("My RBX Tools — portable.exe");
        let directory = tempfile::Builder::new()
            .prefix(STAGING_PREFIX)
            .tempdir_in(root.path())
            .unwrap();
        let executable = env::current_exe().unwrap();
        fs::copy(&executable, &target).unwrap();
        fs::copy(&executable, directory.path().join("helper.exe")).unwrap();
        let mut updated = fs::read(&executable).unwrap();
        updated.extend_from_slice(b"updater smoke test: new executable");
        fs::write(directory.path().join("new.exe"), &updated).unwrap();
        let mut parent = fixture_command(&target, "old", directory.path())
            .spawn()
            .unwrap();
        let plan = ApplyPlan {
            target: fs::canonicalize(&target).unwrap(),
            parent_pid: parent.id(),
            size: updated.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&updated)),
        };
        fs::write(
            directory.path().join("plan.json"),
            serde_json::to_vec(&plan).unwrap(),
        )
        .unwrap();
        let output = fixture_command(
            &directory.path().join("helper.exe"),
            "helper",
            directory.path(),
        )
        .output()
        .unwrap();
        assert!(parent.wait().unwrap().success());
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(&target).unwrap(), updated);
        assert!(directory.path().join("started").is_file());
        assert_eq!(
            fs::read(directory.path().join("previous.exe")).unwrap(),
            fs::read(executable).unwrap()
        );
    }

    #[test]
    #[ignore = "Reads and downloads the actual public GitHub release; never installs or launches it"]
    fn live_github_download_verifies_without_installing() {
        let client = client().unwrap();
        let download = latest(&client, "0.0.0").unwrap().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("download.exe");
        let response = client
            .get(&download.asset.browser_download_url)
            .send()
            .unwrap()
            .error_for_status()
            .unwrap();
        let mut file = fs::File::create(&path).unwrap();
        verified_copy(response, &mut file, download.asset.size, &download.sha256).unwrap();
        drop(file);
        verify_executable(&path).unwrap();
        println!(
            "Verified GitHub v{}: {} bytes, SHA-256 {}",
            download.version, download.asset.size, download.sha256
        );
    }
}
