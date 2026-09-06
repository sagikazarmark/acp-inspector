//! The appearance preference (`docs/architecture.md` §9): the three values the
//! control offers, and where the choice is kept between runs.
//!
//! Driven through the file rather than around it, for the reason
//! `tests/recent.rs` is: the whole promise is that a choice survives the process
//! that made it, so every test that says "across restarts" loads a second store
//! from the same path, which is what a restart is from this store's point of
//! view.
//!
//! None of it needs a window. What the token is, what an unreadable file means
//! and what a value from a future version falls back to are facts about a
//! preference, and the desktop crate only paints what they answer.

use std::path::PathBuf;

use acp_inspector_core::{Appearance, Settings};

#[test]
fn tokens_round_trip() {
    for appearance in Appearance::ALL {
        assert_eq!(Appearance::from_token(appearance.token()), appearance);
    }
}

/// `token()` names the DOM attribute and serde names the stored value; they are
/// two spellings of one string, and a renamed variant must not let them drift
/// apart.
#[test]
fn the_stored_form_is_the_token_for_every_choice() {
    for appearance in Appearance::ALL {
        let stored = serde_json::to_string(&appearance).expect("an appearance serializes");
        assert_eq!(stored, format!("\"{}\"", appearance.token()));

        let restored: Appearance = serde_json::from_str(&stored).expect("an appearance parses");
        assert_eq!(restored, appearance);
    }
}

#[test]
fn a_token_nobody_here_has_heard_of_is_system() {
    // A value written by a future version, and the absence of one. Neither is a
    // reason to leave the window unpainted: the operating system has an answer
    // for how it should look, and that answer is what System is.
    assert_eq!(Appearance::from_token("sepia"), Appearance::System);
    assert_eq!(Appearance::from_token(""), Appearance::System);
    assert_eq!(Appearance::default(), Appearance::System);
}

/// The control offers System first, because it is the default and the one a
/// window opens on before anybody has chosen anything.
#[test]
fn the_control_offers_the_default_first() {
    assert_eq!(Appearance::ALL[0], Appearance::System);
    assert_eq!(
        Appearance::ALL.map(Appearance::label),
        ["System", "Light", "Dark"]
    );
}

#[test]
fn a_choice_is_still_in_effect_after_a_restart() {
    let path = scratch("restart").join("settings.json");
    let settings = Settings::load_from(&path);
    assert_eq!(
        settings.appearance(),
        Appearance::System,
        "nothing has been chosen yet"
    );

    settings
        .set_appearance(Appearance::Light)
        .expect("somewhere to write");

    // The store the next launch of the app would build: same path, new value,
    // nothing carried over in memory.
    let reopened = Settings::load_from(&path);
    assert_eq!(reopened.appearance(), Appearance::Light);

    // And returning to System is a choice like the other two, not a forgetting:
    // it has to survive the same restart.
    reopened
        .set_appearance(Appearance::System)
        .expect("somewhere to write");
    assert_eq!(Settings::load_from(&path).appearance(), Appearance::System);
}

#[test]
fn a_file_that_is_not_there_yet_leaves_the_window_on_system() {
    // The first launch of the app, which is the state it spends its first run
    // in: no file, no directory, and nothing to say about either.
    let path = scratch("first-run").join("nested").join("settings.json");
    let settings = Settings::load_from(&path);
    assert_eq!(settings.appearance(), Appearance::System);

    settings
        .set_appearance(Appearance::Dark)
        .expect("somewhere to write");
    assert_eq!(Settings::load_from(&path).appearance(), Appearance::Dark);
}

#[test]
fn a_file_this_never_wrote_leaves_the_window_on_system() {
    for (name, contents) in [
        // Not JSON at all.
        ("unreadable", "{ this is not the file we left }".to_owned()),
        // JSON, and something else's.
        (
            "foreign",
            r#"{"format":"something-else","version":1,"appearance":"dark"}"#.to_owned(),
        ),
        // This file, from a version whose shape this one cannot read. The
        // appearance in it is deliberately one this *could* name: it is the
        // version that makes the file unusable, not the value.
        (
            "future",
            format!(
                r#"{{"format":"acp-inspector-settings","version":{},"appearance":"dark"}}"#,
                u32::MAX
            ),
        ),
        // This file, this version, and a token no version of this has heard of.
        (
            "unknown-value",
            r#"{"format":"acp-inspector-settings","version":1,"appearance":"sepia"}"#.to_owned(),
        ),
    ] {
        let path = scratch(name).join("settings.json");
        std::fs::write(&path, contents).expect("a file to write");

        let settings = Settings::load_from(&path);
        assert_eq!(
            settings.appearance(),
            Appearance::System,
            "{name}: a file this cannot read is no preference at all"
        );

        settings
            .set_appearance(Appearance::Dark)
            .expect("somewhere to write");
        assert_eq!(
            Settings::load_from(&path).appearance(),
            Appearance::Dark,
            "{name}: and the next choice is written over it"
        );
    }
}

#[test]
fn a_preference_with_nowhere_to_live_still_applies_for_the_session() {
    // A save that fails is silent — an appearance is one click to redo — but it
    // must still be the appearance this run presents in, or the click would do
    // nothing at all.
    let blocked = scratch("blocked").join("in-the-way");
    std::fs::write(&blocked, "not a directory").expect("a file to write");

    let settings = Settings::load_from(blocked.join("settings.json"));
    let saved = settings.set_appearance(Appearance::Dark);

    assert!(saved.is_err(), "there is nowhere to write it: {saved:?}");
    assert_eq!(
        settings.appearance(),
        Appearance::Dark,
        "and the window is dark for this run regardless"
    );
}

#[test]
fn the_file_is_the_shape_the_docs_describe() {
    let path = scratch("shape").join("settings.json");
    let settings = Settings::load_from(&path);
    settings
        .set_appearance(Appearance::Light)
        .expect("somewhere to write");

    let written = std::fs::read_to_string(&path).expect("the file it wrote");
    let file: serde_json::Value = serde_json::from_str(&written).expect("one JSON document");

    assert_eq!(file["format"], "acp-inspector-settings");
    assert_eq!(file["version"], 1);
    assert_eq!(
        file["appearance"], "light",
        "the stored form is the token the document element reads"
    );
}

#[test]
fn where_it_lives_when_nobody_says_is_beside_the_recent_commands_and_not_in_them() {
    let location = Settings::location().expect("a home to keep it in");

    assert!(
        location.is_absolute(),
        "not resolved against whatever directory the app was started in: {location:?}"
    );
    assert!(
        location.ends_with("acp-inspector/settings.json"),
        "under a directory of the inspector's own: {location:?}"
    );
    assert_eq!(
        Settings::load().path(),
        Some(location.as_path()),
        "and that is where the store nobody configured reads and writes"
    );

    // Its own file: the recent commands are `0600` because they carry an
    // environment, and a preference has no business inheriting that file's
    // version.
    assert_ne!(
        Some(location.as_path()),
        acp_inspector_core::RecentCommands::location().as_deref()
    );
}

/// An empty directory of this test run's own.
fn scratch(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("inspector-settings-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}
