//! The window's stylesheet, compiled into the binary.
//!
//! `sheet.css` is Tailwind and daisyUI's output, generated from
//! `style/input.css` by `npm run css` and committed; `npm run css:check` fails
//! when it is stale (`docs/adr/0003-the-stylesheet-is-generated-and-committed.md`).
//! The tests below read the source stylesheet, the file a person edits.
//!
//! The palette and type scale are the design's own
//! (`docs/adr/0009-the-window-is-the-drawing.md`). Rules the tests hold:
//!
//! - Every colour is a token; only the palette blocks at the top carry literals.
//!   The dark values appear twice (media query and `data-theme`) and must match.
//! - `theme.css` clears Tailwind's colour namespace, so no class can carry a
//!   literal colour either.
//! - Every ink meets WCAG AA (4.5:1 for text, 3:1 for boundaries) on every
//!   ground it rests on; nothing is under 11px, and nothing pressable under 12px.
//! - Nothing is known by colour alone. The two hairlines are decoration.

/// One bundled face as an `@font-face` rule with the font inlined as a `data:`
/// URL.
///
/// IBM Plex is shipped (latin subset, OFL-1.1, `style/fonts/`) so the window
/// looks the same on every machine. `font-display: block`, because the face is
/// already in the document.
macro_rules! face {
    ($family:literal, $style:literal, $weight:literal, $file:literal) => {{
        const FONT: &[u8] = include_bytes!(concat!("../style/fonts/", $file));
        const ENCODED: [u8; base64_len(FONT.len())] = base64(FONT);
        const PARTS: &[&[u8]] = &[
            concat!(
                "\n@font-face{font-family:\"",
                $family,
                "\";font-style:",
                $style,
                ";font-weight:",
                $weight,
                ";font-display:block;src:url(data:font/woff2;base64,"
            )
            .as_bytes(),
            &ENCODED,
            b") format(\"woff2\");unicode-range:U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD}",
        ];
        const RULE: [u8; total(PARTS)] = join(PARTS);
        &RULE
    }};
}

/// The generated stylesheet followed by the faces, as the root component
/// inlines them into `<style>`.
///
/// The faces are appended here rather than generated into `sheet.css`, so the
/// committed sheet stays a readable diff.
pub static SHEET: &str = joined(&JOINED);

/// The compiled sheet followed by one `@font-face` per bundled face.
const PARTS: &[&[u8]] = &[
    include_bytes!("../style/sheet.css"),
    face!(
        "IBM Plex Sans",
        "normal",
        400,
        "ibm-plex-sans-latin-400-normal.woff2"
    ),
    face!(
        "IBM Plex Sans",
        "italic",
        400,
        "ibm-plex-sans-latin-400-italic.woff2"
    ),
    face!(
        "IBM Plex Sans",
        "normal",
        500,
        "ibm-plex-sans-latin-500-normal.woff2"
    ),
    face!(
        "IBM Plex Sans",
        "normal",
        600,
        "ibm-plex-sans-latin-600-normal.woff2"
    ),
    face!(
        "IBM Plex Mono",
        "normal",
        400,
        "ibm-plex-mono-latin-400-normal.woff2"
    ),
    face!(
        "IBM Plex Mono",
        "normal",
        600,
        "ibm-plex-mono-latin-600-normal.woff2"
    ),
];

/// A static: the joined sheet is a few hundred kilobytes, and a constant would
/// be copied into every use.
static JOINED: [u8; total(PARTS)] = join(PARTS);

/// How long `bytes` bytes are once base64-encoded, padding included.
const fn base64_len(bytes: usize) -> usize {
    bytes.div_ceil(3) * 4
}

/// Standard base64 with padding, at compile time.
const fn base64<const N: usize>(bytes: &[u8]) -> [u8; N] {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = [b'='; N];
    let mut read = 0;
    let mut write = 0;
    while read < bytes.len() {
        let first = bytes[read];
        let second = if read + 1 < bytes.len() {
            bytes[read + 1]
        } else {
            0
        };
        let third = if read + 2 < bytes.len() {
            bytes[read + 2]
        } else {
            0
        };
        out[write] = ALPHABET[(first >> 2) as usize];
        out[write + 1] = ALPHABET[(((first & 0b11) << 4) | (second >> 4)) as usize];
        if read + 1 < bytes.len() {
            out[write + 2] = ALPHABET[(((second & 0b1111) << 2) | (third >> 6)) as usize];
        }
        if read + 2 < bytes.len() {
            out[write + 3] = ALPHABET[(third & 0b11_1111) as usize];
        }
        read += 3;
        write += 4;
    }
    out
}

/// The summed length of `parts`.
const fn total(parts: &[&[u8]]) -> usize {
    let mut sum = 0;
    let mut at = 0;
    while at < parts.len() {
        sum += parts[at].len();
        at += 1;
    }
    sum
}

/// `parts`, one after another.
const fn join<const N: usize>(parts: &[&[u8]]) -> [u8; N] {
    let mut out = [0; N];
    let mut written = 0;
    let mut at = 0;
    while at < parts.len() {
        let part = parts[at];
        let (_, rest) = out.split_at_mut(written);
        let (into, _) = rest.split_at_mut(part.len());
        into.copy_from_slice(part);
        written += part.len();
        at += 1;
    }
    out
}

/// The joined bytes as the text they are: a stylesheet and some base64.
const fn joined(bytes: &'static [u8]) -> &'static str {
    match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => panic!("the stylesheet is UTF-8"),
    }
}

/// The source stylesheet, `theme.css` then `input.css`, as the tests read it.
#[cfg(test)]
const SOURCE: &str = concat!(
    include_str!("../style/theme.css"),
    include_str!("../style/input.css"),
);

#[cfg(test)]
mod tests {
    use super::{SHEET, SOURCE};

