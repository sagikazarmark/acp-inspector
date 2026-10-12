//! Keys for lists named by values the agent chose.
//!
//! Dioxus requires sibling keys to be unique: a debug build asserts it, and a
//! release build's keyed diff maps two equal keys onto one node and leaves the
//! other behind. Request ids, command names, option ids and the like are the
//! agent's to choose, and a misbehaving agent — the kind this window exists to
//! look at — repeats them. So a list keyed by them takes its keys from here.

use std::collections::HashSet;

/// Each key as given, except that a repeat is told apart by its occurrence.
///
/// The first of any run keeps its key unchanged, so a well-behaved agent's
/// rows key exactly as before, and a row keeps its key while later rows
/// arrive. A repeat gets a suffix no printable value carries; should the
/// suffixed key itself be taken, the count goes on until it is not.
pub(crate) fn distinct<I>(keys: I) -> Vec<String>
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let mut taken = HashSet::new();
    keys.into_iter()
        .map(|key| {
            let key: String = key.into();
            let mut candidate = key.clone();
            let mut occurrence = 1;
            while taken.contains(&candidate) {
                candidate = format!("{key}\u{1f}{occurrence}");
                occurrence += 1;
            }
            taken.insert(candidate.clone());
            candidate
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::distinct;

    #[test]
    fn unrepeated_keys_are_unchanged() {
        assert_eq!(distinct(["a", "b"]), ["a", "b"]);
    }

    #[test]
    fn a_repeat_is_told_apart_and_the_first_keeps_its_key() {
        let keys = distinct(["a", "a", "b", "a"]);
        assert_eq!(keys[0], "a");
        assert_eq!(keys[2], "b");
        let unique: std::collections::HashSet<&String> = keys.iter().collect();
        assert_eq!(unique.len(), keys.len(), "{keys:?}");
    }

    #[test]
    fn a_value_that_looks_like_a_suffixed_key_does_not_collide() {
        let keys = distinct(["a", "a", "a\u{1f}1"]);
        let unique: std::collections::HashSet<&String> = keys.iter().collect();
        assert_eq!(unique.len(), keys.len(), "{keys:?}");
    }
}
