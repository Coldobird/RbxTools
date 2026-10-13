#[path = "../src/spacewar_privacy.rs"]
mod spacewar_privacy;

use std::path::Path;

fn main() {
    if let Some(code) = spacewar_privacy::run_helper_from_args() {
        std::process::exit(code);
    }
    let steam = std::env::args_os().nth(1).expect("pass the Steam executable path");
    match spacewar_privacy::run(Path::new(&steam), None) {
        Ok(is_private) => println!("{is_private}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
