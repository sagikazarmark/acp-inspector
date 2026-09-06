//! What the agent says it has open (`docs/architecture.md` §7.1, §9): the
//! answer to `session/list`, and the page marker it may carry.
//!
//! **The cursor is opaque, and this module is where that is a fact rather than
//! a promise.** ACP calls `nextCursor` an opaque token; no agent the survey
//! covered emits one, so the paging path is unexercised ground the inspector
//! must not improvise on. [`Cursor`] therefore has no constructor outside this
//! crate and no way to read what is inside it: the only thing anyone above core
//! can do with one is hand it back, which is the only thing the protocol says
//! it is for.

use agent_client_protocol_schema::v1;

/// The agent's answer to `session/list`, page after page.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct SessionListing {
    /// The session infos the agent returned, in the order it returned them —
    /// every page that has been asked for, oldest ask first.
    pub sessions: Vec<v1::SessionInfo>,
    /// The agent's marker for a page it did not send.
    ///
    /// `None` means the agent said there is no more, which is what every agent
    /// the survey covered says. When one does appear, handing it back is the
    /// *whole* of what this crate does with paging (§7.1: paging is ignored) —
    /// nothing counts pages, nothing prefetches, and nothing looks inside the
    /// token.
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

    /// The token, for the one caller allowed to have it: the request that hands
    /// it back.
    pub(crate) fn token(&self) -> String {
        self.0.clone()
    }
}
