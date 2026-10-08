use crate::{network::NetworkBlock, steam};
use std::path::Path;

pub fn restart(
    path: &Path,
    force: bool,
    network: &mut Option<NetworkBlock>,
) -> Result<String, String> {
    if let Some(block) = network.as_mut() {
        block.close()?;
    }
    *network = None;
    steam::restart(path, force)
}
