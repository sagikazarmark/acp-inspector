//! The indentation preference (`docs/architecture.md` §8, §9): what indenting a
//! payload does to it, what it refuses to do, and where the choice is kept
//! between runs.
//!
//! Driven through the file rather than around it, for the reason
//! `tests/appearance.rs` is: the whole promise of a preference is that it
//! survives the process that made it, so every test that says "across restarts"
//! loads a second store from the same path.
//!
//! None of it needs a window. What indenting does to a frame is a fact about the
//! bytes, and the desktop crate only draws what it answers.

use std::path::PathBuf;

use acp_inspector_core::{Appearance, Indentation, Settings};

#[test]
fn tokens_round_trip() {
    for indentation in Indentation::ALL {
        assert_eq!(Indentation::from_token(indentation.token()), indentation);

        let stored = serde_json::to_string(&indentation).expect("an indentation serializes");
        assert_eq!(stored, format!("\"{}\"", indentation.token()));
    }
}

#[test]
fn a_token_nobody_here_has_heard_of_is_the_wire() {
    // A value written by a future version, and the absence of one. Neither is a
    // reason to draw a reader something other than what crossed, which is the
    // answer every version of this tool can give honestly (§8).
    assert_eq!(Indentation::from_token("tabs"), Indentation::Wire);
    assert_eq!(Indentation::from_token(""), Indentation::Wire);
    assert_eq!(Indentation::default(), Indentation::Wire);
    assert_eq!(Indentation::ALL[0], Indentation::Wire);
}

#[test]
fn the_switch_reads_both_ways() {
    assert_eq!(Indentation::of(true), Indentation::Indented);
    assert_eq!(Indentation::of(false), Indentation::Wire);
    assert!(Indentation::Indented.indented());
    assert!(!Indentation::Wire.indented());
}

/// The whole of what the default does, which is nothing.
#[test]
fn the_wire_draws_what_crossed() {
    for payload in [
        r#"{"jsonrpc":"2.0","id":1,"method":"session/prompt"}"#,
        "not json at all",
        "",
    ] {
        assert_eq!(Indentation::Wire.draw(payload), payload);
    }
}

#[test]
fn indenting_lays_a_frame_out_without_editing_it() {
    let frame = r#"{"jsonrpc":"2.0","id":1,"method":"session/prompt","params":{"sessionId":"s1","prompt":[{"type":"text","text":"hi"}]}}"#;

    let drawn = Indentation::Indented.draw(frame);

    assert_eq!(
        drawn,
        concat!(
            "{\n",
            "  \"jsonrpc\": \"2.0\",\n",
            "  \"id\": 1,\n",
            "  \"method\": \"session/prompt\",\n",
            "  \"params\": {\n",
            "    \"sessionId\": \"s1\",\n",
            "    \"prompt\": [\n",
            "      {\n",
            "        \"type\": \"text\",\n",
            "        \"text\": \"hi\"\n",
            "      }\n",
            "    ]\n",
            "  }\n",
            "}"
        )
    );

    // The claim the layout is allowed to make, stated as the thing that is
    // actually true of it: the same bytes, in the same order, with whitespace
    // between them. A decode-and-print would sort `jsonrpc` after `id` and this
    // is what says it did not.
    assert_eq!(
        drawn.split_whitespace().collect::<String>(),
        frame.split_whitespace().collect::<String>(),
        "indenting moved the bytes apart and changed none of them"
    );
}

