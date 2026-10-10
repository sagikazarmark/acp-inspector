//! What the agent says it has open (`docs/architecture.md` §7.1, §9): the
//! answer to `session/list`, and the page marker it may carry.
//!
//! ACP calls `nextCursor` opaque, so [`Cursor`] cannot be built or read outside
//! this crate; it can only be handed back.

use agent_client_protocol_schema::v1;

/// The agent's answer to `session/list`, page after page.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct SessionListing {
    /// The session infos the agent returned, in the order it returned them —
    /// every page that has been asked for, oldest ask first.
    pub sessions: Vec<v1::SessionInfo>,
    /// The agent's marker for a page it did not send; `None` means no more.
    /// It is only ever handed back on request: nothing prefetches or counts
    /// pages.
    pub next_page: Option<Cursor>,
}

/// A page marker the agent handed out, kept exactly as it arrived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor(String);

impl SessionListing {
    /// Folds a page into the listing it continues.
    pub(crate) fn extend(&mut self, page: Self) {
        self.sessions.extend(page.sessions);
        self.next_page = page.next_page;
    }
}

impl Cursor {
    pub(crate) fn new(token: String) -> Self {
        Self(token)
    }

    /// The token, for the request that hands it back.
    pub(crate) fn token(&self) -> String {
        self.0.clone()
    }
}
