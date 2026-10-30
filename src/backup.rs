use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Copies `path` to the first free of `<path>.bak`, `<path>.bak.1`, ... and returns it.
/// A symlink's target supplies the content; the backup sits next to the link, where
/// the tool reads its config, not in the dotfiles repo.
pub fn backup(path: &Path) -> Result<PathBuf> {
    let mut src = File::open(path).with_context(|| format!("{}", path.display()))?;
    let meta = src
        .metadata()
        .with_context(|| format!("{}", path.display()))?;
    if !meta.is_file() {
        bail!("{} is not a file", path.display());
    }
    let (dst_path, mut dst) = claim(path).with_context(|| format!("{}", path.display()))?;
    if let Err(e) = copy(&mut src, &mut dst, meta.permissions()) {
        // A partial copy must not pass for a backup.
        let _ = fs::remove_file(&dst_path);
        return Err(e).with_context(|| format!("{}", dst_path.display()));
    }
    Ok(dst_path)
}

/// `create_new` makes the claim atomic: a name taken by anyone, even as a dangling
/// symlink, is skipped rather than overwritten.
fn claim(path: &Path) -> io::Result<(PathBuf, File)> {
    let mut n = 0;
    loop {
        let candidate = candidate(path, n);
        // Private until the source's mode is copied: configs can hold secrets.
        let opened = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&candidate);
        match opened {
            Ok(file) => return Ok((candidate, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(e),
        }
    }
}

fn candidate(path: &Path, n: u32) -> PathBuf {
    let mut name = OsString::from(path);
    match n {
        0 => name.push(".bak"),
        n => name.push(format!(".bak.{n}")),
    }
    name.into()
}

fn copy(src: &mut File, dst: &mut File, permissions: Permissions) -> io::Result<()> {
    io::copy(src, dst)?;
    dst.set_permissions(permissions)?;
    // The caller replaces the original next, so the copy must reach disk first.
    dst.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use tempfile::TempDir;

    fn config(dir: &TempDir) -> PathBuf {
        let path = dir.path().join("config.yml");
        fs::write(&path, "original").unwrap();
        path
    }

    fn sibling(path: &Path, suffix: &str) -> PathBuf {
        let mut name = OsString::from(path);
        name.push(suffix);
        name.into()
    }

    #[test]
    fn first_backup_is_bak() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir);

        let out = backup(&path).unwrap();

        assert_eq!(out, sibling(&path, ".bak"));
        assert_eq!(fs::read_to_string(out).unwrap(), "original");
        assert_eq!(fs::read_to_string(path).unwrap(), "original");
    }

    #[test]
    fn existing_bak_is_left_untouched() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir);
        let users = sibling(&path, ".bak");
        fs::write(&users, "user's own").unwrap();

        let out = backup(&path).unwrap();

        assert_eq!(out, sibling(&path, ".bak.1"));
        assert_eq!(fs::read_to_string(users).unwrap(), "user's own");
        assert_eq!(fs::read_to_string(out).unwrap(), "original");
    }

    #[test]
    fn fills_gaps_in_numbering() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir);
        fs::write(sibling(&path, ".bak"), "").unwrap();
        fs::write(sibling(&path, ".bak.2"), "").unwrap();

        assert_eq!(backup(&path).unwrap(), sibling(&path, ".bak.1"));
    }

    #[test]
    fn dangling_symlink_counts_as_taken() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir);
        let missing = dir.path().join("missing");
        symlink(&missing, sibling(&path, ".bak")).unwrap();

        assert_eq!(backup(&path).unwrap(), sibling(&path, ".bak.1"));
        assert!(!missing.exists());
    }

    #[test]
    fn symlinked_config_is_backed_up_next_to_the_link() {
        let dir = TempDir::new().unwrap();
        let dotfiles = dir.path().join("dotfiles");
        let config_dir = dir.path().join("config");
        fs::create_dir_all(&dotfiles).unwrap();
        fs::create_dir_all(&config_dir).unwrap();
        let real = dotfiles.join("config.yml");
        let link = config_dir.join("config.yml");
        fs::write(&real, "original").unwrap();
        symlink(&real, &link).unwrap();

        let out = backup(&link).unwrap();

        assert_eq!(out, sibling(&link, ".bak"));
        assert!(out.symlink_metadata().unwrap().file_type().is_file());
        assert_eq!(fs::read_to_string(out).unwrap(), "original");
        assert!(!sibling(&real, ".bak").exists());
    }

    #[test]
    fn missing_file_is_an_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yml");

        assert!(backup(&path).is_err());
        assert!(!sibling(&path, ".bak").exists());
    }

    #[test]
    fn directory_is_an_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.d");
        fs::create_dir(&path).unwrap();

        assert!(backup(&path).is_err());
        assert!(!sibling(&path, ".bak").exists());
    }

    #[test]
    fn keeps_mode() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir);
        fs::set_permissions(&path, Permissions::from_mode(0o640)).unwrap();

        let out = backup(&path).unwrap();

        let mode = fs::metadata(out).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }
}
