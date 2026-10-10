//! The Trace filter box (F8): what it asked for, and what it matched.

use acp_inspector_core::TracedFrame;

use super::pairing::Pairs;
use crate::console::contains_folded;

/// What the Trace filter box asked for (F8): `method:NAME` and `id:N` match
/// those envelope fields, and every other word is text the frame must contain.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Query {
    pub(super) method: Option<String>,
    pub(super) id: Option<String>,
    /// The free text, lowercased once.
    pub(super) text: String,
}

impl Query {
    pub(super) fn parse(filter: &str) -> Self {
        let mut query = Self::default();
        let mut text = Vec::new();
        for word in filter.split_whitespace() {
            if let Some(method) = word.strip_prefix("method:").filter(|rest| !rest.is_empty()) {
                query.method = Some(method.to_lowercase());
            } else if let Some(id) = word.strip_prefix("id:").filter(|rest| !rest.is_empty()) {
                query.id = Some(id.trim_matches('"').to_owned());
            } else {
                text.push(word);
            }
        }
        query.text = text.join(" ").to_lowercase();
        query
    }
}

/// What the Trace remembers between renders (L1): which frames matched the
/// text of the current filter.
#[derive(Default)]
pub(super) struct Index {
    /// The filter text the matches below are for.
    pub(super) text: String,
    matches: std::collections::HashMap<u64, bool>,
}

impl Index {
    /// Forgets what the filter matched when the filter changed or frames aged
    /// out.
    pub(super) fn update(&mut self, dropped: usize, query: &Query) {
        let first = dropped as u64;
        self.matches.retain(|ordinal, _| *ordinal >= first);
        if self.text != query.text {
            self.text.clone_from(&query.text);
            self.matches.clear();
        }
    }

    /// Whether a frame survives the query, remembering the text match.
    pub(super) fn matches(
        &mut self,
        (ordinal, traced): (u64, &TracedFrame),
        query: &Query,
        pairs: &Pairs<'_>,
    ) -> bool {
        let summary = traced.frame.summary();
        if let Some(method) = &query.method {
            let named = summary
                .method
                .as_deref()
                .or_else(|| pairs.answering(ordinal, traced))
                .unwrap_or_default();
            if !contains_folded(named, method) {
                return false;
            }
        }
        if let Some(id) = &query.id
            && summary.id.as_deref().map(|own| own.trim_matches('"')) != Some(id.as_str())
        {
            return false;
        }
        if query.text.is_empty() {
            return true;
        }
        *self
            .matches
            .entry(ordinal)
            .or_insert_with(|| contains_folded(traced.frame.as_str(), &query.text))
    }
}
