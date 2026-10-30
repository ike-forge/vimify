use anyhow::{Result, bail};
use vimify::{apply, palette, paths, port, reference, scan, state, ui};

fn main() -> Result<()> {
    let reference = reference::load()?;
    let palette = palette::load(&reference.theme)?;
    let mut state = state::load()?;
    let ports = port::embedded()?;
    let extra = reference.scan.extra_paths.iter().map(String::as_str);
    let dirs = scan::resolve_dirs(scan::DEFAULT_DIRS.iter().copied().chain(extra));
    let hash = reference.hash()?;
    let Some(outcome) = ui::run(dirs, ports, &reference.exclusions, &state, &hash)? else {
        return Ok(());
    };
    reference::save_exclusions(&outcome.exclusions)?;
    let cx = apply::Context {
        ports: &outcome.ports,
        reference: &reference,
        palette: &palette,
        reference_hash: &hash,
    };
    let reports = apply::execute(&outcome.ops, &cx, &mut state, &mut state::save);
    let mut failed = 0;
    for report in &reports {
        let path = report
            .op
            .action
            .path()
            .map(paths::display)
            .unwrap_or_default();
        let label = report.op.action.label();
        println!("{label:8} {:12} {path}", report.op.tool);
        match &report.result {
            Ok(warnings) => warnings.iter().for_each(|w| eprintln!("warning: {w}")),
            Err(e) => {
                failed += 1;
                eprintln!("failed: {e:#}");
            }
        }
    }
    if failed > 0 {
        bail!("{failed} of {} tools failed", reports.len());
    }
    Ok(())
}
