use anyhow::Result;
use vimify::{port, reference, scan, ui};

fn main() -> Result<()> {
    let reference = reference::load()?;
    let ports = port::embedded()?;
    let extra = reference.scan.extra_paths.iter().map(String::as_str);
    let dirs = scan::resolve_dirs(scan::DEFAULT_DIRS.iter().copied().chain(extra));
    let Some(selection) = ui::run(dirs, ports, &reference.exclusions)? else {
        return Ok(());
    };
    // TODO: replace with the confirm screen.
    println!("selected: {}", selection.selected.join(", "));
    println!("excluded: {}", selection.excluded.join(", "));
    Ok(())
}
