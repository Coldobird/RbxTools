use std::{
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};
use tauri::{webview::NewWindowResponse, Manager, Url, WebviewUrl, WebviewWindowBuilder};

const WINDOW: &str = "steam-privacy";
const CALLBACK: &str = "https://rbx-tools.invalid/spacewar-privacy";

fn allowed_navigation(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some(
                "store.steampowered.com"
                    | "login.steampowered.com"
                    | "steamcommunity.com"
                    | "help.steampowered.com"
            )
        )
}

fn result_message(result: &str) -> Result<String, String> {
    match result {
        "private" => Ok("Spacewar is now private. Steam confirmed the setting for your current account.".into()),
        "already-private" => Ok("Spacewar is already private. Steam confirmed the setting for your current account.".into()),
        "account" => Err("Sign in with the same account that is currently open in Steam, then try again.".into()),
        "signin" => Err("Steam's sign-in session could not be refreshed. Sign in to Steam again and retry.".into()),
        "identity" => Err("Steam's account identity could not be verified. No privacy setting was changed.".into()),
        "verify" => Err("Steam accepted the request, but Spacewar's private setting could not be confirmed. Try again.".into()),
        "service" => Err("Steam's privacy service rejected the request or returned an unexpected response. Try again when Steam is online.".into()),
        "cancelled" => Err("Spacewar privacy was cancelled before it could be confirmed.".into()),
        "timeout" => Err("Steam sign-in timed out. Try again to confirm Spacewar privacy.".into()),
        _ => Err("Could not reach Steam's privacy service. Check your connection and try again.".into()),
    }
}

pub async fn make_private(app: tauri::AppHandle, steam_id: String) -> Result<String, String> {
    let script = include_str!("spacewar_privacy.js").replace(
        "__RBX_PRIVACY_CONFIG__",
        &serde_json::json!({ "steamId": steam_id, "callback": CALLBACK }).to_string(),
    );
    let (sender, receiver) = mpsc::channel();
    let sender = Arc::new(Mutex::new(Some(sender)));
    let navigation_sender = sender.clone();
    let navigation_app = app.clone();
    // A separate WebView2 profile keeps Steam's session out of the dashboard.
    // This window has no Tauri capability and cannot call application commands.
    let profile = app
        .path()
        .app_data_dir()
        .map_err(|_| "Steam sign-in storage unavailable")?
        .join("steam-privacy-session");
    let window = WebviewWindowBuilder::new(
        &app,
        WINDOW,
        WebviewUrl::External(Url::parse("https://store.steampowered.com/points/shop/").unwrap()),
    )
    .title("Steam sign-in · Make Spacewar private")
    .inner_size(960.0, 740.0)
    .data_directory(profile)
    .devtools(false)
    .initialization_script(script)
    .on_new_window(|_, _| NewWindowResponse::Deny)
    .on_navigation(move |url| {
        if url.scheme() == "https"
            && url.host_str() == Some("rbx-tools.invalid")
            && url.path() == "/spacewar-privacy"
        {
            // Accept a result only from the page where our privacy script runs.
            let from_privacy_page = navigation_app
                .get_webview_window(WINDOW)
                .and_then(|window| window.url().ok())
                .is_some_and(|current| {
                    current.origin().ascii_serialization() == "https://store.steampowered.com"
                        && current.path() == "/points/shop/"
                });
            if !from_privacy_page {
                return false;
            }
            let result = url
                .query_pairs()
                .find(|(key, _)| key == "result")
                .map(|(_, value)| value.into_owned())
                .unwrap_or_else(|| "network".into());
            if let Ok(mut sender) = navigation_sender.lock() {
                if let Some(sender) = sender.take() {
                    let _ = sender.send(result);
                }
            }
            return false;
        }
        allowed_navigation(url)
    })
    .build()
    .map_err(|_| {
        "Could not open Steam's sign-in window. Check that WebView2 is installed.".to_string()
    })?;

    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            if let Ok(mut sender) = sender.lock() {
                if let Some(sender) = sender.take() {
                    let _ = sender.send("cancelled".into());
                }
            }
        }
    });
    let code = tauri::async_runtime::spawn_blocking(move || {
        receiver
            .recv_timeout(Duration::from_secs(900))
            .unwrap_or_else(|_| "timeout".into())
    })
    .await
    .map_err(|_| "Steam privacy task could not finish".to_string())?;
    if code == "account" {
        // Only clear this isolated browser session, so the next attempt can
        // sign into the correct account without touching the Steam client.
        let _ = window.clear_all_browsing_data();
    }
    let _ = window.close();
    let result = result_message(&code);
    if result.is_ok() && crate::steam::active_steam_id().as_deref() != Some(steam_id.as_str()) {
        return Err("The account open in Steam changed. Try again for the current account.".into());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricts_sign_in_navigation_to_exact_steam_hosts() {
        for url in [
            "https://store.steampowered.com/login/",
            "https://login.steampowered.com/",
            "https://steamcommunity.com/",
        ] {
            assert!(allowed_navigation(&Url::parse(url).unwrap()));
        }
        for url in [
            "http://store.steampowered.com/",
            "https://store.steampowered.com.evil.test/",
            "https://evil.test/",
            "https://store.steampowered.com:8443/",
            "https://user@store.steampowered.com/",
            "tauri://localhost/",
        ] {
            assert!(!allowed_navigation(&Url::parse(url).unwrap()));
        }
    }

    #[test]
    fn only_verified_results_report_success() {
        assert!(result_message("private").is_ok());
        assert!(result_message("already-private").is_ok());
        for result in [
            "verify", "account", "identity", "service", "network", "", "success",
        ] {
            assert!(result_message(result).is_err());
        }
    }
}
