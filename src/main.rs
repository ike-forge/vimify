use anyhow::Result;
use vimify::{paths, port, reference, scan, state, ui};

fn main() -> Result<()> {
    let reference = reference::load()?;
    let state = state::load()?;
    let ports = port::embedded()?;
    let extra = reference.scan.extra_paths.iter().map(String::as_str);
    let dirs = scan::resolve_dirs(scan::DEFAULT_DIRS.iter().copied().chain(extra));
    let hash = reference.hash()?;
    let Some(outcome) = ui::run(dirs, ports, &reference.exclusions, &state, &hash)? else {
        return Ok(());
    };
    reference::save_exclusions(&outcome.exclusions)?;
    // TODO: execute the plan instead of printing it.
    for op in outcome.ops.iter().filter(|op| op.action.writes()) {
        let path = op.action.path().map(paths::display).unwrap_or_default();
        println!("{:8} {:12} {path}", op.action.label(), op.tool);
    }
    Ok(())
}
