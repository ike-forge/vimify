use anyhow::Result;
use vimify::{port, reference, scan, ui};

fn main() -> Result<()> {
    let reference = reference::load()?;
    let ports = port::embedded()?;
    let extra = reference.scan.extra_paths.iter().map(String::as_str);
    let dirs = scan::resolve_dirs(scan::DEFAULT_DIRS.iter().copied().chain(extra));
    ui::run(dirs, ports)
}
