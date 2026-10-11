//! The files the inspector keeps of its own (recent commands, window settings):
//! where they live, how they are loaded, and how one is replaced atomically.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The directory the inspector keeps its own state under, inside the platform's.
const DIRECTORY: &str = "acp-inspector";

/// Who a file is written for.
pub(crate) enum Access {
    /// The user's own to read (`0600` on Unix), for files that may hold
    /// secrets such as environment values.
    Private,
    /// Whatever the platform gives a new file.
    Ordinary,
}

/// Where a file of the inspector's lives by default: the platform's state
/// directory (the data directory on macOS and Windows), under the inspector's
/// own directory. State rather than config, since nobody is meant to edit them.
///
/// `None` when there is no home directory; no fallback location is invented.
pub(crate) fn location(file: &str) -> Option<PathBuf> {
    let base = dirs::state_dir().or_else(dirs::data_dir)?;
    Some(base.join(DIRECTORY).join(file))
}

/// Reads a document and parses it with `parse`.
///
/// A file that is there but cannot be read as one of these is moved aside to
/// `<name>.bak` rather than left for the next save to overwrite; the second
/// value is where it went. A missing file is just an empty answer.
pub(crate) fn load<T>(
    path: &Path,
    parse: impl FnOnce(&str) -> Option<T>,
) -> (Option<T>, Option<PathBuf>) {
    let Ok(bytes) = fs::read(path) else {
        return (None, None);
    };
    if let Some(parsed) = std::str::from_utf8(&bytes).ok().and_then(parse) {
        return (Some(parsed), None);
    }
    let backup = beside(path, ".bak");
    (None, fs::rename(path, &backup).ok().map(|()| backup))
}

/// Puts a document where it lives, making the directory if it is not there yet.
///
/// Written beside, synced, and renamed into place, so an interrupted save leaves
/// the old file or the new one, never half of one. The staging name is unique,
/// so concurrent saves cannot write into each other's.
pub(crate) fn replace(path: &Path, document: &str, access: Access) -> io::Result<()> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }

    let staged = staging(path);
    write(&staged, document, access)
        .and_then(|()| fs::rename(&staged, path))
        .inspect_err(|_| {
            let _ = fs::remove_file(&staged);
        })?;
    // Makes the rename itself durable; best effort, as not every filesystem
    // lets a directory be synced.
    #[cfg(unix)]
    if let Some(directory) = path.parent() {
        let _ = File::open(directory).and_then(|directory| directory.sync_all());
    }
    Ok(())
}

/// Where a new document is written before it becomes the file: beside it, since
/// a rename is only atomic within one filesystem.
fn staging(path: &Path) -> PathBuf {
    static STAGED: AtomicU64 = AtomicU64::new(0);
    let nth = STAGED.fetch_add(1, Ordering::Relaxed);
    beside(path, &format!(".{}-{nth}.new", std::process::id()))
}

fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut beside = path.as_os_str().to_owned();
    beside.push(suffix);
    PathBuf::from(beside)
}

fn write(path: &Path, document: &str, access: Access) -> io::Result<()> {
    let mut file = create(path, access)?;
    file.write_all(document.as_bytes())?;
    file.sync_all()
}

/// Creates the staging file, which no one else can have.
fn create(path: &Path, access: Access) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        if matches!(access, Access::Private) {
            options.mode(0o600);
        }
    }
    #[cfg(not(unix))]
    let _ = access;
    options.open(path)
}
