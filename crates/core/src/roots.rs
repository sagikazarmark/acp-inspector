//! The additional roots a session may be opened with
//! (`docs/architecture.md` §7.7): `additionalDirectories`, as the user supplies
//! them.
//!
//! **An advertisement the panel drew for four rings with nothing anywhere that
//! could reach it.** `sessionCapabilities.additionalDirectories` is claimed like
//! every other session capability and driven like none of them, because it gates
//! a *field on a request* rather than a method — so what makes it reachable is a
//! control beside the calls that carry the field, on `session/new` and on both
//! ways of reopening a session.
//!
//! **One absolute path per line**, which is the spawn form's rule for its two
//! list fields ([`AgentCommand::args`](crate::AgentCommand::args)) and is here
//! for the same reasons: nothing to quote, nothing to escape, and a blank line
//! is somebody halfway through typing rather than a root.
//!
//! **A relative path is resolved against the session's `cwd`.** The schema says
//! each path must be absolute and, two lines above that, that `cwd` "remains the
//! base for relative paths" — so the base is the *session's*, and not the
//! inspector's own working directory, which is what the spawn form's `cwd`
//! resolves against for a reason that does not transfer (the child process is
//! started there). This tool speaks ACP as a well-formed client (§1), and a
//! request the schema calls malformed would contaminate every conformance
//! annotation drawn against that agent with this client's own violation. Driving
//! the corners the specification leaves *undefined* is the rule (§7.6); a
//! malformed request is not one of them.
//!
//! **Empty puts nothing on the wire.** The field is omitted when the list is
//! empty ([`v1::NewSessionRequest`] skips it), so an empty control drives
//! nothing — which is how the record of what was driven reads the capability off
//! the outgoing frame rather than off what anybody meant (§7.7).

use std::path::{Path, PathBuf};

use agent_client_protocol_schema::v1;

/// The roots a session is to be opened with, as they were supplied.
///
/// Text, because that is what a control holds and because the rules for reading
/// it are core's rather than a window's — the same split the spawn form's four
/// fields already follow ([`AgentCommand`](crate::AgentCommand)). What crosses
/// the wire is [`resolved`](Self::resolved) against the session's own working
/// directory, and never this.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Roots(String);

impl Roots {
    /// The roots as somebody supplied them, one per line.
    #[must_use]
    pub fn supplied(supplied: impl Into<String>) -> Self {
        Self(supplied.into())
    }

    /// The roots an agent reported for a session it listed, as a control would
    /// hold them.
    ///
    /// **What a reopen is prefilled with** (§7.5, §7.7), so that reopening stays
    /// a reopen unless the user deliberately changes it. It is a default and not
    /// a constraint: the schema permits a reopen's list to differ from "any
    /// previously used or reported list as long as the request `cwd` matches the
    /// session's `cwd`", and changing it is the point of the control.
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

    /// The roots this asks for, every one of them absolute (§7.7).
    ///
    /// Relative paths are resolved against `cwd` — the *session's* working
    /// directory, which the request carries and which the schema names as the
    /// base for them. Blank lines carry no root and surrounding whitespace is
    /// not part of a path, under the rule the spawn form's list fields already
    /// follow.
    ///
    /// **Nothing is canonicalized**, and nothing is checked to exist. ACP asks
    /// for an absolute path and says nothing about whether it resolves; a
    /// client that quietly rewrote `/tmp/../srv` or refused a directory that is
    /// not there yet would be answering a question the agent was asked.
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

/// The roots a `session/new` asks for: the ones a control supplied, and none
/// where there is no control (§7.7).
///
/// Nothing is a session's *default* roots here the way it is for a reopen —
/// there is no session yet to have reported any — so the two callers of this
/// (the request, and the caller that has no connection to send it on) read the
/// same empty answer.
pub(crate) fn creating(roots: Option<&Roots>, cwd: &Path) -> Vec<PathBuf> {
    roots.map(|roots| roots.resolved(cwd)).unwrap_or_default()
}

/// The roots a reopen asks for: the ones a control supplied, and the ones the
/// listing reported where none did (§7.5, §7.7).
///
/// **The reported list is the default and crosses whatever the agent
/// advertised**, because handing an agent its own words back is fidelity to the
/// session being reopened rather than a claim about a capability — a reopen
/// gated into asking for less would be reopening a different session from the
/// one it named. Resolved against the session's own `cwd` either way.
pub(crate) fn reopening(roots: Option<&Roots>, session: &v1::SessionInfo) -> Vec<PathBuf> {
    match roots {
        Some(roots) => roots.resolved(&session.cwd),
        None => under(&session.cwd, &session.additional_directories),
    }
}

/// Paths as they will cross, resolved against the working directory the request
/// carries.
///
/// The one rule, in the one place both callers read it from: what the user
/// supplied goes through [`Roots::resolved`], and what an agent *reported* for a
/// session goes through this on the way back to it. An agent that reported a
/// relative root has said something the schema forbids, and handing it back
/// unchanged would put this client's name on it.
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
        // Never against the inspector's, which is where the spawn form's `cwd`
        // resolves and for a reason that does not transfer: the agent's child
        // process is started there, and a session's roots are the session's.
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
        // Half-typed input is the state a form spends most of its life in, and a
        // trailing space nobody can see is not something a control should be able
        // to put on the wire.
        let roots = Roots::supplied("\n  /srv/data  \n\n\t\n/srv/models\n");

        assert_eq!(
            roots.resolved(Path::new("/home/work")),
            [PathBuf::from("/srv/data"), PathBuf::from("/srv/models")]
        );
    }

    #[test]
    fn an_empty_control_asks_for_nothing() {
        // Which is what keeps it off the wire: the field is omitted when the
        // list is empty, so an empty control drives nothing (§7.7).
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
        // One per line, as the control holds them — and back out again exactly
        // as they went in, because a reopen that quietly asked for something
        // else would not be a reopen (§7.5).
        let reported = [PathBuf::from("/srv/data"), PathBuf::from("/srv/models")];
        let roots = Roots::reported(&reported);

        assert_eq!(roots.text(), "/srv/data\n/srv/models");
        assert_eq!(roots.resolved(Path::new("/tmp")), reported);
        assert_eq!(Roots::reported(&[]), Roots::default());
    }

    #[test]
    fn a_root_an_agent_reported_relative_is_resolved_too() {
        // An agent that reported a relative root said something the schema
        // forbids. Handing it back unchanged would put this client's name on it,
        // so the one rule covers both directions.
        assert_eq!(
            under(Path::new("/home/work"), [PathBuf::from("data")]),
            [PathBuf::from("/home/work/data")]
        );
    }
}
