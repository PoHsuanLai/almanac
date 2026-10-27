//! Reading and writing the config files memory keeps.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Writes `text` to `path` so that a crash leaves the old file or the new one, never a cut one: a
/// temp file next to it (mode 0600), synced, renamed over it, then the directory synced. The
/// parent directory is created.
pub fn write_private_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let temp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)
        .and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temp, path));
    written.inspect_err(|_| drop(std::fs::remove_file(&temp)))?;
    std::fs::File::open(dir).and_then(|d| d.sync_all())
}

/// The text of a config file: `None` when there is no such file (a fresh install), an error for
/// any other failure to read it, so a file that cannot be read is never taken for an empty one.
pub fn read_config(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
