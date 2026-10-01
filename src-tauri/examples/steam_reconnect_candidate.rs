//! Run one experimental Steam request in a disposable process without a game AppID.
#[allow(dead_code)]
#[path = "support/steamworks_noappid.rs"]
mod steamworks;

use std::{env, path::Path};

fn main() {
    let result = (|| -> Result<String, String> {
        let mut args = env::args().skip(1);
        let name = args.next().ok_or("candidate name required")?;
        let steam_dir = args.next().ok_or("Steam directory required")?;
        let runtime = args.next().ok_or("steam_api64.dll path required")?;
        let ugc_app_id = args
            .next()
            .ok_or("installed UGC app ID required")?
            .parse::<u32>()
            .map_err(|e| e.to_string())?;
        if env::var_os("SteamAppId").is_some() || env::var_os("SteamGameId").is_some() {
            return Err("Refusing to run with a Steam AppID environment variable".into());
        }
        let client = steamworks::Client::connect(
            &Path::new(&steam_dir).join("steam.exe"),
            Some(Path::new(&runtime)),
        )?;
        eprintln!("connected to Steam; online={}", client.is_logged_on());
        client.experimental_request(&name, ugc_app_id)
    })();
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
