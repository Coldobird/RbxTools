#[allow(dead_code)]
#[path = "../src/steamworks.rs"]
mod steamworks;

use std::{env, path::Path, time::Duration};

fn main() {
    let steam_path = env::args().nth(1).expect("steam.exe path");
    let client = steamworks::Client::connect(Path::new(&steam_path), None)
        .expect("Steam client pipe and reconnect interfaces");
    let online_before = client.is_logged_on();
    let stats_result = client.request_user_stats();
    let online_after = client.wait_until_logged_on(Duration::from_secs(2));
    println!("online_before={online_before};online_after={online_after};request_user_stats={stats_result:?}");
}
