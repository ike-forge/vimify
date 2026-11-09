use anyhow::{Result, bail};
use vimify::{apply, palette, paths, port, reference, scan, state, ui};

fn main() -> Result<()> {
    let reference = reference::load()?;
    palette::load(&reference.theme)?;
    let mut state = state::load()?;
    let ports = port::embedded()?;
    let extra = reference.scan.extra_paths.iter().map(String::as_str);
    let dirs = scan::resolve_dirs(scan::DEFAULT_DIRS.iter().copied().chain(extra));
    let Some(outcome) = ui::run(dirs, ports, &reference, &state)? else {
        return Ok(());
    };
    reference::save_exclusions(&outcome.exclusions)?;
    reference::save_settings(&reference, &outcome.reference)?;
    let reference = outcome.reference;
    let palette = palette::load(&reference.theme)?;
    let hash = reference.hash()?;
    let cx = apply::Context {
        ports: &outcome.ports,
        reference: &reference,
        palette: &palette,
        reference_hash: &hash,
    };
    let reports = apply::execute(&outcome.ops, &cx, &mut state, &mut state::save);
    let mut failed = 0;
    for report in &reports {
        let shown: Vec<String> = report
            .op
            .action
            .paths()
            .iter()
            .map(|path| paths::display(path))
            .collect();
        let path = shown.join(", ");
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
