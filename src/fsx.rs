use std::fs;
use std::io::{self, Write};
use std::path::Path;

use tempfile::NamedTempFile;

/// Temp file + rename, so a crash never leaves a half-written config.
/// Writes through symlinks (stow-style dotfiles) and keeps the file's mode.
pub fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let mut tmp = NamedTempFile::new_in(dir)?;
    tmp.write_all(contents.as_bytes())?;
    if let Ok(meta) = fs::metadata(&path) {
        tmp.as_file().set_permissions(meta.permissions())?;
    }
    tmp.persist(&path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use tempfile::TempDir;

    #[test]
    fn creates_missing_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a/b/config.toml");
        write_atomic(&path, "x").unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "x");
    }

    #[test]
    fn writes_through_symlink() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("dotfiles-config.toml");
        let link = dir.path().join("config.toml");
        fs::write(&real, "old").unwrap();
        symlink(&real, &link).unwrap();

        write_atomic(&link, "new").unwrap();

        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(real).unwrap(), "new");
    }

    #[test]
    fn keeps_mode() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        write_atomic(&path, "new").unwrap();

        let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
    }
}
