//! What the agent reports about a session as it goes, apart from its settings:
//! the title and last activity a `session_info_update` states, and the context
//! and cost the last `usage_update` stated. See `docs/architecture.md` §7.5.
//!
//! Kept per session rather than for the live one only, because a
//! `session/load` replays ahead of its answer: what it says about the session
//! being opened has to land somewhere before that session is live.

use std::collections::HashMap;

use agent_client_protocol_schema::{MaybeUndefined, v1};

/// What the agent has reported about each session on this connection, as one
/// read of [`Inspector::session_reports`](crate::Inspector::session_reports).
///
/// Written by the reader as each update crosses, so what is held is the last
/// thing said. Emptied when another agent connects; a session's own report is
/// dropped as a call reopening it goes out, so its replay starts from nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionReports(HashMap<v1::SessionId, SessionReport>);

/// What the agent has stated about one session.
///
/// `title` and `updated_at` are `None` where no update has mentioned them,
/// `Some(None)` where the last one that did cleared them, and `Some(Some(_))`
/// where it set them — the three states a `session_info_update` field has on
/// the wire.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct SessionReport {
    pub title: Option<Option<String>>,
    pub updated_at: Option<Option<String>>,
    /// The last `usage_update`, whole: each one restates the context in use,
    /// the window's size and the session's cumulative cost, so the latest is
    /// the account and nothing is summed. `None` until the agent sends one.
    pub usage: Option<v1::UsageUpdate>,
}

/// A value the agent stated about a session, and where it stated it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stated {
    /// `None` where the agent said there is none: a listed session with no
    /// title, or an update that cleared it.
    pub value: Option<String>,
    pub by: StatedBy,
}

/// Which of the agent's two ways of describing a session a [`Stated`] value
/// came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatedBy {
    /// A `session_info_update`.
    Update,
    /// A `session/list` row.
    Listing,
}

impl SessionReports {
    /// What the agent has stated about `session`, if it has stated anything.
    pub fn of(&self, session: &v1::SessionId) -> Option<&SessionReport> {
        self.0.get(session)
    }

    /// The title `session` goes by: the last update's where an update has
    /// stated one, else the listing's, else `None`.
    ///
    /// **An update outranks the listing**, whichever arrived later. The
    /// listing is an answer the inspector asked for and folds when the call
    /// returns, not where its frame crossed, so their order on the wire is not
    /// one this layer can read off; and an update is the agent telling the
    /// client, unasked, what the session is called now. The listing's value
    /// stays in the listing, where a surface can show the two disagreeing.
    pub fn title(
        &self,
        session: &v1::SessionId,
        listed: Option<&v1::SessionInfo>,
    ) -> Option<Stated> {
        stated(
            self.of(session).and_then(|report| report.title.clone()),
            listed.map(|info| info.title.clone()),
        )
    }

    /// When `session` was last active, by the same rule as
    /// [`title`](Self::title).
    pub fn updated_at(
        &self,
        session: &v1::SessionId,
        listed: Option<&v1::SessionInfo>,
    ) -> Option<Stated> {
        stated(
            self.of(session)
                .and_then(|report| report.updated_at.clone()),
            listed.map(|info| info.updated_at.clone()),
        )
    }

    /// Folds a `session_info_update` into `session`'s report, and answers
    /// whether it changed anything. An absent field leaves what was stated.
    ///
    /// The reader does this as each update crosses; public so a screen can be
    /// drawn over reports nobody received.
    pub fn info(&mut self, session: &v1::SessionId, update: &v1::SessionInfoUpdate) -> bool {
        let report = self.0.entry(session.clone()).or_default();
        let before = report.clone();
        patched(&mut report.title, &update.title);
        patched(&mut report.updated_at, &update.updated_at);
        *report != before
    }

    /// Takes a `usage_update` as `session`'s usage, and answers whether it
    /// changed anything. Public for the same reason as [`info`](Self::info).
    pub fn usage(&mut self, session: &v1::SessionId, update: &v1::UsageUpdate) -> bool {
        let report = self.0.entry(session.clone()).or_default();
        if report.usage.as_ref() == Some(update) {
            return false;
        }
        report.usage = Some(update.clone());
        true
    }

