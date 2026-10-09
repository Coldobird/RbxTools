#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = rbx_tools_lib::run_status_helper() {
        std::process::exit(code);
    }
    if let Some(code) = rbx_tools_lib::run_reconnect_helper() {
        std::process::exit(code);
    }
    if let Some(code) = rbx_tools_lib::run_update_helper() {
        std::process::exit(code);
    }
    rbx_tools_lib::run();
}
