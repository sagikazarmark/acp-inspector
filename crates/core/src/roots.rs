//! The additional roots a session may be opened with
//! (`docs/architecture.md` §7.7): `additionalDirectories`, as the user supplies
//! them.
//!
//! One path per line. A relative path is resolved against the session's `cwd`,
//! which the schema names as the base, so every root on the wire is absolute
//! as the schema requires. An empty list puts nothing on the wire
//! ([`v1::NewSessionRequest`] omits the field), so it drives nothing.

use std::path::{Path, PathBuf};

use agent_client_protocol_schema::v1;

/// The roots a session is to be opened with, as text the control holds. What
/// crosses the wire is [`resolved`](Self::resolved) against the session's
/// working directory.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Roots(String);

impl Roots {
    /// The roots as somebody supplied them, one per line.
    #[must_use]
    pub fn supplied(supplied: impl Into<String>) -> Self {
        Self(supplied.into())
    }

    /// The roots an agent reported for a session it listed, one per line.
    ///
    /// Used to prefill a reopen (§7.5); the user may change them, which the
    /// schema permits as long as `cwd` matches.
    #[must_use]
    pub fn reported(reported: &[PathBuf]) -> Self {
        Self(
            reported
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    /// What the control holds, for the control to render.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.0
    }

    /// The roots this asks for, every one of them absolute.
    ///
    /// Lines are trimmed and blank ones skipped; relative paths are joined to
    /// `cwd`, the session's working directory. Nothing is canonicalized or
    /// checked to exist.
    #[must_use]
    pub fn resolved(&self, cwd: &Path) -> Vec<PathBuf> {
        under(
            cwd,
            self.0
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty()),
        )
    }
}

/// The roots a `session/new` asks for: the supplied ones, or none.
pub(crate) fn creating(roots: Option<&Roots>, cwd: &Path) -> Vec<PathBuf> {
    roots.map(|roots| roots.resolved(cwd)).unwrap_or_default()
}

/// The roots a reopen asks for: the supplied ones, or else the ones the listing
/// reported (§7.5).
///
/// The reported default is sent whether or not the agent advertised the
/// capability: it is the session's own list, not a new claim.
pub(crate) fn reopening(roots: Option<&Roots>, session: &v1::SessionInfo) -> Vec<PathBuf> {
    match roots {
        Some(roots) => roots.resolved(&session.cwd),
        None => under(&session.cwd, &session.additional_directories),
    }
}

/// Paths resolved against `cwd`. Applied to reported roots too, so a relative
/// root an agent reported is never sent back unresolved.
fn under<P: AsRef<Path>>(cwd: &Path, roots: impl IntoIterator<Item = P>) -> Vec<PathBuf> {
    roots
        .into_iter()
        .map(|root| {
            let root = root.as_ref();
            if root.is_absolute() {
                root.to_path_buf()
            } else {
                cwd.join(root)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_root_the_user_typed_absolute_crosses_as_typed() {
        let roots = Roots::supplied("/srv/data\n/srv/models");

        assert_eq!(
            roots.resolved(Path::new("/home/work")),
            [PathBuf::from("/srv/data"), PathBuf::from("/srv/models")]
        );
    }

    #[test]
    fn a_relative_root_is_resolved_against_the_sessions_own_directory() {
        // The session's cwd, never the inspector's own.
        let roots = Roots::supplied("data\n../shared");

        assert_eq!(
            roots.resolved(Path::new("/home/work")),
            [
                PathBuf::from("/home/work/data"),
                PathBuf::from("/home/work/../shared")
            ],
            "resolved, and not normalized: what `..` means is the agent's business"
        );
    }

    #[test]
    fn blank_lines_and_surrounding_space_carry_no_root() {
        let roots = Roots::supplied("\n  /srv/data  \n\n\t\n/srv/models\n");

        assert_eq!(
            roots.resolved(Path::new("/home/work")),
            [PathBuf::from("/srv/data"), PathBuf::from("/srv/models")]
        );
    }

    #[test]
    fn an_empty_control_asks_for_nothing() {
        // An empty list keeps the field off the wire, so it drives nothing.
        assert!(
            Roots::default()
                .resolved(Path::new("/home/work"))
                .is_empty()
        );
        assert!(
            Roots::supplied("   \n\n")
                .resolved(Path::new("/home/work"))
                .is_empty()
        );
    }

    #[test]
    fn what_the_listing_reported_is_what_the_control_is_prefilled_with() {
        // Round-trips exactly, so an unchanged reopen asks for the same roots.
        let reported = [PathBuf::from("/srv/data"), PathBuf::from("/srv/models")];
        let roots = Roots::reported(&reported);

        assert_eq!(roots.text(), "/srv/data\n/srv/models");
        assert_eq!(roots.resolved(Path::new("/tmp")), reported);
        assert_eq!(Roots::reported(&[]), Roots::default());
    }

    #[test]
    fn a_root_an_agent_reported_relative_is_resolved_too() {
        assert_eq!(
            under(Path::new("/home/work"), [PathBuf::from("data")]),
            [PathBuf::from("/home/work/data")]
        );
    }
}
