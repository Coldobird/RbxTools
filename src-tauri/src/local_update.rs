use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const BUILD_SUBDIRECTORY: &str = "Work\\1-My Own\\7 - RBX\\RBxTools\\Builds";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildManifest {
    version: String,
    installer: String,
    #[serde(default)]
    notes: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    pub version: String,
    pub notes: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub shared_builds_available: bool,
    pub update: Option<AvailableUpdate>,
}

pub fn check() -> Result<UpdateCheck, String> {
    let Some(directory) = build_directory().filter(|directory| directory.is_dir()) else {
        return Ok(UpdateCheck {
            shared_builds_available: false,
            update: None,
        });
    };
    let manifest_path = directory.join("latest.json");
    if !manifest_path.is_file() {
        return Ok(UpdateCheck {
            shared_builds_available: true,
            update: None,
        });
    }
    let manifest: BuildManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|e| format!("Could not read the shared update manifest: {e}"))?,
    )
    .map_err(|e| format!("The shared update manifest is invalid: {e}"))?;
    let installer = installer_path(&directory, &manifest.installer)?;
    if !installer.is_file() {
        return Err(format!(
            "The shared installer for version {} is not available yet.",
            manifest.version
        ));
    }
    if version_is_newer(&manifest.version, env!("CARGO_PKG_VERSION")) {
        Ok(UpdateCheck {
            shared_builds_available: true,
            update: Some(AvailableUpdate {
                version: manifest.version,
                notes: manifest.notes,
            }),
        })
    } else {
        Ok(UpdateCheck {
            shared_builds_available: true,
            update: None,
        })
    }
}

pub fn install() -> Result<(), String> {
    let directory = build_directory()
        .ok_or("The shared OneDrive Builds folder is not available on this PC.")?;
    let manifest_path = directory.join("latest.json");
    let manifest: BuildManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|e| format!("Could not read the shared update manifest: {e}"))?,
    )
    .map_err(|e| format!("The shared update manifest is invalid: {e}"))?;
    if !version_is_newer(&manifest.version, env!("CARGO_PKG_VERSION")) {
        return Err("No newer shared build is available.".into());
    }
    let installer = installer_path(&directory, &manifest.installer)?;
    if !installer.is_file() {
        return Err(format!(
            "The shared installer for version {} is not available yet.",
            manifest.version
        ));
    }
    Command::new(&installer)
        .spawn()
        .map_err(|e| format!("Could not start the shared installer: {e}"))?;
    Ok(())
}

fn build_directory() -> Option<PathBuf> {
    let root = env::var_os("OneDrive")
        .or_else(|| env::var_os("OneDriveConsumer"))
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("USERPROFILE").map(|profile| PathBuf::from(profile).join("OneDrive"))
        })?;
    Some(root.join(BUILD_SUBDIRECTORY))
}

fn installer_path(directory: &Path, installer_name: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(installer_name);
    if candidate
        .file_name()
        .is_none_or(|name| name != installer_name)
        || candidate
            .extension()
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("exe"))
    {
        return Err("The shared update manifest has an unsafe installer name.".into());
    }
    Ok(directory.join(candidate))
}

fn version_is_newer(candidate: &str, current: &str) -> bool {
    let parse = |version: &str| {
        version
            .trim_start_matches('v')
            .split('.')
            .map(|part| part.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()
    };
    match (parse(candidate), parse(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::version_is_newer;

    #[test]
    fn compares_release_versions() {
        assert!(version_is_newer("0.2.3", "0.2.2"));
        assert!(version_is_newer("v1.0.0", "0.9.9"));
        assert!(!version_is_newer("0.2.2", "0.2.2"));
        assert!(!version_is_newer("0.2.1", "0.2.2"));
    }
}