#[test]
fn what_a_decoder_would_quietly_edit_is_left_alone() {
    // Every one of these survives a round trip through nothing and none of them
    // survives a round trip through a JSON value: keys sort, duplicates
    // collapse, numbers are respelled and escapes are normalised. An inspector
    // that did any of it would be showing its own work (§8).
    for (payload, drawn) in [
        (
            r#"{"zeta":1,"alpha":2}"#,
            "{\n  \"zeta\": 1,\n  \"alpha\": 2\n}",
        ),
        (r#"{"id":1,"id":2}"#, "{\n  \"id\": 1,\n  \"id\": 2\n}"),
        (
            r#"{"kept":1.50,"long":100000000000000000000000000000}"#,
            "{\n  \"kept\": 1.50,\n  \"long\": 100000000000000000000000000000\n}",
        ),
        (
            r#"{"escaped":"\u0041\n"}"#,
            "{\n  \"escaped\": \"\\u0041\\n\"\n}",
        ),
    ] {
        assert_eq!(Indentation::Indented.draw(payload), drawn, "{payload}");
    }
}

#[test]
fn a_payload_that_is_not_json_is_drawn_as_itself() {
    // A frame can be anything, which is the rule this preference is written
    // under rather than around (§8): the transport's own noise, a half a frame,
    // an agent that wrote a line of English to a JSON-RPC pipe. Every one is
    // evidence, and none of them is reformatted, refused or annotated.
    for payload in [
        "not json at all",
        "",
        "   ",
        r#"{"jsonrpc":"2.0","#,
        r#"{"jsonrpc":"2.0"} and then some"#,
        "<!DOCTYPE html>",
    ] {
        assert_eq!(
            Indentation::Indented.draw(payload),
            payload,
            "{payload} is not JSON, so it is itself"
        );
    }
}

#[test]
fn a_bare_value_is_a_document_and_an_empty_one_stays_on_its_line() {
    for (payload, drawn) in [
        ("42", "42"),
        (r#""a bare string""#, r#""a bare string""#),
        ("{}", "{}"),
        ("[ ]", "[]"),
        (
            r#"{"empty":{},"none":[]}"#,
            "{\n  \"empty\": {},\n  \"none\": []\n}",
        ),
    ] {
        assert_eq!(Indentation::Indented.draw(payload), drawn, "{payload}");
    }
}

#[test]
fn a_choice_is_still_in_effect_after_a_restart() {
    let path = scratch("restart").join("settings.json");
    let settings = Settings::load_from(&path);
    assert_eq!(
        settings.indentation(),
        Indentation::Wire,
        "nothing has been chosen yet, and the wire is what the tool is for"
    );

    settings
        .set_indentation(Indentation::Indented)
        .expect("somewhere to write");

    // The store the next launch of the app would build: same path, new value,
    // nothing carried over in memory.
    let reopened = Settings::load_from(&path);
    assert_eq!(reopened.indentation(), Indentation::Indented);

    // And switching back is a choice like the other one, not a forgetting.
    reopened
        .set_indentation(Indentation::Wire)
        .expect("somewhere to write");
    assert_eq!(Settings::load_from(&path).indentation(), Indentation::Wire);
}

#[test]
fn the_two_preferences_are_kept_in_one_file_without_taking_each_others_place() {
    let path = scratch("both").join("settings.json");
    let settings = Settings::load_from(&path);

    settings
        .set_appearance(Appearance::Dark)
        .expect("somewhere to write");
    settings
        .set_indentation(Indentation::Indented)
        .expect("somewhere to write");

    let reopened = Settings::load_from(&path);
    assert_eq!(
        reopened.appearance(),
        Appearance::Dark,
        "writing one preference is not a way of forgetting the other"
    );
    assert_eq!(reopened.indentation(), Indentation::Indented);

    // And the other order, because a save writes the pair either way.
    reopened
        .set_appearance(Appearance::Light)
        .expect("somewhere to write");
    let again = Settings::load_from(&path);
    assert_eq!(again.appearance(), Appearance::Light);
    assert_eq!(again.indentation(), Indentation::Indented);
}

#[test]
fn a_file_written_before_this_preference_existed_is_still_the_file() {
    // The shape the last version wrote: this version's format and version, and
    // no indentation at all. It is a readable file, which is what makes adding a
    // preference cost no version bump — and what it does not name is the wire.
    let path = scratch("older").join("settings.json");
    std::fs::write(
        &path,
        r#"{"format":"acp-inspector-settings","version":1,"appearance":"dark"}"#,
    )
    .expect("a file to write");

    let settings = Settings::load_from(&path);
    assert_eq!(settings.appearance(), Appearance::Dark, "still honoured");
    assert_eq!(settings.indentation(), Indentation::Wire);
}

#[test]
fn a_file_this_never_wrote_leaves_the_window_on_the_wire() {
    for (name, contents) in [
        ("unreadable", "{ this is not the file we left }".to_owned()),
        (
            "foreign",
            r#"{"format":"something-else","version":1,"indentation":"indented"}"#.to_owned(),
        ),
        (
            "future",
            format!(
                r#"{{"format":"acp-inspector-settings","version":{},"indentation":"indented"}}"#,
                u32::MAX
            ),
        ),
        (
            "unknown-value",
            r#"{"format":"acp-inspector-settings","version":1,"indentation":"tabs"}"#.to_owned(),
        ),
    ] {
        let path = scratch(name).join("settings.json");
        std::fs::write(&path, contents).expect("a file to write");

        assert_eq!(
            Settings::load_from(&path).indentation(),
            Indentation::Wire,
            "{name}: a file this cannot read is no preference at all"
        );
    }
}

#[test]
fn a_preference_with_nowhere_to_live_still_applies_for_the_session() {
    let blocked = scratch("blocked").join("in-the-way");
    std::fs::write(&blocked, "not a directory").expect("a file to write");

    let settings = Settings::load_from(blocked.join("settings.json"));
    let saved = settings.set_indentation(Indentation::Indented);

    assert!(saved.is_err(), "there is nowhere to write it: {saved:?}");
    assert_eq!(
        settings.indentation(),
        Indentation::Indented,
        "and this run draws it indented regardless"
    );
}

#[test]
fn the_file_is_the_shape_the_docs_describe() {
    let path = scratch("shape").join("settings.json");
    let settings = Settings::load_from(&path);
    settings
        .set_indentation(Indentation::Indented)
        .expect("somewhere to write");

    let written = std::fs::read_to_string(&path).expect("the file it wrote");
    let file: serde_json::Value = serde_json::from_str(&written).expect("one JSON document");

    assert_eq!(file["format"], "acp-inspector-settings");
    assert_eq!(file["version"], 1, "a new field is not a new shape");
    assert_eq!(file["indentation"], "indented");
    assert_eq!(
        file["appearance"], "system",
        "the preference nobody touched is written as what it is"
    );
}

/// An empty directory of this test run's own.
fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "inspector-indentation-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}
