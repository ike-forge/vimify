# vimify

Read `vision.md`, `stack.md` and `architecture.md` before changing behaviour.

## Commands

- `mise run lint`, `mise run test`, `mise run build`, `mise run run`.
- Without mise: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`.
- `mise run sandbox` needs Docker; skip it where Docker is unavailable.

## Rules

- Never overwrite or delete a user file without a backup (`architecture.md`,
  Backups and state).
- Lint and tests must pass before a commit. Commit only when asked.

## Style

- Comments only for what the code cannot say: why, external constraints, edge
  cases, known-temporary code. One line by default. No restating code, no
  narration, no change history.
- Commit messages and docs follow the same rule: state the decision and its
  constraint.
- Match surrounding code: tests in a `#[cfg(test)] mod tests` per module,
  `anyhow` for errors, `tempfile` for filesystem tests.
