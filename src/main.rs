use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use vimify::{apply, palette, paths, plan, port, reference, scan, state, ui};

/// One vim-style keymap and theme for your CLI and TUI tools.
#[derive(Parser)]
#[command(version, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Edit the theme and keys in ~/.config/vimify/config.toml
    Config,
    /// Scan for supported tools and apply the config to the selected ones
    Apply {
        /// No prompts: apply to every detected tool not in `exclusions`. Fails if
        /// the config is missing, invalid, or has no `exclusions` (`[]` for none)
        #[arg(short, long)]
        yes: bool,
    },
    /// Take vimify's keys and theme back out of applied tools
    Rollback,
}

fn main() -> Result<()> {
    let command = Cli::parse().command;
    let ports = port::embedded()?;
    let reference = match command {
        Command::Apply { yes: true } => {
            let known: Vec<&str> = ports.iter().map(|p| p.name.as_str()).collect();
            reference::load_saved(&known)?
        }
        _ => reference::load()?,
    };
    palette::load(&reference.theme)?;
    match command {
        Command::Config => config(&reference),
        Command::Apply { yes: false } => apply(&reference, ports),
        Command::Apply { yes: true } => apply_saved(&reference, &ports),
        Command::Rollback => rollback(&reference),
    }
}

fn config(reference: &reference::Reference) -> Result<()> {
    let Some(edited) = ui::config(reference)? else {
        return Ok(());
    };
    if edited.hash()? == reference.hash()? {
        println!("No changes.");
        return Ok(());
    }
    reference::save_settings(reference, &edited)?;
    println!("Saved. Run `vimify apply` to update your tools.");
    Ok(())
}

fn scan_dirs(reference: &reference::Reference) -> Vec<std::path::PathBuf> {
    let extra = reference.scan.extra_paths.iter().map(String::as_str);
    scan::resolve_dirs(scan::DEFAULT_DIRS.iter().copied().chain(extra))
}

fn apply(reference: &reference::Reference, ports: Vec<port::Port>) -> Result<()> {
    let mut state = state::load()?;
    let Some(outcome) = ui::apply(scan_dirs(reference), ports, reference, &state)? else {
        return Ok(());
    };
    reference::save_exclusions(&outcome.exclusions)?;
    execute(reference, &outcome.ops, &outcome.ports, &mut state)
}

/// What an interactive apply does when the user confirms the preselection. Exclusions
/// are left as saved.
fn apply_saved(reference: &reference::Reference, ports: &[port::Port]) -> Result<()> {
    let mut state = state::load()?;
    let found = scan::detect(&scan_dirs(reference), ports);
    let (selection, ports) = plan::preselect(&found, ports, &reference.exclusions);
    let ops = plan::plan(&selection, &ports, &state, &reference.hash()?);
    if !ops.iter().any(|op| op.action.writes()) {
        println!("Nothing to apply.");
        return Ok(());
    }
    execute(reference, &ops, &ports, &mut state)
}

fn rollback(reference: &reference::Reference) -> Result<()> {
    let mut state = state::load()?;
    if state.tools.is_empty() {
        println!("Nothing to roll back: no tool is applied.");
        return Ok(());
    }
    let Some(outcome) = ui::rollback(&state, &reference.exclusions)? else {
        return Ok(());
    };
    reference::save_exclusions(&outcome.exclusions)?;
    execute(reference, &outcome.ops, &outcome.ports, &mut state)
}

fn execute(
    reference: &reference::Reference,
    ops: &[plan::Op],
    ports: &[port::Port],
    state: &mut state::State,
) -> Result<()> {
    let palette = palette::load(&reference.theme)?;
    let hash = reference.hash()?;
    let cx = apply::Context {
        ports,
        reference,
        palette: &palette,
        reference_hash: &hash,
    };
    let reports = apply::execute(ops, &cx, state, &mut state::save);
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
        if report.op.action.manual() {
            eprintln!(
                "warning: overrode keys edited by hand in {}",
                report.op.tool
            );
        }
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
