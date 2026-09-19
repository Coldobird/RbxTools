#[path = "../src/network.rs"]
mod network;

use std::{env, thread, time::Duration};

fn main() {
    let hold_seconds = env::args()
        .nth(1)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(3);
    let executable = env::current_exe().expect("test executable path");
    let _block = network::NetworkBlock::start(&executable).expect("temporary WFP test block");
    thread::sleep(Duration::from_secs(hold_seconds));
}