    /// Drops everything stated about `session`, and answers whether there was
    /// anything.
    pub(crate) fn forget(&mut self, session: &v1::SessionId) -> bool {
        self.0.remove(session).is_some()
    }
}

/// Writes a field the update mentions, set or cleared; leaves one it does not.
fn patched(field: &mut Option<Option<String>>, update: &MaybeUndefined<String>) {
    match update {
        MaybeUndefined::Undefined => {}
        MaybeUndefined::Null => *field = Some(None),
        MaybeUndefined::Value(value) => *field = Some(Some(value.clone())),
    }
}

fn stated(updated: Option<Option<String>>, listed: Option<Option<String>>) -> Option<Stated> {
    updated
        .map(|value| Stated {
            value,
            by: StatedBy::Update,
        })
        .or_else(|| {
            listed.map(|value| Stated {
                value,
                by: StatedBy::Listing,
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> v1::SessionId {
        v1::SessionId::new("s1")
    }

    fn listed(title: Option<&str>) -> v1::SessionInfo {
        v1::SessionInfo::new("s1", "/tmp").title(title.map(str::to_owned))
    }

    #[test]
    fn an_update_sets_clears_and_leaves_alone_as_its_fields_say() {
        let mut reports = SessionReports::default();

        assert!(reports.info(
            &session(),
            &v1::SessionInfoUpdate::new().title("Fix the build")
        ));
        assert_eq!(
            reports.of(&session()),
            Some(&SessionReport {
                title: Some(Some("Fix the build".to_owned())),
                updated_at: None,
                usage: None,
            })
        );

        // A later update that does not mention the title keeps it.
        assert!(reports.info(
            &session(),
            &v1::SessionInfoUpdate::new().updated_at("2026-10-12T09:00:00Z")
        ));
        assert_eq!(
            reports
                .of(&session())
                .and_then(|report| report.title.clone()),
            Some(Some("Fix the build".to_owned()))
        );

        // One that clears it says so, rather than forgetting it was said.
        let mut cleared = v1::SessionInfoUpdate::new();
        cleared.title = MaybeUndefined::Null;
        assert!(reports.info(&session(), &cleared));
        assert_eq!(
            reports
                .of(&session())
                .and_then(|report| report.title.clone()),
            Some(None)
        );

        assert!(
            !reports.info(&session(), &cleared),
            "the same statement twice changes nothing"
        );
    }

    #[test]
    fn a_title_an_update_stated_outranks_the_listings() {
        let mut reports = SessionReports::default();
        assert_eq!(
            reports.title(&session(), None),
            None,
            "nobody said anything"
        );
        assert_eq!(
            reports.title(&session(), Some(&listed(None))),
            Some(Stated {
                value: None,
                by: StatedBy::Listing,
            }),
            "a listed session with no title is the agent saying it has none"
        );

        reports.info(&session(), &v1::SessionInfoUpdate::new().title("Renamed"));
        assert_eq!(
            reports.title(&session(), Some(&listed(Some("Listed")))),
            Some(Stated {
                value: Some("Renamed".to_owned()),
                by: StatedBy::Update,
            })
        );

        // An update about the activity alone leaves the title to the listing.
        let other = v1::SessionId::new("s2");
        reports.info(&other, &v1::SessionInfoUpdate::new().updated_at("now"));
        assert_eq!(
            reports
                .title(&other, Some(&listed(Some("Listed"))))
                .map(|title| title.by),
            Some(StatedBy::Listing)
        );
    }

    #[test]
    fn the_last_usage_stated_is_the_usage_and_leaves_the_title_alone() {
        let mut reports = SessionReports::default();
        reports.info(&session(), &v1::SessionInfoUpdate::new().title("Named"));

        assert!(reports.usage(&session(), &v1::UsageUpdate::new(1_000, 200_000)));
        let later = v1::UsageUpdate::new(42_000, 200_000).cost(v1::Cost::new(0.31, "USD"));
        assert!(reports.usage(&session(), &later));
        assert!(
            !reports.usage(&session(), &later),
            "the same statement twice changes nothing"
        );

        let report = reports.of(&session()).expect("reported");
        assert_eq!(report.usage.as_ref(), Some(&later), "replaced, not summed");
        assert_eq!(report.title, Some(Some("Named".to_owned())));
    }
}
