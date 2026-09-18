# Desktop daisyUI smoke matrix

This is the integrated verification contract for the inspector's desktop surface. It complements
the core integration suite: core proves what crosses the wire and enters each store; desktop tests
prove what those facts render as; the stylesheet tests prove the computed theme inputs, contrast,
focus, motion and narrow-window rules that SSR cannot lay out.

The two required window sizes are the configured minimum, **960 x 640**, and normal desktop size,
**1440 x 880**. Run every visual row under explicit **Inspector Light**, explicit **Inspector
Dark**, and **System** with the operating system in both light and dark mode. System must leave
`data-theme` absent; Light must set `inspector-light`; Dark must set `inspector-dark`.

## Automated matrix

| Contract | Evidence |
|---|---|
| Shell, top bar, the tool's own mark and name once, explicit dot-plus-text Connection states, the protocol and Session facts, the spine, and one connection control that is Connect… or Disconnect and never a refusal | `crates/app/src/main.rs::tests` |
| Appearance values, persistence and System fallback | `crates/core/tests/appearance.rs`, `crates/app/src/appearance.rs::tests` |
| Indentation persistence, whitespace-only layout, non-JSON payloads left alone, and the switch reaching both screens | `crates/core/tests/indentation.rs`, `crates/app/src/{indent,trace,timeline,update}.rs::tests` |
| Inspector Light, Inspector Dark and preferred-dark System selectors | `style::tests::production_uses_only_the_custom_unprefixed_inspector_themes` |
| Compact daisyUI field, radius, border and elevation theme values | `style::tests::inspector_themes_own_compact_ordinary_component_geometry` |
| 14px conversation, 13px ordinary content, 12px evidence, 11px metadata and 10px capitals in one register only | `style::tests::inspector_text_roles_keep_the_cross_surface_scale` |
| Theme text, semantic marks, boundaries, hover ground, focus and primary fill contrast | `style::tests::inspector_light_and_dark_clear_the_inspector_contrast_floors` |
| 44px toolbar, a 248–296px rail, the two screens beside it and every region an inset card | `style::tests::{the_window_is_a_toolbar_over_a_rail_and_two_screens,every_region_is_an_inset_card_on_the_chrome_ground,every_region_scrolls_as_a_screen_of_its_own}`, `crates/app/src/rail.rs::tests` |
| Consistent compact theme-aware Heroicons Outline rendering, 28px icon-only targets, names, tooltips and non-nested disclosure actions | `style::tests::{heroicons_share_one_compact_theme_aware_box,ordinary_buttons_center_content_and_icon_only_targets_are_large_enough,timeline_disclosures_resolve_tailwinds_collapse_name_collision}`, `crates/app/src/{spawn,console,trace,timeline,update}.rs::tests` |
| One visible focus treatment for buttons, fields, native select, tabs, disclosures and recall | `style::tests::every_control_family_has_the_same_visible_focus_indicator` |
| Ordinary button centering, intentional multiline centering and primary/ordinary/quiet/destructive hierarchy | `style::tests::{ordinary_buttons_center_content_and_icon_only_targets_are_large_enough,permission_choices_keep_centered_compact_and_multiline_geometry}`, `console::tests::the_traces_verbs_are_on_the_console_bar_and_only_on_its_own_surface`, component SSR tests |
| Launch, fold, running state, one-time Command autofocus, focus-stable Launching, overflow-free empty textareas and politely confirmed recent-command recall | `crates/app/src/{spawn,style}.rs::tests`, `crates/core/tests/recent.rs` |
| Agent/client identity, low-chrome Advertisements and complete authentication states | `crates/app/src/agent.rs::tests`, `crates/core/tests/capability.rs` |
| Two-line capability rows in the rail, hairline separators, long-value wrapping, and Authentication methods as a name and the call that uses it | `style::tests::{a_capability_row_is_two_lines_in_the_rail_and_says_both_of_them,long_agent_identifiers_and_auth_method_facts_wrap_rather_than_widen,an_auth_method_is_a_row_of_a_name_and_the_call_that_uses_it,agent_controlled_narrow_window_content_wraps}` |
| Session new/list/page/load/resume/close/delete, refresh and roots behavior | `crates/app/src/agent.rs::tests`, `crates/core/tests/{session,restore,lifecycle,roots,capability}.rs` |
| Flat Session rows, current/available/stale wording, centered lifecycle hierarchy and one Agent-rail scroller | `agent::tests::{listed_sessions_state_which_one_is_current_without_changing_its_affordances,session_lifecycle_and_roots_use_compact_daisyui_components}`, `style::tests::session_lifecycle_uses_grouped_rows_and_explicit_states` |
| Every listed Session and repeated roots Affordance has a target-specific accessible name | `agent::tests::every_listed_session_affordance_names_the_session_it_drives` |
| Complete adversarial root paths remain in the compact Heroicons disclosures and locally scrollable fields | `agent::tests::a_long_reported_root_remains_complete_in_the_summary_and_field`, `style::tests::{agent_controlled_narrow_window_content_wraps,session_lifecycle_uses_grouped_rows_and_explicit_states}` |
| Flat Timeline entry families, bounded structured groups, ordering, raw evidence, text-borne plan state and semantic diff sides | `timeline::tests::the_timeline_is_a_flat_center_surface_rather_than_a_card`, `style::tests::{an_entry_is_a_marked_gutter_and_the_content_beside_it,semantic_diff_elements_keep_the_existing_verbatim_block_presentation}`, `crates/app/src/{timeline,update}.rs::tests`, `crates/core/tests/{typed,misbehaviour,conformance}.rs` |
| Permission order, waiting state, allow/reject/refusal and focus-stable answered choices | `crates/core/tests/permission.rs`, `crates/app/src/permission.rs::tests` |
| Elicitation form controls per property type, unrenderable types drawing none, reported-not-enforced constraints, prefilled defaults, the raw answer surface, the three always-offered focus-stable answers, and a URL shown in full with its host | `crates/core/tests/elicitation.rs`, `crates/app/src/elicitation.rs::tests` |
| Session Settings flat rows, segmented pairs, distinctly named native selectors/actions, boolean toggles, focus-stable pending states, refusal and Advertisement gates | `crates/app/src/session_settings.rs::tests`, `crates/core/tests/session_settings.rs` |
| Full-width labelled composer with inset keyboard guidance, one persistent Send/Stop Affordance, IME-safe Enter predicate, restrained Turn-end announcement markup and explicit ready/running/waiting/error states | `crates/app/src/composer.rs::tests`, `style::tests::the_composer_is_one_bounded_field_with_its_footer_in_flow`, core Turn tests |
| The Console beside the turn, its two verbs on its own bar, envelope-shape chips that can hide nothing without a way back, complete tab keyboard navigation and text-borne Diagnostic provenance | `crates/app/src/console.rs::tests`, `crates/core/tests/{inspector,stdio}.rs` |
| Trace ordering, the selected Frame read whole in its own pane, plain Connection metadata, local width containment, focus-moving cross-screen navigation, export and focus-stable clear | `crates/app/src/{trace,timeline,style}.rs::tests`, `style::tests::{the_frame_a_reader_selected_is_read_in_a_pane_and_not_in_the_row,a_payload_is_coloured_by_what_it_is_made_of_and_marked_no_other_way}`, `crates/core/tests/{trace,export}.rs` |
| Main Timeline landmark, peer pane headings, labelled Console region and persistent automatic-navigation announcement | `crates/app/src/{main,console,timeline,session_settings}.rs::tests` |
| Shared Copy accessible name and tooltip, and toast live region | `crates/app/src/{copy,toast}.rs::tests` |
| A JSON payload lexed rather than parsed, every byte of it returned, and a key told from the text beside it | `crates/app/src/json.rs::tests` |
| The rail's Connection card, its two lookups, both accounts drawn alike, and the way into the palette | `crates/app/src/rail.rs::tests` |
| Every command reachable by name, grouped, narrowed and run from a platform-owned dialog with a document-level shortcut | `crates/app/src/palette.rs::tests` |
| The commands the Agent published offered where they are typed, replaced rather than merged, and refilling rather than sending | `crates/app/src/composer.rs::tests` |
| Flexible-surface containment, the complete set of internal and local evidence scrollers, and export-error wrapping | `style::tests::the_document_and_flexible_surfaces_shrink_without_adding_scrollers` |
| Flexible Timeline, a rail with a floor, independent scrollers and a Console that stacks under the turn before either becomes unreadable | `style::tests::{the_window_is_a_toolbar_over_a_rail_and_two_screens,a_window_too_narrow_for_three_columns_stacks_the_two_screens}` |
| Agent-controlled wrapping declarations used by narrow-window surfaces | `style::tests::agent_controlled_narrow_window_content_wraps`; computed 960px and 1440px layout remains in the walkthrough |
| Reduced motion and no layout-moving evidence disclosure | `style::tests::{a_reader_who_asked_for_less_motion_gets_none,daisyui_disclosures_do_not_transition_evidence_layout,nothing_that_arrives_animates_and_nothing_that_moves_transitions}` |
| daisyUI owns ordinary component presentation | `style::tests::daisyui_owns_ordinary_component_presentation` |
| Generated stylesheet is current | `npm run css:check` |
| Tracked-source, offline, Node/npm/npx/dx-free desktop build and tests | `scripts/check-node-free.sh` |

