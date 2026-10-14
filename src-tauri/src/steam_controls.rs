use crate::{network::NetworkBlock, steam};
use std::path::Path;

pub fn restore_network(network: &mut Option<NetworkBlock>) -> Result<(), String> {
    if let Some(block) = network.as_mut() {
        block.close()?;
    }
    *network = None;
    Ok(())
}

pub fn restart(path: &Path, force: bool, on_loading_ui: impl FnOnce()) -> Result<String, String> {
    // NetworkBlock stays owned by the app until explicit restore or exit.
    steam::restart(path, force, on_loading_ui)
}
