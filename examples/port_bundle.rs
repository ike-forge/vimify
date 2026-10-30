//! Prints every port as one file, for publishing.
use anyhow::Result;
use vimify::{port, registry};

fn main() -> Result<()> {
    print!("{}", registry::bundle(port::sources())?);
    Ok(())
}