    /// The declarations of the rule with this exact selector at the start of a
    /// line, at any nesting depth (some sit inside `@layer` blocks).
    fn rule(selector: &str) -> &'static str {
        let opens = format!("{selector} {{");
        SOURCE
            .match_indices(opens.as_str())
            .find(|(at, _)| {
                SOURCE[..*at]
                    .rsplit('\n')
                    .next()
                    .is_some_and(|indent| indent.chars().all(|c| c == ' '))
            })
            .map(|(at, _)| &SOURCE[at + opens.len()..])
            .unwrap_or_else(|| panic!("`{selector}` is a rule in the sheet"))
            .split_once('}')
            .expect("the rule is closed")
            .0
    }

    /// The source with its comments removed, since comments often name the very
    /// things the tests forbid a rule to use.
    fn declarations() -> String {
        let mut left = SOURCE;
        let mut kept = String::with_capacity(SOURCE.len());
        while let Some((before, rest)) = left.split_once("/*") {
            kept.push_str(before);
            left = rest.split_once("*/").expect("a comment is closed").1;
        }
        kept.push_str(left);
        kept
    }

    /// Every rule that declares something, as `(nesting, declarations)`.
    ///
    /// The nesting is outermost first and ends in the rule's own selector, e.g.
    /// `["@media (prefers-color-scheme: dark)", ":root:not(…)"]`. Blocks that
    /// only hold other blocks are not returned.
    fn rules() -> Vec<(Vec<String>, String)> {
        let source = declarations();
        let mut found = Vec::new();
        let mut nesting: Vec<String> = Vec::new();
        let mut buffer = String::new();

        for character in source.chars() {
            match character {
                // The selector is what follows the last `;` (after `@import` etc.).
                '{' => {
                    let selector = buffer.rsplit(';').next().unwrap_or(&buffer);
                    nesting.push(selector.trim().to_owned());
                    buffer.clear();
                }
                '}' => {
                    if !buffer.trim().is_empty() {
                        found.push((nesting.clone(), buffer.clone()));
                    }
                    nesting.pop();
                    buffer.clear();
                }
                _ => buffer.push(character),
            }
        }
        found
    }

    /// The selector a rule is written under, which is the last of its nesting.
    fn selector(nesting: &[String]) -> &str {
        nesting.last().map(String::as_str).unwrap_or_default()
    }

    /// The compound selector each alternative of a selector list ends in: the
    /// element the rule paints. Split at the top level only (`:is(.a, .b)`).
    fn subjects(selector: &str) -> Vec<String> {
        let mut alternatives = vec![String::new()];
        let mut depth = 0usize;
        for character in selector.chars() {
            match character {
                '(' | '[' => depth += 1,
                ')' | ']' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    alternatives.push(String::new());
                    continue;
                }
                _ => {}
            }
            alternatives
                .last_mut()
                .expect("one alternative")
                .push(character);
        }
        alternatives
            .into_iter()
            .map(|alternative| {
                let mut compounds = vec![String::new()];
                let mut depth = 0usize;
                for character in alternative.trim().chars() {
                    match character {
                        '(' | '[' => depth += 1,
                        ')' | ']' => depth = depth.saturating_sub(1),
                        ' ' | '>' | '+' | '~' if depth == 0 => {
                            compounds.push(String::new());
                            continue;
                        }
                        _ => {}
                    }
                    compounds.last_mut().expect("one compound").push(character);
                }
                compounds
                    .into_iter()
                    .rfind(|compound| !compound.is_empty())
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Every `(property, value)` a rule body declares. Per body, because a
    /// selector's pseudo-class colons would otherwise parse as properties.
    fn declarations_of(body: &str) -> Vec<(String, String)> {
        body.split(';')
            .filter_map(|declaration| declaration.split_once(':'))
            .map(|(property, value)| (property.trim().to_owned(), value.trim().to_owned()))
            .collect()
    }

    /// The value a rule gives a property, if it gives it one.
    fn declared(selector: &str, property: &str) -> Option<String> {
        declarations_of(rule(selector))
            .into_iter()
            .find(|(name, _)| name == property)
            .map(|(_, value)| value)
    }

    /// Whether a rule declares a property at all.
    fn declares(body: &str, property: &str) -> bool {
        declarations_of(body)
            .iter()
            .any(|(name, _)| name == property)
    }

    /// Every hex token one palette block declares, found by its opening line.
    fn scheme(opens: &str) -> Vec<(String, String)> {
        let source = declarations();
        let block = source
            .split_once(opens)
            .unwrap_or_else(|| panic!("`{opens}` opens a palette block"))
            .1
            .split_once("\n}")
            .expect("the palette block is closed")
            .0;

        declarations_of(block)
            .into_iter()
            .filter(|(name, value)| name.starts_with("--") && value.starts_with('#'))
            .collect()
    }

    /// One scheme's value for a token.
    fn value(scheme: &[(String, String)], token: &str) -> String {
        scheme
            .iter()
            .find(|(name, _)| name == token)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| panic!("{token} is in the palette"))
    }

    /// WCAG relative luminance of a `#rrggbb`.
    fn luminance(hex: &str) -> f64 {
        let channel = |at: usize| {
            let value =
                u8::from_str_radix(&hex[at..at + 2], 16).expect("a hex pair") as f64 / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
    }

    /// The contrast ratio between two `#rrggbb`, as WCAG defines it.
    fn contrast(first: &str, second: &str) -> f64 {
        let first = luminance(first);
        let second = luminance(second);
        (first.max(second) + 0.05) / (first.min(second) + 0.05)
    }

    /// Every declaration in the `@theme static` vocabulary block, comments
    /// removed.
    fn vocabulary() -> Vec<(String, String)> {
        declarations_of(
            declarations()
                .split_once("\n@theme static {")
                .expect("the vocabulary is one static theme block")
                .1
                .split_once("\n}")
                .expect("the block is closed")
                .0,
        )
    }

    /// Whether a namespace is cleared before it is declared into: a
    /// `--color-*: initial` after the palette would wipe the palette.
    fn cleared_first(namespace: &str) -> bool {
        let prefix = namespace.trim_end_matches('*');
        let mut declared_into = false;

        for (name, value) in vocabulary() {
            if name == namespace {
                return !declared_into && value == "initial";
            }
            declared_into |= name.starts_with(prefix);
        }
        false
    }

    /// The classes a reader presses: a rule whose subject carries one of these
    /// (or is a `summary`) is painting a control's label.
    const CONTROLS: [&str; 14] = [
        "btn",
        "chip",
        "seg-item",
        "tab",
        "rail-tab",
        "wire-tab",
        "mode-row",
        "mode-cycle",
        "palette-open",
        "to-latest",
        "recent-label",
        "slash-row",
        "palette-row",
        "frame-row",
    ];

    /// Whether a compound selector paints one of [`CONTROLS`] itself.
    fn is_control(compound: &str) -> bool {
        let element = compound.split(['.', '[', ':']).next().unwrap_or_default();
        element == "summary"
            || compound.split('.').skip(1).any(|piece| {
                let class = piece
                    .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
                    .next()
                    .unwrap_or_default();
                CONTROLS.iter().any(|control| {
                    class == *control || (*control == "btn" && class.starts_with("btn-"))
                })
            })
    }

    #[test]
    fn the_compiled_sheet_is_tailwinds_output() {
        // An empty sheet would build and open unpainted. Freshness is
        // `npm run css:check`'s job.
        assert!(SHEET.starts_with("/*! tailwindcss"), "{:.40}", SHEET);
        assert!(SHEET.contains("--color-acc:"), "the tokens are in it");
    }

    #[test]
    fn the_window_is_set_in_the_faces_it_ships() {
        // Six faces, all `data:` URLs, and the font stacks ask for them first.
        use base64::Engine as _;

        let faces = [
            (
                "IBM Plex Sans",
                "normal",
                "400",
                "ibm-plex-sans-latin-400-normal.woff2",
            ),
            (
                "IBM Plex Sans",
                "italic",
                "400",
                "ibm-plex-sans-latin-400-italic.woff2",
            ),
            (
                "IBM Plex Sans",
                "normal",
                "500",
                "ibm-plex-sans-latin-500-normal.woff2",
            ),
            (
                "IBM Plex Sans",
                "normal",
                "600",
                "ibm-plex-sans-latin-600-normal.woff2",
            ),
            (
                "IBM Plex Mono",
                "normal",
                "400",
                "ibm-plex-mono-latin-400-normal.woff2",
            ),
            (
                "IBM Plex Mono",
                "normal",
                "600",
                "ibm-plex-mono-latin-600-normal.woff2",
            ),
        ];
        assert_eq!(SHEET.matches("@font-face{").count(), faces.len());
        assert!(
            !SHEET.contains("url(http") && !SHEET.contains("url(./") && !SHEET.contains("url(/"),
            "no face is fetched"
        );
        for (family, style, weight, file) in faces {
            let path = format!("{}/style/fonts/{file}", env!("CARGO_MANIFEST_DIR"));
            let font = std::fs::read(&path).expect("the face is in the tree");
            let encoded = base64::engine::general_purpose::STANDARD.encode(&font);
            let rule = format!(
                "@font-face{{font-family:\"{family}\";font-style:{style};font-weight:{weight};font-display:block;src:url(data:font/woff2;base64,{encoded}) format(\"woff2\")"
            );
            assert!(
                SHEET.contains(&rule),
                "{family} {style} {weight} is inlined byte for byte"
            );
        }
        // A size budget, so another weight is a decision rather than drift.
        let inlined = SHEET.len() - include_str!("../style/sheet.css").len();
        assert!(inlined < 400 * 1024, "the faces add {inlined} bytes");

        for (stack, family) in [
            ("--font-sans", "\"IBM Plex Sans\", system-ui"),
            ("--font-mono", "\"IBM Plex Mono\", ui-monospace"),
        ] {
            assert!(
                vocabulary()
                    .iter()
                    .any(|(name, value)| name == stack && value.starts_with(family)),
                "{stack} asks for the bundled face first and the platform's after it"
            );
        }
        // No weight the faces do not carry; the engine would synthesise it.
        for (name, value) in vocabulary() {
            if name.starts_with("--font-weight-") {
                assert!(
                    ["400", "500", "600"].contains(&value.as_str()),
                    "{name} is {value}, which no bundled face carries"
                );
            }
        }
    }

    #[test]
    fn no_comment_in_the_source_is_closed_twice() {
        // A stray second `*/` turns prose into CSS; Tailwind then silently drops
        // the following rule.
        let source = declarations();
        assert!(
            !source.contains("*/"),
            "a comment is closed twice, which makes the prose between the two closes into CSS: {}",
            source
                .match_indices("*/")
                .map(|(at, _)| {
                    let from = source[..at].rfind('\n').map(|line| line + 1).unwrap_or(0);
                    source[from..at].trim()
                })
                .collect::<Vec<_>>()
                .join(" / ")
        );
    }

    #[test]
    fn the_document_declines_to_bounce() {
        // macOS bounces the document anyway; only the root element propagates
        // `overscroll-behavior` to the viewport.
        assert_eq!(
            declared("html", "overscroll-behavior").as_deref(),
            Some("none"),
        );
    }

    #[test]
    fn every_region_scrolls_as_a_screen_of_its_own() {
        // Otherwise a scroller at its end hands the gesture to what is behind it.
        let contained = rules()
            .into_iter()
            .find(|(_, body)| body.contains("overscroll-behavior: contain"))
            .map(|(nesting, _)| selector(&nesting).to_owned())
            .expect("the regions that scroll stop at their own end");

        for scroller in [
            ".rail-body",
            ".turn-stream",
            ".wire-scroll",
            ".frame-json",
            ".lines-scroll",
            ".palette-list",
            ".slash-list",
            ".dialog-body",
            ".quoted",
            ".disclose-body",
        ] {
            assert!(
                contained.contains(scroller),
                "{scroller} scrolls, so it stops at its own end: {contained}"
            );
        }
    }

    #[test]
    fn the_platform_draws_the_scrollbars() {
        // A skinned scrollbar is a permanent classic one on macOS; `color-scheme`
        // already darkens the platform's.
        for (nesting, body) in rules() {
            let selector = selector(&nesting);
            assert!(
                !selector.contains("::-webkit-scrollbar"),
                "`{selector}` skins the platform's scrollbar"
            );
            for property in ["scrollbar-width", "scrollbar-color"] {
                assert!(
                    !declares(&body, property),
                    "`{selector}` restyles the platform's scrollbar with `{property}`"
                );
            }
        }
    }

    #[test]
    fn what_the_reader_is_doing_is_drawn_in_the_accent() {
        // The accent marks the reader's own actions, including selected text.
        assert_eq!(
            declared("::selection", "background").as_deref(),
            Some("var(--acc-fill)"),
        );
        assert_eq!(
            declared("::selection", "color").as_deref(),
            Some("var(--acc-ink)"),
            "and the ink on it is the one the fill was given"
        );
        assert_eq!(
            declared(":is(.input, .textarea, textarea)", "caret-color").as_deref(),
            Some("var(--acc)"),
        );
        assert_eq!(
            declared("::placeholder", "color").as_deref(),
            Some("var(--dim2)"),
        );
        assert_eq!(declared("::placeholder", "opacity").as_deref(), Some("1"));
        for filled in [".btn-filled", ".mark"] {
            let body = rule(filled);
            assert!(
                body.contains("var(--acc-fill)") && body.contains("var(--acc-ink)"),
                "{filled} is the lime with dark ink: {body}"
            );
        }
    }

    #[test]
    fn the_windows_own_words_are_not_selectable_and_the_agents_are() {
        // The window's own labels are not selectable; evidence is.
        let chrome = rules()
            .into_iter()
            .find(|(_, body)| body.contains("user-select: none"))
            .map(|(nesting, _)| selector(&nesting).to_owned())
            .expect("the window's own words decline selection");

        for own in [
            ".bar",
            ".pane-head",
            ".rail-tabs",
            ".eyebrow",
            ".wire-filters",
            ".palette-foot",
            ".prompt-foot",
        ] {
            assert!(
                chrome.contains(own),
                "{own} is this window talking: {chrome}"
            );
        }
        for evidence in [".said", ".raw", ".frame-json", ".quoted", ".line-text"] {
            assert!(
                !chrome.contains(evidence),
                "{evidence} carries what crossed and stays selectable: {chrome}"
            );
        }
    }

    #[test]
    fn the_window_is_a_bar_over_a_rail_and_two_screens() {
        // A fixed bar and rail; the turn is wider than the wire.
        assert_eq!(
            declared("#main", "flex-direction").as_deref(),
            Some("column")
        );
        assert_eq!(declared(".bar", "height").as_deref(), Some("52px"));
        assert_eq!(declared(".bar", "flex").as_deref(), Some("none"));
        assert_eq!(declared(".body", "flex").as_deref(), Some("1"));
        assert_eq!(declared(".body", "min-height").as_deref(), Some("0"));
        // 244px until the reader drags its edge.
        assert_eq!(
            declared(".rail", "width").as_deref(),
            Some("var(--rail-width, 244px)")
        );
        assert_eq!(declared(".body", "position").as_deref(), Some("relative"));
        assert_eq!(declared(".rail", "flex").as_deref(), Some("none"));
        assert_eq!(declared(".split", "flex").as_deref(), Some("1"));
        assert_eq!(declared(".turns", "flex").as_deref(), Some("1.45"));
        assert_eq!(declared(".wire", "flex").as_deref(), Some("1"));
        // Columns may shrink below their content, so long strings elide.
        for column in [".rail", ".split", ".turns", ".wire"] {
            assert!(
                declared(column, "min-width").as_deref() == Some("0")
                    || declared(column, "min-height").as_deref() == Some("0"),
                "{column} shrinks rather than pushing the window wider"
            );
        }
    }

    #[test]
    fn a_window_too_narrow_for_the_rail_keeps_both_screens_and_opens_the_rail_as_a_drawer() {
        // Below 1180px the rail is a drawer over both screens, with a shade and
        // a bar button; its edge is not draggable there.
        let drawer = rules()
            .into_iter()
            .filter(|(nesting, _)| {
                nesting
                    .first()
                    .is_some_and(|at| at.contains("max-width: 1180px"))
            })
            .map(|(nesting, body)| (selector(&nesting).to_owned(), body))
            .collect::<Vec<_>>();
        let says = |subject: &str, wanted: &str| {
            drawer
                .iter()
                .any(|(selector, body)| selector == subject && body.contains(wanted))
        };
        assert!(says(".rail", "position: absolute") && says(".rail", "display: none"));
        assert!(says(".body.drawer-open .rail", "display: flex"));
        assert!(says(".body.drawer-open .rail-backdrop", "display: block"));
        assert!(says(".bar-drawer", "display: inline-flex"));
        assert!(says(".rail-resize", "display: none"));
        assert!(
            !drawer
                .iter()
                .any(|(selector, body)| selector.contains(".split")
                    && body.contains("display: none")),
            "both screens stay on screen"
        );
        assert_eq!(declared(".bar-drawer", "display").as_deref(), Some("none"));
        assert_eq!(
            declared(".rail-backdrop", "display").as_deref(),
            Some("none")
        );
    }

    #[test]
    fn a_window_too_narrow_for_three_columns_shows_one_region_at_a_time() {
        // Below 760px one region shows, chosen from a bar that is hidden at full
        // width.
        assert_eq!(declared(".mobile-bar", "display").as_deref(), Some("none"));

        let narrow = rules()
            .into_iter()
            .filter(|(nesting, _)| {
                nesting
                    .first()
                    .is_some_and(|at| at.contains("max-width: 760px"))
            })
            .map(|(nesting, body)| (selector(&nesting).to_owned(), body))
            .collect::<Vec<_>>();
        assert!(
            !narrow.is_empty(),
            "the narrow window has answers of its own"
        );

        let said = |wanted: &str| {
            narrow
                .iter()
                .any(|(selector, body)| format!("{selector} {{{body}}}").contains(wanted))
        };
        for hidden in [
            ".body.on-rail .split",
            ".body:not(.on-rail) .rail",
            ".body.on-turn .wire",
            ".body.on-wire .turns",
        ] {
            assert!(
                narrow
                    .iter()
                    .any(|(selector, body)| selector == hidden && body.contains("display: none")),
                "{hidden} gives way when the window is too narrow for three columns"
            );
        }
        assert!(
            said("display: flex"),
            "and the chooser at the foot appears with them"
        );
    }

    #[test]
    fn the_lists_that_tail_are_laid_out_bottom_up() {
        // Rows are in the document oldest first; the scroller around each list
        // is reversed, which pins it to its end without a scroll script.
        let scroller = ":is(.wire-scroll, .lines-scroll)";
        assert_eq!(
            declared(scroller, "flex-direction").as_deref(),
            Some("column-reverse"),
            "the wire log and the diagnostics scroll bottom-up"
        );
        assert_eq!(
            declared(".turn-stream", "flex-direction").as_deref(),
            Some("column-reverse"),
            "and so does the conversation"
        );
        for list in [".wire-rows", ".lines", ".stream"] {
            assert_eq!(
                declared(list, "flex-direction").as_deref(),
                Some("column"),
                "{list} is in reading order"
            );
        }
        for (nesting, body) in rules() {
            if body.contains("column-reverse") {
                assert!(
                    [scroller, ".turn-stream"].contains(&selector(&nesting)),
                    "`{}` reverses a list rather than its scroller",
                    selector(&nesting)
                );
            }
        }
        // A short wire log sits at the bottom; a short conversation reads from
        // the top.
        assert_eq!(declared(".wire-rows", "flex").as_deref(), Some("none"));
        assert_eq!(declared(".stream", "flex").as_deref(), Some("1"));
        assert_eq!(
            declared(".stream", "justify-content").as_deref(),
            Some("flex-start"),
            "and a short conversation reads from the top"
        );
    }

    #[test]
    fn the_way_back_to_the_newest_row_is_drawn_only_when_there_is_one() {
        // Positioned against the scroller's box, and hidden at the tail.
        assert_eq!(
            declared(".tailing", "position").as_deref(),
            Some("relative")
        );
        assert_eq!(
            declared(".tail-anchor", "position").as_deref(),
            Some("absolute")
        );
        assert_eq!(
            declared(".tail-anchor", "pointer-events").as_deref(),
            Some("none"),
            "the anchor is not in the way of the rows under it"
        );
        assert_eq!(
            declared(".to-latest", "pointer-events").as_deref(),
            Some("auto"),
            "and the button in it is"
        );
        let hidden = rule(
            "[data-tailed=\"true\"] + .tail-anchor .to-latest,\n.tailing:not(:has([data-tailed=\"false\"])) .to-latest",
        );
        assert!(
            hidden.contains("visibility: hidden") && hidden.contains("opacity: 0"),
            "a list at its tail offers no way back to it: {hidden}"
        );
    }

    #[test]
    fn nothing_taken_out_of_flow_is_positioned_against_the_window() {
        // Out-of-flow elements are positioned against their region, never the
        // viewport, and only these may be.
        for (nesting, body) in rules() {
            let selector = selector(&nesting).to_owned();
            let Some((_, position)) = declarations_of(&body)
                .into_iter()
                .find(|(property, _)| property == "position")
            else {
                continue;
            };
            assert_ne!(
                position, "fixed",
                "`{selector}` is positioned against the window rather than against its region"
            );
            if position == "absolute" {
                assert!(
                    // `.reach` is a visually-hidden control's clip box; the rail
                    // and its shade are the drawer, against `.body`.
                    [
                        ".tail-anchor",
                        ".slash",
                        ".reach",
                        ".rail",
                        ".body.drawer-open .rail-backdrop",
                    ]
                    .contains(&selector.as_str()),
                    "`{selector}` is out of flow and is not one of the things that may be"
                );
            }
        }
        assert_eq!(
            declared(".composer-inner", "position").as_deref(),
            Some("relative"),
            "the commands are positioned against the composer"
        );
    }

    #[test]
    fn the_frame_a_reader_selected_is_read_in_a_pane_and_not_in_the_row() {
        // The selected frame is read whole in a fixed-size pane (ADR 0009).
        assert_eq!(declared(".frame-pane", "flex").as_deref(), Some("none"));
        assert_eq!(declared(".frame-pane", "height").as_deref(), Some("44%"));
        assert_eq!(
            declared(".frame-pane", "min-height").as_deref(),
            Some("180px"),
            "a short window still reads a frame rather than a sliver of one"
        );
        assert_eq!(declared(".frame-json", "overflow").as_deref(), Some("auto"));
        assert_eq!(
            declared(".frame-json", "white-space").as_deref(),
            Some("pre"),
            "the bytes are laid out and never rewrapped"
        );
        let chosen = rule(".frame-row[aria-current=\"true\"]");
        assert!(
            chosen.contains("border-left-color: var(--acc)")
                && chosen.contains("background: var(--tint-selected)"),
            "the selected row is tinted and barred in the reader's own colour: {chosen}"
        );
    }

    #[test]
    fn the_trace_is_striped_and_says_only_what_the_width_allows() {
        // Striped rather than ruled; the connection ordinal only once there is a
        // second; the preview dropped when the wire is narrow.
        assert!(
            !declares(rule(".frame-line"), "border-bottom"),
            "a trace row is not ruled off from the next"
        );
        assert_eq!(
            declared(
                ".wire-rows > li:nth-child(even) > .frame-line",
                "background"
            )
            .as_deref(),
            Some("var(--stripe)"),
        );
        assert_eq!(
            declared(
                ".wire-rows:not(:has([data-switched=\"true\"])) .frame-crossing",
                "display"
            )
            .as_deref(),
            Some("none"),
            "`#1` says nothing until there is a `#2`"
        );
        assert_eq!(
            declared(".wire", "container-type").as_deref(),
            Some("inline-size")
        );
        assert!(
            rules().iter().any(|(nesting, body)| {
                nesting
                    .first()
                    .is_some_and(|at| at.starts_with("@container"))
                    && selector(nesting) == ".frame-peek"
                    && body.contains("display: none")
            }),
            "a narrow wire drops the preview"
        );
    }

    #[test]
    fn a_payload_is_coloured_by_what_it_is_made_of_and_marked_no_other_way() {
        // The five token classes `json.rs` emits are coloured and nothing else.
        for (part, token) in [
            (".json-key", "var(--key)"),
            (".json-string", "var(--str)"),
            (".json-number", "var(--num)"),
            (".json-literal", "var(--lit)"),
            (".json-punctuation", "var(--dim2)"),
        ] {
            assert_eq!(
                declared(part, "color").as_deref(),
                Some(token),
                "{part} is drawn in {token}"
            );
            assert_eq!(
                declarations_of(rule(part)).len(),
                1,
                "{part} says what it is with colour and nothing else"
            );
        }
    }

    #[test]
    fn an_entry_is_a_marked_gutter_and_the_content_beside_it() {
        // A gutter of marks keyed on tone, beside the content.
        assert_eq!(declared(".row", "display").as_deref(), Some("flex"));
        assert_eq!(declared(".row-mark", "width").as_deref(), Some("20px"));
        assert_eq!(declared(".row-mark", "flex").as_deref(), Some("none"));
        assert_eq!(declared(".row-body", "flex").as_deref(), Some("1"));
        assert_eq!(declared(".row-body", "min-width").as_deref(), Some("0"));

        // A row's kind changes only its mark's colour.
        for (nesting, body) in rules() {
            let selector = selector(&nesting).to_owned();
            if !selector.starts_with(".row.") || !selector.contains(".row-mark") {
                continue;
            }
            assert_eq!(
                declarations_of(&body)
                    .into_iter()
                    .map(|(property, _)| property)
                    .collect::<Vec<_>>(),
                vec!["color".to_owned()],
                "`{selector}` gives a kind more than its colour"
            );
        }
    }

    #[test]
    fn agent_provided_content_wraps_or_scrolls_on_its_local_axis() {
        // Nothing an agent controls the length of may widen the window.
        for wrapping in [
            ".said",
            ".raw",
            ".cap-name",
            ".cap-said",
            ".tile-endpoint",
            ".session-id",
            ".resource",
            ".blocking-title",
            ".blocking-meta",
            ".url-target",
            ".line .line-text",
        ] {
            let body = rule(wrapping);
            assert!(
                body.contains("overflow-wrap: anywhere") || body.contains("word-break: break-all"),
                "{wrapping} wraps rather than widening the window: {body}"
            );
        }
        let quoted = rule(".quoted");
        assert!(
            quoted.contains("max-height: 240px") && quoted.contains("overflow: auto"),
            "quoted output is bounded and scrolls on its own axis: {quoted}"
        );
        assert!(
            quoted.contains("white-space: pre-wrap"),
            "and it is drawn as it arrived: {quoted}"
        );
    }

    #[test]
    fn no_control_is_hidden_until_a_pointer_arrives() {
        // Nothing appears only on hover.
        for (nesting, body) in rules() {
            let selector = selector(&nesting).to_owned();
            if !selector.contains(":hover") {
                continue;
            }
            for hiding in ["display", "visibility", "opacity"] {
                assert!(
                    !declares(&body, hiding),
                    "`{selector}` reveals or hides something with `{hiding}`: {body}"
                );
            }
        }
    }

    #[test]
    fn every_row_a_pointer_can_be_over_says_so() {
        // Hover is a ground change; the striped trace hovers a step darker.
        for row in [".row", ".slash-row", ".palette-row"] {
            assert_eq!(
                declared(&format!("{row}:hover"), "background").as_deref(),
                Some("var(--panel2)"),
                "{row} says the pointer is over it"
            );
        }
        assert_eq!(
            declared(".wire-rows > li > .frame-line:hover", "background").as_deref(),
            Some("var(--line)"),
            "a trace row says the pointer is over it, striped or not"
        );
    }

    #[test]
    fn the_hand_cursor_is_for_links_alone() {
        // Desktop controls keep the arrow; only links get the hand (daisyUI's
        // default is overridden).
        for (nesting, body) in rules() {
            let selector = selector(&nesting).to_owned();
            if declared_value(&body, "cursor").as_deref() == Some("pointer") {
                assert_eq!(selector, "a", "`{selector}` takes the hand cursor");
            }
        }
        for control in [".btn", ".toggle", ".checkbox"] {
            assert_eq!(
                declared(control, "cursor").as_deref(),
                Some("default"),
                "{control} keeps the arrow"
            );
        }
    }

    /// The value one rule body gives a property.
    fn declared_value(body: &str, property: &str) -> Option<String> {
        declarations_of(body)
            .into_iter()
            .find(|(name, _)| name == property)
            .map(|(_, value)| value)
    }

    #[test]
    fn every_control_family_has_the_same_visible_focus_indicator() {
        let everything = ":is(.btn, .input, .textarea, .select, .toggle, .checkbox, textarea, input, button, summary, a, [tabindex=\"0\"]):focus-visible";
        let ring = rule(everything);

        assert!(
            ring.contains("outline: 2px solid var(--acc)") && ring.contains("outline-offset: 1px"),
            "one focus ring, in the reader's own colour: {ring}"
        );
        // Bare elements are included, so any `<button>` row gets the ring.
        for control in [
            ".btn",
            ".input",
            ".textarea",
            ".select",
            "button",
            "summary",
        ] {
            assert!(
                everything.contains(control),
                "{control} takes the window's one focus ring"
            );
        }
        // A focused field's edge takes the same colour as the ring.
        assert_eq!(
            declared(
                ".input:focus, .input:focus-within, .textarea:focus, .select:focus, .select:focus-within",
                "--input-color"
            )
            .as_deref(),
            Some("var(--acc)"),
        );

        // Two fields are drawn as their surface; each indicates focus there.
        let exempt = ":is(.prompt-box textarea, .palette-query):focus-visible";
        assert_eq!(
            declared(exempt, "outline").as_deref(),
            Some("none"),
            "the composer's field and the palette's question take no ring of their own"
        );
        assert_eq!(
            declared(".prompt-box:focus-within", "border-color").as_deref(),
            Some("var(--acc)"),
            "the composer says where the keyboard is on the box the field is in"
        );
        assert_eq!(
            declared(".palette-query:focus-visible", "border-bottom-color").as_deref(),
            Some("var(--acc)"),
            "and the palette says it on the rule under its question"
        );
        // No other rule may turn the ring off.
        for (nesting, body) in rules() {
            let turned_off = selector(&nesting).to_owned();
            if !turned_off.contains(":focus") || !body.contains("outline: none") {
                continue;
            }
            assert!(
                turned_off == exempt
                    // `showModal` focuses the box itself; it is not tabbable.
                    || turned_off == ".dialog-box:focus",
                "`{turned_off}` turns off the focus indicator without drawing another"
            );
        }
    }

    #[test]
    fn chosen_is_a_tint_and_a_mark_and_never_the_focus_ring() {
        // The ring is the keyboard's; chosen is a tint and a bar or check, never
        // an outline.
        for (chosen, mark) in [
            (".chip[aria-pressed=\"true\"]", None),
            (
                ".mode-row[aria-pressed=\"true\"]",
                Some("inset 2px 0 0 var(--acc)"),
            ),
            (".session-tile", Some("inset 2px 0 0 var(--acc)")),
            (".session-row.live", Some("inset 2px 0 0 var(--acc)")),
            (".palette-row[aria-selected=\"true\"]", None),
            (".slash-row[aria-selected=\"true\"]", None),
            (".frame-row[aria-current=\"true\"]", None),
        ] {
            let body = rule(chosen);
            assert!(
                body.contains("background: var(--tint-selected)"),
                "{chosen} is tinted: {body}"
            );
            assert!(
                !body.contains("outline"),
                "{chosen} draws the focus ring's shape: {body}"
            );
            if let Some(mark) = mark {
                assert!(body.contains(mark), "{chosen} carries a bar: {body}");
            }
        }
        assert!(
            rule(".chip[aria-pressed=\"true\"]::before").contains("rotate(45deg)"),
            "a chosen chip carries a check"
        );
        // An off filter has a dashed edge.
        assert_eq!(
            declared(
                ".wire-filters .chip[aria-pressed=\"false\"]",
                "border-style"
            )
            .as_deref(),
            Some("dashed"),
        );
    }

    #[test]
    fn a_reader_who_asked_for_less_motion_gets_none() {
        let reduced = rules()
            .into_iter()
            .find(|(nesting, _)| {
                nesting
                    .first()
                    .is_some_and(|at| at.contains("prefers-reduced-motion"))
            })
            .map(|(_, body)| body)
            .expect("the sheet answers a reader who asked for less motion");

        for declaration in [
            "animation-duration: 0.001ms !important",
            "animation-iteration-count: 1 !important",
            "transition-duration: 0.001ms !important",
            "scroll-behavior: auto !important",
        ] {
            assert!(
                reduced.contains(declaration),
                "reduced motion requires `{declaration}`: {reduced}"
            );
        }
    }

    #[test]
    fn the_two_things_that_move_on_their_own_move_only_when_they_mean_it() {
        // Exactly two animations (ADR 0009): arriving rows fade up, and live dots
        // blink while a turn streams.
        let keyframes = declarations()
            .match_indices("@keyframes")
            .map(|(at, _)| {
                declarations()[at..]
                    .split_whitespace()
                    .nth(1)
                    .expect("a keyframe is named")
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            keyframes,
            ["blink", "fadeup"],
            "two animations, and they are the drawing's two"
        );

        // Only rows marked as just arrived fade up, so re-shown regions do not
        // replay; dots blink only while a turn runs.
        for (nesting, body) in rules() {
            let selector = selector(&nesting).to_owned();
            if nesting.iter().any(|at| at.starts_with("@keyframes")) {
                continue;
            }
            let Some(animation) = declared_value(&body, "animation") else {
                continue;
            };
            if animation.starts_with("fadeup") {
                assert_eq!(
                    selector, ".row[data-arrived]",
                    "`{selector}` replays an arrival"
                );
            } else if animation.starts_with("blink") {
                assert!(
                    selector.starts_with("body:has([data-slot=\"turn-state\"].live)"),
                    "`{selector}` blinks while nothing is streaming"
                );
            } else {
                panic!("`{selector}` animates something of its own: {animation}");
            }
        }

        // Transitions are for interactive state only.
        for (nesting, body) in rules() {
            let selector = selector(&nesting).to_owned();
            let Some(transition) = declared_value(&body, "transition") else {
                continue;
            };
            assert!(
                transition == "none"
                    || selector.contains(":hover")
                    || selector.contains(":focus")
                    || selector.contains("aria-pressed")
                    || selector.contains(".seg-item")
                    || selector.contains(".disclosure-icon"),
                "`{selector}` transitions something that is not interactive state: {transition}"
            );
        }
    }

    #[test]
    fn the_capitals_are_one_register_and_no_wire_name_wears_them() {
        // Only `.eyebrow` is uppercase, and wire names undo it: capitals would
        // spell names the protocol does not have.
        for (nesting, body) in rules() {
            if !body.contains("text-transform: uppercase") {
                continue;
            }
            assert_eq!(
                selector(&nesting),
                ".eyebrow",
                "capitals outside the one register they belong to: {body}"
            );
            assert!(
                body.contains("font-size: var(--text-meta)")
                    && body.contains("letter-spacing: var(--tracking-label)"),
                "the section name is one size and one tracking: {body}"
            );
        }
        let wire = ".wire-name";
        assert_eq!(declared(wire, "text-transform").as_deref(), Some("none"));
        assert_eq!(
            declared(wire, "font-family").as_deref(),
            Some("var(--font-mono)"),
            "a wire name is spelled in the face the wire is read in"
        );
        let heading = ".heading";
        assert_eq!(declared(heading, "text-transform").as_deref(), Some("none"));
        assert_eq!(
            declared(heading, "font-size").as_deref(),
            Some("var(--text-row)")
        );
    }

    #[test]
    fn every_size_in_the_window_is_a_step_of_the_scale_and_none_is_below_the_floor() {
        // Five sizes, none under 11px. Dimensions of things stay written out.
        let steps = vocabulary()
            .into_iter()
            .filter(|(name, _)| name.starts_with("--text-") && !name.contains("--line-height"))
            .collect::<Vec<_>>();
        assert_eq!(steps.len(), 5, "five steps: {steps:?}");
        for (name, value) in &steps {
            let rem = value
                .strip_suffix("rem")
                .and_then(|rem| rem.parse::<f64>().ok())
                .unwrap_or_else(|| panic!("{name} is in rem: {value}"));
            assert!(rem * 16.0 >= 11.0, "{name} is {}px", rem * 16.0);
        }

        for (nesting, body) in rules() {
            for (property, value) in declarations_of(&body) {
                if property != "font-size" {
                    continue;
                }
                assert!(
                    value.starts_with("var(--text-") || value == "inherit",
                    "`{}` sets a size off the scale: {value}",
                    selector(&nesting)
                );
            }
        }
        assert_eq!(
            declared("body", "font-size").as_deref(),
            Some("var(--text-body)")
        );
        assert_eq!(
            declared(".said", "font-size").as_deref(),
            Some("var(--text-read)")
        );
        assert_eq!(
            declared(".palette-query", "font-size").as_deref(),
            Some("var(--text-ask)")
        );
    }

    #[test]
    fn a_controls_label_is_twelve_pixels_and_never_the_metadata_grey() {
        // A pressable label is at least 12px and never in the `--dim2` grey.
        for (nesting, body) in rules() {
            let selector = selector(&nesting).to_owned();
            if nesting.iter().any(|at| at.starts_with("@keyframes")) {
                continue;
            }
            if !subjects(&selector)
                .iter()
                .any(|subject| is_control(subject))
            {
                continue;
            }
            if let Some(size) = declared_value(&body, "font-size") {
                assert_ne!(
                    size, "var(--text-meta)",
                    "`{selector}` sets a control's label below 12px"
                );
            }
            assert_ne!(
                declared_value(&body, "color").as_deref(),
                Some("var(--dim2)"),
                "`{selector}` draws a control's label in the metadata grey"
            );
        }
    }

    #[test]
    fn there_is_one_button_one_tab_one_chip_and_one_tag() {
        // One button style (sans, 12px, 500), including the form library's
        // presence buttons. Monospace is for data.
        for button in [".btn-quiet", ".btn-filled"] {
            assert_eq!(
                declared(button, "font-family").as_deref(),
                Some("var(--font-sans)")
            );
            assert_eq!(
                declared(button, "font-size").as_deref(),
                Some("var(--text-row)")
            );
            assert_eq!(
                declared(button, "font-weight").as_deref(),
                Some("var(--font-weight-label)")
            );
            assert!(!declares(rule(button), "text-transform"));
        }
        let tab = ".tab";
        assert_eq!(
            declared(tab, "font-size").as_deref(),
            Some("var(--text-row)")
        );
        assert_eq!(
            declared(&format!("{tab}[aria-selected=\"true\"]"), "box-shadow").as_deref(),
            Some("inset 0 -2px 0 var(--acc)"),
        );
        for (nesting, _) in rules() {
            let selector = selector(&nesting);
            // Regions may lay out their tabs, but tab states are shared.
            assert!(
                ![".rail-tab[", ".rail-tab:", ".wire-tab[", ".wire-tab:"]
                    .iter()
                    .any(|state| selector.starts_with(state)),
                "`{selector}` draws one region's tabs differently from the other's"
            );
        }
        // One tag, whose tone is a variable.
        let tag = ":is(.tag, .frame-tag)";
        assert_eq!(declared(tag, "--tone").as_deref(), Some("var(--dim)"));
        assert_eq!(declared(tag, "color").as_deref(), Some("var(--tone)"));
    }

    #[test]
    fn every_corner_is_one_of_four() {
        // Four corners (mark, control, card, dialog) and the pill.
        let corners = [
            ("--radius-mark", "0.25rem"),
            ("--radius-control", "0.375rem"),
            ("--radius-card", "0.625rem"),
            ("--radius-dialog", "0.875rem"),
            ("--radius-pill", "9999px"),
        ];
        let declared_corners = vocabulary()
            .into_iter()
            .filter(|(name, _)| name.starts_with("--radius-"))
            .collect::<Vec<_>>();
        assert_eq!(
            declared_corners,
            corners
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect::<Vec<_>>(),
            "four corners and the pill"
        );
        // daisyUI's three radius names are set per theme, to the same corners.
        for (daisy, ours) in [
            ("--radius-selector", "0.25rem"),
            ("--radius-field", "0.375rem"),
            ("--radius-box", "0.625rem"),
        ] {
            assert!(
                !vocabulary().iter().any(|(name, _)| name == daisy),
                "{daisy} is daisyUI's name for a corner and not this window's"
            );
            assert_eq!(
                SOURCE.matches(&format!("{daisy}: {ours};")).count(),
                2,
                "both themes give {daisy} one of the four corners"
            );
        }

        for (nesting, body) in rules() {
            if nesting.iter().any(|at| at.contains("@plugin")) {
                continue;
            }
            for (property, value) in declarations_of(&body) {
                if !property.ends_with("radius") {
                    continue;
                }
                assert!(
                    value.starts_with("var(--radius-") || value == "0" || value == "50%",
                    "`{}` rounds `{property}` off the four: {value}",
                    selector(&nesting)
                );
            }
        }
    }

    #[test]
    fn every_space_is_on_the_four_pixel_grid() {
        // Spacing is on a 4px grid, with 2px steps below 8 and 1px for hairlines.
        let spacing = |property: &str| {
            property.starts_with("padding")
                || property.starts_with("margin")
                || property == "gap"
                || property == "row-gap"
                || property == "column-gap"
        };
        for (nesting, body) in rules() {
            if nesting.iter().any(|at| at.starts_with("@keyframes")) {
                continue;
            }
            for (property, value) in declarations_of(&body) {
                if !spacing(&property) {
                    continue;
                }
                for length in value.split_whitespace() {
                    let Some(pixels) = length.strip_suffix("px") else {
                        continue;
                    };
                    let pixels = pixels
                        .trim_start_matches('-')
                        .parse::<u32>()
                        .unwrap_or_else(|_| panic!("`{length}` is whole pixels"));
                    assert!(
                        pixels % 4 == 0 || (pixels < 8 && pixels % 2 == 0) || pixels == 1,
                        "`{}` puts `{property}` off the grid: {value}",
                        selector(&nesting)
                    );
                }
            }
        }
    }

    #[test]
    fn the_controls_are_the_heights_of_the_grid() {
        // Held here so a control cannot quietly shrink.
        for (selector, height) in [
            (".bar", "52px"),
            (".pane-head", "40px"),
            (".frame-pane-head", "36px"),
            (".palette-query", "48px"),
            (".tab", "32px"),
            (".seg-item", "24px"),
            (".mode-row", "32px"),
            (".mode-cycle", "28px"),
            (".palette-open", "32px"),
            (".chip", "24px"),
            (".dot", "8px"),
            (".tile-actions .btn, .rail-head .btn", "24px"),
            (".pane-head .btn", "28px"),
        ] {
            assert_eq!(
                declared(selector, "height").as_deref(),
                Some(height),
                "{selector} is {height}"
            );
        }
        for (size, height) in [("xs", "1.5rem"), ("sm", "1.75rem"), ("md", "2rem")] {
            assert_eq!(
                declared(
                    &format!(".btn-{size}, .input-{size}, .select-{size}"),
                    "--size"
                )
                .as_deref(),
                Some(height)
            );
        }
    }

    #[test]
    fn the_surfaces_that_overlap_the_window_are_the_only_lifted_things() {
        // Only vocabulary shadows lift; rings and inset bars are not lifts.
        for (nesting, body) in rules() {
            let selector = selector(&nesting).to_owned();
            for (property, value) in declarations_of(&body) {
                if property != "box-shadow" || value == "none" {
                    continue;
                }
                assert!(
                    value.starts_with("var(--shadow-")
                        || value.starts_with("inset")
                        || value.starts_with("0 0 0"),
                    "`{selector}` lifts something with a shadow of its own: {value}"
                );
            }
        }
        for lifted in [".slash", ".to-latest", ".dialog-box", ".toast .alert"] {
            assert!(
                rule(lifted).contains("var(--shadow-"),
                "{lifted} is over the window and says so"
            );
        }
    }

    #[test]
    fn a_request_the_turn_stopped_on_is_ringed_in_the_tone_of_what_it_is() {
        // Ringed while waiting (permission `--in`, elicitation `--vio`); the ring
        // goes once answered.
        assert!(
            rule(".blocking").contains("var(--in)") && rule(".blocking").contains("box-shadow"),
            "a request that is waiting is ringed"
        );
        assert!(
            rule(".blocking.elicit").contains("var(--vio)"),
            "and an elicitation is ringed as the other kind of ask"
        );
        assert_eq!(
            declared(".blocking.answered", "box-shadow").as_deref(),
            Some("none"),
            "a request that has been answered is no longer waiting on anybody"
        );
    }

    #[test]
    fn the_evidence_cards_say_what_they_are_quietly() {
        // Diff sides are washes; an unrecognized row tones its label only; a
        // turn's end is a rule, not a box.
        for (side, tone) in [("removed", "--err"), ("added", "--ok")] {
            let body = rule(&format!(".diff-version.{side} .quoted"));
            assert!(
                body.contains(&format!("color-mix(in oklab, var({tone})"))
                    && body.contains(&format!("inset 2px 0 0 var({tone})")),
                "the {side} side is a wash and an edge in {tone}: {body}"
            );
        }
        assert_eq!(
            declared("p.unnamed", "color").as_deref(),
            Some("var(--txt)")
        );
        assert_eq!(
            declared(".row.unnamed .row-head > .wire-name", "color").as_deref(),
            Some("var(--vio)")
        );
        let ended = rule(".ended");
        assert!(
            !ended.contains("border:") && !ended.contains("background"),
            "the end of a turn is not a box: {ended}"
        );
    }

    #[test]
    fn the_composer_is_one_bounded_field_with_its_footer_inside_it() {
        // One bounded box; the textarea inside draws no boundary of its own.
        assert!(
            rule(".prompt-box").contains("border: 1px solid var(--line2)"),
            "the composer is one bounded field"
        );
        assert_eq!(
            declared(".prompt-box:focus-within", "border-color").as_deref(),
            Some("var(--acc)"),
            "and it says when the keyboard is in it"
        );
        let field = rule(".prompt-box textarea");
        assert!(
            field.contains("border: none")
                && field.contains("outline: none")
                && field.contains("background: transparent"),
            "the field inside it draws no second box: {field}"
        );
        assert!(
            field.contains("resize: none") && field.contains("field-sizing: content"),
            "it grows with what is typed rather than being dragged: {field}"
        );
        assert_eq!(
            declared(".prompt-box textarea", "min-height").as_deref(),
            Some("44px"),
            "and empty, it is two lines rather than three (D14)"
        );
        assert!(
            rule(".prompt-foot").contains("border-top: 1px solid var(--line)"),
            "and the footer is a band of the same box"
        );
    }

    #[test]
    fn an_empty_screen_is_centred_in_the_room_it_has() {
        let empty = rule(".empty");
        for declaration in [
            "align-items: center",
            "justify-content: center",
            "text-align: center",
            "text-wrap: balance",
        ] {
            assert!(
                empty.contains(declaration),
                "an empty screen requires `{declaration}`: {empty}"
            );
        }
    }

    #[test]
    fn no_rule_singles_out_one_advertisers_claims() {
        // The agent's and the client's accounts are drawn identically.
        for (nesting, _) in rules() {
            let selector = selector(&nesting);
            assert!(
                !selector.contains("data-advertiser") && !selector.contains("data-identity"),
                "`{selector}` draws one advertiser's claims differently from the other's"
            );
        }
    }

    #[test]
    fn every_class_the_sheet_styles_is_one_the_markup_uses() {
        // No dead rules: every class a selector names appears in the crate's
        // source (comments included), literally or as a composed prefix.
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut source = String::new();
        let mut pending = vec![std::path::PathBuf::from(root)];
        while let Some(path) = pending.pop() {
            if path.is_dir() {
                for entry in std::fs::read_dir(&path).expect("the source is readable") {
                    pending.push(entry.expect("an entry").path());
                }
            } else if path
                .extension()
                .is_some_and(|ext| ext == "rs" || ext == "js")
                && !path.ends_with("style.rs")
            {
                source.push_str(&std::fs::read_to_string(&path).expect("UTF-8 source"));
            }
        }
        let used = |class: &str| {
            source.match_indices(class).any(|(at, _)| {
                let before = source[..at].chars().next_back();
                let after = source[at + class.len()..].chars().next();
                !matches!(before, Some(c) if c.is_alphanumeric() || c == '-' || c == '_')
                    && !matches!(after, Some(c) if c.is_alphanumeric() || c == '-' || c == '_')
            })
        };
        // Offered but not yet used, and composed at run time.
        let offered = ["tag-ok", "tag-err", "tag-vio", "tag-in", "tag-out"];
        let composed = ["dot-"];

        let mut dead = Vec::new();
        for (nesting, _) in rules() {
            let selector = selector(&nesting);
            if selector.starts_with('@') || nesting.iter().any(|at| at.contains("@plugin")) {
                continue;
            }
            let mut rest = selector;
            while let Some(at) = rest.find('.') {
                rest = &rest[at + 1..];
                let class = rest
                    .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
                    .next()
                    .unwrap_or_default();
                if class.is_empty()
                    || class.chars().next().is_some_and(|c| c.is_ascii_digit())
                    || offered.contains(&class)
                    || composed.iter().any(|prefix| class.starts_with(prefix))
                    || used(class)
                {
                    continue;
                }
                dead.push(format!("{class} (in `{selector}`)"));
            }
        }
        dead.sort();
        dead.dedup();
        assert!(
            dead.is_empty(),
            "rules for classes nothing renders: {dead:#?}"
        );
    }

    #[test]
    fn the_vocabulary_is_emitted_whether_or_not_a_utility_uses_it() {
        // Without `static`, Tailwind drops variables used only through `var()`.
        assert!(
            SOURCE.contains("\n@theme static {"),
            "the vocabulary is one static theme block"
        );
    }

    #[test]
    fn a_utility_can_paint_with_no_colour_but_this_windows_own() {
        // Clearing the default palette makes classes like `bg-red-500` not
        // exist; the literal test below cannot see Rust files.
        assert!(
            cleared_first("--color-*"),
            "the default palette is not cleared before ours is declared"
        );

        for (name, value) in vocabulary() {
            if !name.starts_with("--color-") || name == "--color-*" {
                continue;
            }
            assert!(
                value.starts_with("var(--")
                    || matches!(value.as_str(), "transparent" | "currentColor"),
                "{name} is painted from something other than a token: {value}"
            );
        }
    }

    #[test]
    fn production_uses_only_the_custom_unprefixed_inspector_themes() {
        let declarations = declarations();

        assert!(
            declarations.contains("@plugin \"daisyui\"")
                && declarations.contains("themes: false")
                && declarations.contains("name: \"inspector-light\"")
                && declarations.contains("name: \"inspector-dark\"")
                && declarations.contains("default: true")
                && declarations.contains("prefersdark: true")
                && !declarations.contains("prefix:"),
            "production selects only the unprefixed Inspector themes at the Appearance seam"
        );
        // daisyUI's `list` is deliberately excluded: every list here has its own
        // layout.
        assert!(
            !declarations.contains(" list,"),
            "daisyUI's list is not included"
        );
        assert!(
            SHEET.contains(".btn")
                && SHEET.contains(".input")
                && SHEET.contains(".select")
                && SHEET.contains(".textarea")
                && SHEET.contains(".badge")
                && SHEET.contains(".alert")
                && SHEET.contains(".toggle")
                && SHEET.contains(".toast")
                && SHEET.contains(".modal")
                && SHEET.contains("[data-theme=\"inspector-light\"]")
                && SHEET.contains("[data-theme=\"inspector-dark\"]")
                && SHEET.contains("@media (prefers-color-scheme: dark)")
                && SHEET.contains(":root:not([data-theme])"),
            "the committed stylesheet contains both explicit themes and OS-driven System"
        );
    }

    #[test]
    fn the_palette_is_declared_once_and_the_themes_point_at_it() {
        // The daisyUI themes carry no colours of their own, only `var()`s to the
        // palette.
        for name in ["inspector-light", "inspector-dark"] {
            let opens = format!("name: \"{name}\";");
            let theme = declarations()
                .split_once(opens.as_str())
                .expect("both themes are declared")
                .1
                .split_once("\n}")
                .expect("the theme block is closed")
                .0
                .to_owned();
            let roles = declarations_of(&theme)
                .into_iter()
                .filter(|(property, _)| property.starts_with("--color-"))
                .collect::<Vec<_>>();
            assert_eq!(roles.len(), 20, "{name} gives every daisyUI role a value");
            for (role, value) in roles {
                assert!(
                    value.starts_with("var(--") && !value.contains('#'),
                    "{name} {role} is a value of its own rather than the palette's: {value}"
                );
            }
        }
    }

    #[test]
    fn the_themes_carry_the_windows_control_unit() {
        // The control unit is on the 4px grid, and controls are flat.
        for name in ["inspector-light", "inspector-dark"] {
            let opens = format!("name: \"{name}\";");
            let theme = SOURCE
                .split_once(opens.as_str())
                .expect("both themes are declared")
                .1
                .split_once("\n}")
                .expect("the theme block is closed")
                .0;
            for declaration in [
                "--size-field: 0.25rem",
                "--size-selector: 0.203125rem",
                "--radius-selector: 0.25rem",
                "--radius-field: 0.375rem",
                "--radius-box: 0.625rem",
                "--border: 1px",
                "--depth: 0",
                "--noise: 0",
            ] {
                assert!(
                    theme.contains(declaration),
                    "{name} must provide `{declaration}` to daisyUI components"
                );
            }
        }
    }

    #[test]
    fn daisyui_components_are_repainted_in_daisyuis_own_base_layer() {
        // daisyUI draws bases in `daisyui.l1.l2.l3` and variants in
        // `daisyui.l1.l2`. Repaints go in the base layer so variants still win;
        // an unlayered repaint would outrank them.
        for layer in ["@layer daisyui.l1.l2.l3", "@layer daisyui.l1.l2 "] {
            assert!(
                SHEET.contains(layer),
                "daisyUI no longer draws into `{layer}`, and the repaints are in the wrong layer"
            );
        }
        let in_base = |nesting: &[String]| {
            nesting.len() == 3
                && nesting[0] == "@layer utilities"
                && nesting[1] == "@layer daisyui.l1.l2.l3"
        };
        for component in [
            ".btn",
            ".input, .textarea, .select",
            ".badge",
            ".toggle",
            ".checkbox",
        ] {
            let found = rules()
                .into_iter()
                .find(|(nesting, _)| selector(nesting) == component)
                .unwrap_or_else(|| panic!("`{component}` is repainted"));
            assert!(
                in_base(&found.0),
                "`{component}` is repainted outside daisyUI's base layer: {:?}",
                found.0
            );
        }
        for (nesting, _) in rules() {
            if nesting.iter().any(|at| at.starts_with("@layer")) {
                continue;
            }
            let selector = selector(&nesting);
            for component in [
                ".badge",
                ".input",
                ".select",
                ".textarea",
                ".checkbox",
                ".toggle",
            ] {
                assert!(
                    selector.contains(' ') && !selector.contains(", ")
                        || !subjects(selector)
                            .iter()
                            .any(|subject| subject == component),
                    "`{selector}` repaints {component} outside daisyUI's layers"
                );
            }
        }

        // Repaints set paint only; geometry and states stay daisyUI's.
        let paint = [
            "background",
            "background-color",
            "color",
            "border-color",
            "border-radius",
            "box-shadow",
            "font-family",
            "font-size",
            "font-weight",
            "font-variant-numeric",
            "letter-spacing",
            "text-decoration",
            "opacity",
            "cursor",
            "caret-color",
        ];
        for component in [
            ".btn",
            ".input, .textarea, .select",
            ".badge",
            ".toggle",
            ".checkbox",
        ] {
            for (property, _) in declarations_of(rule(component)) {
                assert!(
                    property.starts_with("--") || paint.contains(&property.as_str()),
                    "`{component}` declares `{property}`, which is daisyUI's to decide"
                );
            }
        }
        // The badge's colour is its variant's.
        for property in ["color", "background", "background-color", "border-color"] {
            assert!(
                !declares(rule(".badge"), property),
                "the badge repaint decides its `{property}`, which is the tone's"
            );
        }
        // `.alert` is redrawn as a hairline box rather than daisyUI's fill.
        let alert = rule(".alert");
        assert!(
            alert.contains("border: 1px solid var(--line)")
                && alert.contains("background: var(--panel2)")
                && alert.contains("box-shadow: none"),
            "the notice is a hairline box rather than a fill: {alert}"
        );
    }

    #[test]
    fn dark_is_written_twice_and_the_two_are_the_same_palette() {
        // System dark (media query) and explicit Dark (attribute) are two
        // copies of one palette, so they must match.
        let system = scheme("\n  :root:not([data-theme]) {");
        let chosen = scheme("\n[data-theme=\"inspector-dark\"] {");

        assert!(!system.is_empty(), "System's dark palette is declared");
        assert_eq!(
            system, chosen,
            "the two ways of asking for dark are the same palette"
        );
    }

    #[test]
    fn every_ink_clears_the_text_floor_on_every_ground() {
        // WCAG AA: 4.5:1 for every ink on every ground, with no exceptions.
        const GROUNDS: [&str; 4] = ["--bg", "--panel", "--panel2", "--sunk"];
        const INK: [&str; 13] = [
            "--txt", "--dim", "--dim2", "--acc", "--out", "--in", "--ok", "--err", "--vio",
            "--key", "--str", "--num", "--lit",
        ];

        for (name, palette) in [
            ("light", scheme("\n:root {")),
            ("dark", scheme("\n[data-theme=\"inspector-dark\"] {")),
        ] {
            for token in INK {
                let worst = GROUNDS
                    .into_iter()
                    .map(|ground| contrast(&value(&palette, token), &value(&palette, ground)))
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    worst >= 4.5,
                    "{name} {token} is text at {worst:.2}:1 on its tightest ground"
                );
            }

            assert!(
                contrast(
                    &value(&palette, "--acc-ink"),
                    &value(&palette, "--acc-fill")
                ) >= 4.5,
                "{name} the ink on a filled control clears the text floor"
            );
            for hue in ["--dim", "--vio", "--out", "--ok", "--in", "--err"] {
                assert!(
                    contrast(&value(&palette, "--on-hue"), &value(&palette, hue)) >= 4.5,
                    "{name} the ink on a filled {hue} clears the text floor"
                );
            }
        }

        // A payload's strings must be distinguishable from the accent.
        let dark = scheme("\n[data-theme=\"inspector-dark\"] {");
        assert_ne!(value(&dark, "--str"), value(&dark, "--acc"));
        assert!(
            contrast(&value(&dark, "--str"), &value(&dark, "--acc")) > 1.2,
            "a string in a payload is told apart from the accent"
        );
    }

    #[test]
    fn no_rule_outside_the_palette_blocks_carries_a_colour_literal() {
        // A literal outside the three palette blocks would work in only one
        // scheme.
        for (nesting, body) in rules() {
            let selector = selector(&nesting).to_owned();
            if selector.contains("@theme")
                || selector == ":root"
                || selector == ":root:not([data-theme])"
                || selector == "[data-theme=\"inspector-dark\"]"
            {
                continue;
            }
            for (property, value) in declarations_of(&body) {
                assert!(
                    !value.contains('#')
                        && !value.contains("rgb(")
                        && !value.contains("rgba(")
                        && !value.contains("hsl(")
                        && !value.contains("oklch(")
                        && !value.contains("oklab("),
                    "`{selector}` paints `{property}` with a literal: {value}"
                );
            }
        }
    }
}
