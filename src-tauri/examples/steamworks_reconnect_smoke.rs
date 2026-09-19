#[path = "../src/steamworks.rs"]
mod steamworks;

use std::{env, path::Path, time::Duration};

fn main() {
    let steam_path = env::args().nth(1).expect("steam.exe path");
    let client = steamworks::Client::connect(Path::new(&steam_path)).expect("Steamworks context");
    let online_before = client.is_logged_on();
    client.request_user_stats().expect("RequestUserStats");
    let online_after = client.wait_until_logged_on(Duration::from_secs(2));
    println!("online_before={online_before};online_after={online_after};request_user_stats=ok");
}
