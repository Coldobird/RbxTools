//! Read-only diagnostic built directly against the application's Steam module.
#[path = "../src-tauri/src/steam.rs"]
mod steam;

fn main() {
    let path = steam::detect().expect("Steam should be detected on this development machine");
    let processes = steam::matching_processes(&path).expect("Steam process lookup failed");
    println!("Steam executable: {}", path.display());
    println!("Matching Steam processes: {}", processes.len());
    println!("Diagnostic elevated: {}", steam::is_elevated());
    println!("Read-only detection passed. No process or network state changed.");
}