## Desktop walkthrough

Use Testy from `just testy`, then launch the inspector with `just run`. Repeat the walkthrough at
both window sizes and each Appearance combination above. At every step, `document.documentElement`
and `body` must have `scrollWidth <= clientWidth`; the rail, the Timeline and the Console may scroll
only on their documented inner axis.

| Step | Drive | Observe |
|---|---|---|
| Empty shell | Start with no Agent | Dot-plus-text Disconnected state with the tool's own name drawn once, a Connection card saying no agent was launched, the client's own claims under the rail's Capabilities tab, a concise titled Timeline empty state carrying the control that resolves it, no composer, and a Console on Trace saying what will be in it with no filter bar over it |
| Appearance | Choose Light, Dark, System; restart after each | Inspector Light, Inspector Dark and OS-driven System persist and switch without stale colors |
| Indentation | Switch Indent on; open a timeline entry's raw evidence and a tool call's raw input, with the pointer and with the keyboard; close each again; restart with it on; copy a row while it is on; select a frame with the switch off | The switch is off on first launch and persists across a restart; a payload lays out when its own disclosure is opened either way and goes back to one line when it is closed, without field order or spelling changing; the Console's frame pane is laid out whatever the switch says, because it holds the one frame that was selected; a frame that is not JSON is unchanged everywhere; the clipboard carries the wire's single line |
| Launch | Enter command, arguments, environment and cwd; focus Launch and activate it; after a later disconnect keep focus outside the form | Focus remains on Launch while it says Launching; fields fold only after the Agent answers; running invocation and identity remain visible; a status-driven field remount does not autofocus Command |
| Recent command | Stop, show fields, clear the multiline fields, activate a recalled invocation, edit each field in turn after recalling again, then recall and Launch | Empty multiline fields show no horizontal scrollbar; fields refill without launching; a polite status names the recalled invocation and next step, then clears after each edit and on Launch; a 2,000-character unbroken Arguments and Environment value scrolls only inside its field; keyboard focus remains visible |
| Advertisements | Open the rail's Capabilities tab and scroll both accounts | Hairlines replace repeated cards; group headings stay on screen while their rows scroll; every stable claim is stated in words on two lines; advertised/not-advertised and driven/refused facts remain distinct, and neither account is drawn more quietly than the other |
| Authentication | Exercise available Authenticate/Logout, loading, disconnect and refusal fixtures | Each method is a name and the call that uses it, wrapping rather than widening the rail; loading, success, refusal and unavailable states remain separate announced facts; ordinary, quiet and disabled actions stay distinct |
| Session lifecycle | New, list, page, load/resume, close and delete | Flat grouped rows state current, available, loading, stale, empty and failed states in words; each Affordance follows only its Advertisement, names its Session, refreshes the listing and preserves Trace evidence; the dialog is reached from the rail's Session tab and from the Timeline's own header |
| Roots | Open new-Session and reported-root disclosures; edit additional directories | Shared Heroicons Outline chevrons expose compact controls; each repeated field names its Session; complete long paths wrap or scroll locally and reported values remain visible when closed |
| Timeline families | Run `session_updates`, `tool_calls`, `full`; inspect plan and diff accessibility text | Messages, thought, tools, plan, commands, usage, resources, unrecognized entries and annotations retain order and raw evidence; plan states and priorities are words; diff paths label regions whose old/new sides read as removed/added content |
| Permission | Run `callbacks`; use keyboard to choose each option kind and activate “go to it” | Waiting live region appears; “go to it” scrolls to and focuses the request row; an activated option retains focus while becoming unavailable; Agent option order and allow/reject distinction survive wrapping |
| Elicitation form | Run `elicitations`; fill the ten-field form by keyboard, switch to Raw and back, put a value outside a stated range, then Accept | Every property type draws its own control and an unknown one draws none beside its raw schema; defaults arrive filled in; constraints are stated and none of them blocks the answer; the tab on screen is the one that sends, says so, and keeps what was written in the other; the activated answer keeps focus while becoming unavailable |
| Elicitation URL | Reach the URL elicitation and activate its link, with the browser both available and not | **The page opens in the platform's browser and never inside this window**; the full URL and its host are readable before the click and nothing is fetched before it; Accept reads as consent rather than completion until the Agent's own `elicitation/complete` arrives |
| Elicitation outside a session | Drive a request-scoped elicitation with no Session open (`crates/core/tests/elicitation.rs`'s scripted agent) | The panel is drawn and answerable with no live Session; the row names the request it is tied to rather than a Session |
| Session Settings | Exercise mode, pairs, three or more choices, long, grouped, boolean and unknown-current values, including a slow answer and refusal | The rows sit under the live Session in the rail's Session tab and stack copy above controls; current values are stated in words; selectors name values while adjacent actions name applying them; the activated control retains focus while pending; segmented pairs, native selects and toggles remain keyboard-operable; long names, ids, methods and errors wrap; the surface scrolls independently |
| Composer | Enter a draft, focus Send, submit, then stop `wait_for_cancel`; repeat with Shift+Enter and an IME composition | The larger-radius textarea stays unobscured; focus remains on the same Affordance as Send becomes Stop and back; plain Enter sends, Shift+Enter and composing Enter edit; Turn endings announce once without streaming content becoming live |
| Console | Traverse tabs with Left/Right/Home/End; press each envelope-shape chip; move the spine through Turn, Split and Wire | Trace and Diagnostic counts remain facts whatever is hidden; selected tab, panel and focus agree; a chip that hides rows says how many are left; the spine gives either screen the whole body and back |
| Trace, the pane and disclosures | Select Frames with the pointer and the keyboard; open raw Timeline/tool evidence; navigate both ways; focus Clear and empty the Trace | A row announces a concise ordinal/direction/Connection/time action rather than all raw bytes and says it is read in the pane below; the row's own Copy and Reach are siblings of it rather than nested in it; the pane reads the newest visible frame until one is chosen and then the one that was; each reveal moves focus to its marked destination and the pane follows it; Clear retains focus as it becomes unavailable; direction, connection, order and raw bytes remain intact |
| Session identity | Inspect and copy an adversarially long Session ID from the Timeline header | The visible value truncates locally; its tooltip and Copy action retain the complete ID |
| Diagnostic error | Launch a missing command and a noisy Agent | Console selects the Diagnostic channel; the automatic change is politely announced without stealing focus; each line identifies Agent stderr or Transport provenance; long evidence remains verbatim and copyable |
| Copy toast | Copy Frame, raw evidence and diagnostic line, including with the Clipboard API unavailable | Focus remains on the Copy control without changing the viewport; polite toast appears without blocking the next Affordance |
| Commands the Agent published | Run an agent that sends `available_commands_update`; type `/`, narrow it, move with the arrows, take one with Enter and with the pointer, and wave the list away with Escape | The list is the Agent's own and appears only while a command is being typed; a newer announcement replaces the earlier list rather than adding to it; taking one refills the box and sends nothing; Escape leaves the draft exactly as it was |
| Command palette | Open it from the rail and with Ctrl/⌘ + K; narrow it; run a command with Enter and with the pointer; close it with Escape and with a click outside | The keystroke toggles; focus is trapped and handed back; every command is one that also exists on screen; running one closes the palette and forgets the query |
| Reduced motion | Enable OS reduced motion and repeat disclosures/copy | Component transition and animation duration is effectively zero; arriving rows never animate |
| Adversarial width | Use unbroken 2,000-character Agent/client names, versions, protocol ids, capability labels, auth methods, paths, values and refusals | Every value wraps or scrolls locally; no document or rail horizontal scrollbar and no inaccessible clipping at either size |

## Recorded execution

### Schemaform 0.5.0 adoption, 2026-09-18

Focused Elicitation walkthrough on Linux, WebKitGTK 2.52.4, Xvfb at 96 DPI,
Openbox and Mesa software rendering. Actual WebView sizes were **1440 x 880** and
**960 x 640** (native window height includes an additional 25px menu strip).
Explicit Light and Dark were inspected at both sizes; document/body scroll widths
equaled viewport widths. The tall form remains in the Timeline's local scroller.

Keyboard-only Testy `callbacks` reached `end_turn`: all ten fields were visited,
age `999`, confidence `2`, confirmed checked, name absent, a priority and a tag
selected. Raw showed exactly that object; switching back retained it. Accept sent
five fields despite three broken rules. All three findings appeared locally and
in summary entries naming age, confidence and name. Accept retained its DOM focus
while becoming ARIA-disabled/chosen; the panel changed to resolved. The remaining
requests were answered Decline, Cancel, Accept and Decline, and the URL completion
updated independently. Checkbox and tab activation no longer navigates to Trace.
After resolution, summary links still focus their fields: an Age link focused the
read-only `999` input, and attempted keyboard replacement left it `999`. Resolved
forms are readable and navigable rather than inert.

System removed `data-theme` and selected light on this host. Changing GNOME's
`color-scheme` preference to prefer-dark did not change WebKit's media query under
this Xvfb session; the preference was restored. **System's OS-dark transition is
not verified**. Screen readers, macOS and Windows remain unrun. This focused run
does not claim to repeat unrelated desktop walkthrough rows.

The rendered release test exercises all three local findings and named summary
entries; the core numeric feature matrix verifies both numeric Cargo features
together. The raw literal fixture remains available as
`python3 scripts/spike-testy-literals.py` when launching an Agent from this root.

> **This records the fifth ring's walkthrough and predates the drawing**
> ([ADR 0009](adr/0009-the-window-is-the-drawing.md)). Every behaviour it exercised is
> still on screen and still covered by the automated matrix above, and the *places* it names are
> not: the Console was a row under the screens, the two accounts were a tablist across the centre,
> and Session Settings were a right rail. The walkthrough is owed again on the arrangement above.

On 2026-08-05 the complete walkthrough passed in a rendered Linux desktop at 1440 x 880 and
960 x 640. The session used Xvfb, Openbox, GTK's Adwaita light and dark appearances, WebKitGTK and
Mesa software EGL at 96 DPI. Mesa's EGL vendor and DRI paths were supplied explicitly because the
headless host otherwise aborted WebKit before rendering with `EGL_BAD_PARAMETER`.

Testy exercised the empty shell; launch and recall; Agent claims; authentication; Session
new/list/page/load/resume/close/delete behavior and roots; `session_updates`, `tool_calls`,
`callbacks`, `full` and `wait_for_cancel`; permission allow and reject; Session Settings changes
and refusal; composer send and stop; Console keyboard routes; Trace and Diagnostic disclosures;
missing-command diagnostics; copy feedback; reduced motion; and an unbroken 2,000-character
reported root. These flows were inspected under explicit Inspector Light and Inspector Dark.
System was restarted under both Adwaita appearances: `data-theme` remained absent, the preferred
color scheme changed with the desktop, and no stale explicit colors survived restart. Focus,
non-color distinctions, native frame and scrollbar integration, wrapping and each surface's
documented scroll axis remained
usable at both sizes.

The keyboard-only pass reached Appearance, Disconnect, launch fields and recall, authentication, every
Session Affordance, roots, Mode and Config Option controls, composer Send/Stop, permission choices,
Timeline evidence, Console tabs and collapse, Trace export/clear and all copy controls. Every stop
showed the shared focus treatment. Ordinary button and icon-and-text labels remained vertically
centered; wrapped permission choices retained symmetric spacing. Long names, versions, protocol
ids, Agent Capability and authentication labels, paths, Session Settings values and refusals,
Frames and diagnostic lines stayed available through wrapping or their documented local scroller.
With the desktop's reduced-motion preference enabled, evidence arrival did not animate and
disclosures and copy feedback used effectively zero-duration motion.

The walkthrough found that Session Settings flex children could shrink below their content height
at 960 x 640, causing adjacent rows to paint through one another instead of making the Session
Settings surface scroll. `.settings > * { flex: none; }` now preserves the rows' content height;
the style contract and a rendered minimum-height recheck cover the fix. At 96 DPI, direct WebKit
inspection reported equal document, body and main `scrollWidth`/`clientWidth` values of 1440/1440
and 960/960. At the host's original fractional DPR of 1.0416666, the root document scroller
remained equal at 1383/1383, while an otherwise empty 1382.4px body was quantized to 1383/1382 by
CSSOM. This was a measurement artifact with no horizontal document scrollbar or inaccessible
content; no clipping rule was added to conceal it.

On 2026-08-07 a focused Linux WebView recheck covered this follow-up at 1440 x 880 and 960 x 640 in
explicit Light and Dark. The refreshed captures show the complete copyable Session ID, peer pane
headings, labelled composer, and raw-evidence actions beside closed disclosures. Keyboard activation
moved Timeline-to-Trace focus to the matching Frame summary and then exposed the sibling Frame
actions; the return action marked and focused the Timeline entry. This focused run supplements rather
than replaces the complete August 5 walkthrough. The polite automatic-navigation announcement remains
covered by persistent-region markup tests until the platform screen-reader pass recorded above is run.
System under both OS appearances and the macOS pass remain part of the next complete matrix execution.
The text-borne Diagnostic provenance, plan-state and diff-side checks, the Launch recall/placeholder
checks, and the focus-stable controls and concise accessible names added after that focused run are
covered by component, markup and stylesheet contracts only; their walkthrough rows remain acceptance
criteria for the next platform execution rather than claims about either recorded run.

## Keyboard route

Without a pointer, reach and operate Appearance, Disconnect, launch disclosure and fields, recall,
target-named authentication, new/list/page/open/close/delete, roots disclosures and fields, every Mode and Config Option segmented group, native select and toggle,
composer Send/Stop, permission options, Timeline raw/reach/copy, Console tabs and collapse,
Export/Clear, Frame disclosure/reach/copy and Diagnostic copy. Native controls use their native
Enter/Space/arrow behavior; Console tabs additionally implement Left/Right/Home/End. Every stop has
the common `--focus` outline and every repeated Session Affordance names its Session.

## Retained custom CSS

Custom rules remain only for shell/grid/scroller layout, wrapping Agent-controlled values, raw and
monospace evidence, direction/annotation/navigation marks, accessibility text and live regions,
semantic contrast aliases, retained text roles, permission meaning, reduced motion and suppression
of layout-moving evidence transitions. Standard daisyUI classes continue to own ordinary control
geometry.

## Release commands

Run from the repository root; then record the desktop walkthrough above on Linux and macOS:

```sh
just testy
cargo test --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
npm ci
npm run css:check
just node-free
git diff --check
```
