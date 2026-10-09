use crate::{network::NetworkBlock, steam};
use std::path::Path;

pub fn restore_network(network: &mut Option<NetworkBlock>) -> Result<(), String> {
    if let Some(block) = network.as_mut() {
        block.close()?;
    }
    *network = None;
    Ok(())
}

#[allow(dead_code)] // Also used by the standalone live diagnostic.
pub fn restart(
    path: &Path,
    force: bool,
    network: &mut Option<NetworkBlock>,
) -> Result<String, String> {
    restore_network(network)?;
    steam::restart(path, force)
}
