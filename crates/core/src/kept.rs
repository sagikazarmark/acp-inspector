//! The files the inspector keeps of its own, and how it replaces one.
//!
//! Two of them now — the spawn form's recent commands and the appearance
//! preference — and neither is a contract: nothing else is meant to read them,
//! so each carries a format tag and a version only so that a later shape can
//! *ignore* an older file rather than misread one. What they share is where they
//! live and how they are written, which is stated once here so that the second
//! file cannot quietly acquire different answers from the first.
//!
//! They stay separate files. Coupling a convenience list's version to a
//! preference's helps neither, and only one of them holds anything worth keeping
//! to its owner.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The directory the inspector keeps its own state under, inside the platform's.
const DIRECTORY: &str = "acp-inspector";

/// Who a file is written for.
pub(crate) enum Access {
    /// The user's own to read (`0600` on Unix) — for a file whose contents
    /// would embarrass its owner, like the environment of a remembered
    /// invocation.
    Private,
    /// Whatever the platform gives a new file.
    Ordinary,
}

/// Where a file of the inspector's lives when nobody says otherwise: the
/// platform's own directory for a program's state, under a directory of the
/// inspector's.
///
/// The state directory rather than the config one, because nobody wrote these
/// and nobody is meant to edit them — they are what the app noticed, not what
/// the user typed into a file. Platforms that have no such notion (macOS,
/// Windows) answer with their data directory, which is where a program's own
/// files go there.
///
/// `None` on a machine with no home directory at all, which is a machine with
/// nowhere to put it — and is answered by saying so rather than by picking
/// somewhere, the same way [`AgentCommand::session_cwd`] refuses to invent a
/// directory.
///
/// [`AgentCommand::session_cwd`]: crate::AgentCommand::session_cwd
pub(crate) fn location(file: &str) -> Option<PathBuf> {
    let base = dirs::state_dir().or_else(dirs::data_dir)?;
    Some(base.join(DIRECTORY).join(file))
}

/// Puts a document where it lives, making the directory if it is not there yet.
///
/// **Written beside and moved into place**, so a save interrupted partway — the
/// window quit, the disk filled — leaves either the old file or the new one and
/// never half of one, which would read as no file at all and take everything
/// that was already in it along. Nothing is flushed to the platter: these are
/// conveniences, and a file that survives a crash but not a power cut is the
/// trade the export makes too.
pub(crate) fn replace(path: &Path, document: &str, access: Access) -> io::Result<()> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }

    let staged = staging(path);
    write(&staged, document, access)
        .and_then(|()| fs::rename(&staged, path))
        .inspect_err(|_| {
            let _ = fs::remove_file(&staged);
        })
}

/// Where a new document is written before it becomes the file.
///
/// Beside the real file rather than in a temp directory, because the move that
/// puts it in place is only atomic within one filesystem.
fn staging(path: &Path) -> PathBuf {
    let mut staged = path.as_os_str().to_owned();
    staged.push(".new");
    PathBuf::from(staged)
}

fn write(path: &Path, document: &str, access: Access) -> io::Result<()> {
    let mut file = create(path, access)?;
    file.write_all(document.as_bytes())?;
    file.flush()
}

/// Creates the staging file, over whatever a previous interrupted save left
/// there.
fn create(path: &Path, access: Access) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
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
