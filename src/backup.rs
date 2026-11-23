use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Returns a backup of `path`: an existing `<path>.bak*` with the same bytes and mode,
/// else a copy at the first free of `<path>.bak`, `<path>.bak.1`, ...
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
    let mut bytes = Vec::new();
    src.read_to_end(&mut bytes)
        .with_context(|| format!("{}", path.display()))?;
    if let Some(same) = find_same(path, &bytes, &meta.permissions())? {
        return Ok(same);
    }
    let (dst_path, mut dst) = claim(path).with_context(|| format!("{}", path.display()))?;
    if let Err(e) = copy(&bytes, &mut dst, meta.permissions()) {
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

/// Lowest-numbered first, so repeated runs settle on the same file. Symlinks are
/// skipped: their content can change without the name changing.
fn find_same(path: &Path, bytes: &[u8], permissions: &Permissions) -> Result<Option<PathBuf>> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Ok(None);
    };
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    let mut numbers = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("{}", dir.display()))? {
        let entry = entry.with_context(|| format!("{}", dir.display()))?;
        if let Some(n) = number(name, &entry.file_name()) {
            numbers.push(n);
        }
    }
    numbers.sort_unstable();
    for n in numbers {
        let candidate = candidate(path, n);
        let meta =
            fs::symlink_metadata(&candidate).with_context(|| format!("{}", candidate.display()))?;
        let same_mode = meta.permissions().mode() & 0o7777 == permissions.mode() & 0o7777;
        if meta.is_file() && meta.len() == bytes.len() as u64 && same_mode {
            let existing =
                fs::read(&candidate).with_context(|| format!("{}", candidate.display()))?;
            if existing == bytes {
                return Ok(Some(candidate));
            }
        }
    }
    Ok(None)
}

/// The `n` of `candidate(path, n)` for a backup name of `name`.
fn number(name: &OsStr, entry: &OsStr) -> Option<u32> {
    let rest = entry
        .as_encoded_bytes()
        .strip_prefix(name.as_encoded_bytes())?;
    let rest = rest.strip_prefix(b".bak")?;
    if rest.is_empty() {
        return Some(0);
    }
    let digits = std::str::from_utf8(rest.strip_prefix(b".")?).ok()?;
    // `.bak.01` is not a name vimify makes; `candidate` would not find it again.
    match digits.parse::<u32>() {
        Ok(n) if n > 0 && n.to_string() == digits => Some(n),
        _ => None,
    }
}

fn copy(bytes: &[u8], dst: &mut File, permissions: Permissions) -> io::Result<()> {
    dst.write_all(bytes)?;
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
    fn reuses_a_backup_with_the_same_content() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir);
        fs::write(sibling(&path, ".bak"), "other").unwrap();
        fs::write(sibling(&path, ".bak.2"), "original").unwrap();
        fs::write(sibling(&path, ".bak.5"), "original").unwrap();

        assert_eq!(backup(&path).unwrap(), sibling(&path, ".bak.2"));
        assert!(!sibling(&path, ".bak.1").exists());
    }

    #[test]
    fn same_content_with_another_mode_is_not_reused() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir);
        let other = sibling(&path, ".bak");
        fs::write(&other, "original").unwrap();
        fs::set_permissions(&other, Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(&path, Permissions::from_mode(0o644)).unwrap();

        assert_eq!(backup(&path).unwrap(), sibling(&path, ".bak.1"));
    }

    #[test]
    fn symlinked_backup_is_not_reused() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir);
        let elsewhere = dir.path().join("elsewhere");
        fs::write(&elsewhere, "original").unwrap();
        symlink(&elsewhere, sibling(&path, ".bak")).unwrap();

        assert_eq!(backup(&path).unwrap(), sibling(&path, ".bak.1"));
    }

    #[test]
    fn only_vimify_backup_names_are_reused() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir);
        for suffix in [".bak.01", ".bak.x", ".bak0", ".bakx"] {
            fs::write(sibling(&path, suffix), "original").unwrap();
        }

        assert_eq!(backup(&path).unwrap(), sibling(&path, ".bak"));
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
