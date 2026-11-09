use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use vimify::{apply, palette, paths, port, reference, scan, state, ui};

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
    Apply,
    /// Take vimify's keys and theme back out of applied tools
    Rollback,
}

fn main() -> Result<()> {
    let command = Cli::parse().command;
    let reference = reference::load()?;
    palette::load(&reference.theme)?;
    match command {
        Command::Config => config(&reference),
        Command::Apply => apply(&reference),
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

fn apply(reference: &reference::Reference) -> Result<()> {
    let mut state = state::load()?;
    let ports = port::embedded()?;
    let extra = reference.scan.extra_paths.iter().map(String::as_str);
    let dirs = scan::resolve_dirs(scan::DEFAULT_DIRS.iter().copied().chain(extra));
    let Some(outcome) = ui::apply(dirs, ports, reference, &state)? else {
        return Ok(());
    };
    execute(reference, &outcome, &mut state)
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
    execute(reference, &outcome, &mut state)
}

fn execute(
    reference: &reference::Reference,
    outcome: &ui::Outcome,
    state: &mut state::State,
) -> Result<()> {
    reference::save_exclusions(&outcome.exclusions)?;
    let palette = palette::load(&reference.theme)?;
    let hash = reference.hash()?;
    let cx = apply::Context {
        ports: &outcome.ports,
        reference,
        palette: &palette,
        reference_hash: &hash,
    };
    let reports = apply::execute(&outcome.ops, &cx, state, &mut state::save);
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
