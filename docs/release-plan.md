# Release plan

This plan comes from the pre-release review of 2026-10-10, at commit `788a653`. Five independent reviews covered:

- form libraries;
- visual design;
- functionality;
- user experience;
- release engineering.

Every finding from those reviews is listed here once, with an id, so commits and issues can refer to it.

**The plan has two parts.**

- **Part 1** is work this repository can do alone, ordered by when it should land:
  - *before the tag*;
  - *soon after* (the next 0.1.x releases);
  - *later* (roadmap).
- **Part 2** is work that waits on another project:
  - our own form libraries, tracked in #21;
  - third-party projects, tracked in #22.

Where a Part 2 item needs an interim fix here, that fix is in Part 1 and refers to the Part 2 item.

How findings were established:

- *(repro)*: reproduced with a scripted agent.
- *(code)*: found by reading the source, not yet driven in a live window. Confirm these natively while fixing them.
- No marker: verified by running a command or checking the code directly.

---

# Part 1: no external dependency

## 1.1 Before the tag

### Release engineering

- [ ] **R1. Make the macOS bundle launch on other Macs.**
  - Problem:
    - A bundle built in the devenv shell links `/nix/store/…/libiconv.2.dylib` and has `minos 14.0`.
    - Its `Info.plist` declares 10.15 (`crates/app/Dioxus.toml`).
    - The stale `dist/InspectorDesktop.app` has the same fault.
  - Fix:
    - Build release artifacts outside Nix, or point libiconv at `/usr/lib` and set `MACOSX_DEPLOYMENT_TARGET`.
    - Make `minimum_system_version` match the real `minos`.
    - Add a guard to `just bundle-macos` that fails on any `otool -L` entry outside `/System` or `/usr/lib`, and on a `minos` that differs from the plist.
  - Verify: sign, notarize, `spctl -a -vv`, then launch on a clean Mac running the oldest supported macOS.
- [ ] **R2. Get `just check` green.**
  - Delete the unused `use dioxus::prelude::*;` at `crates/app/src/rail.rs:325`.
  - Add `rust-toolchain.toml` and `rust-version`. The toolchain currently floats on `stable`, which is how a new lint broke the gate.
- [ ] **R3. Add CI.** There is no `.github/` folder.
  - Add GitHub Actions on macOS and Ubuntu: fmt, clippy (desktop and web), `check-number-features`, Testy at a pinned rev, `cargo test`, `npm ci && npm run css:check`, and the node tests.
  - Add a Windows `cargo check`, or declare Windows unsupported.
  - Correct the docs that already claim CI exists: README lines 343 and 357, the justfile at lines 13, 46 and 104, and ADR 0003:40.
- [ ] **R4. Make the suite pass in the dev shell.**
  - Add `python3` to `devenv.nix` and to the README prerequisites. Without it there are 19 failures in `exit_drain`, `process_tree`, `retention` and `stdio`.
  - Pin `ACP_RUST_SDK_REV` in `scripts/build-testy.sh` and in Dagger. It currently tracks rust-sdk `main`.
- [ ] **R5. Fix the order-dependent test.** `crates/core/tests/exit_drain.rs:51` failed 2 of 7 runs on macOS. Accept either a `WriteFailed` diagnostic or the replies being absent from the trace.
- [ ] **R6. Fix the bundle name.** dx ignores `[application] name`, so the bundle is `AcpInspector.app`. Rename it and patch `CFBundleName`/`CFBundleDisplayName` in `just bundle-macos` before signing. The upstream side is #22.
- [ ] **R7. Run `cargo update -p rustls`.** This fixes RUSTSEC-2026-0285 by moving to 0.23.45.
- [ ] **R8. Ship license notices and add license checks.**
  - Generate `THIRD_PARTY_LICENSES` with cargo-about. Put it in `Contents/Resources` and in the DMG.
  - Add `deny.toml` (licenses, advisories, sources) with commented ignores for the advisories we can't fix (#22), and run `cargo deny check` in CI.
- [ ] **R9. Decide the release identity.**
  - Version: 0.1.0 is recommended over 0.0.1.
  - Supported platforms.
  - A CHANGELOG entry for the release.

### Fidelity and correctness (core)

- [ ] **C1. Read bytes and decode lossily on both pipes.** *(repro)*
  - Today one non-UTF-8 byte does damage on either pipe:
    - On stderr it silently ends console capture. The inspector then drops the pipe and the agent dies of a broken pipe.
    - On stdout it tears down the connection, and the bad frame never reaches the trace.
  - Where: `crates/core/src/stdio.rs:284-318`, `:370-375`.
  - Fix:
    - Read bytes and decode lossily, marking the frame as lossy (or carry the bytes using the export's reserved `encoding` field).
    - Never stop draining stderr.
    - Update the `exit_drain.rs` tests that assert the current behaviour, and add regression tests built from the probe cases.
- [ ] **C2. Check the session id on `session/update`.** *(repro)*
  - Today updates for another session change the live session's mode and settings, and their content lands unmarked in the live timeline (`client.rs:1448-1461`, `:2216-2244`).
  - Show a mismatched update as a "foreign session" entry, and leave the settings alone.
- [ ] **C3. Account for frames dropped at the transport.** *(repro)*
  - Today a frame over 10 MiB is reported only as a diagnostic. The export says `dropped: 0`, and the call whose answer was dropped waits forever (`stdio.rs:276-283`).
  - Record a placeholder in the trace and the export.
  - Fail the matching call with its own error when its id can be recovered from the retained prefix.
- [ ] **C4. Bump `agent-client-protocol-schema` from 1.7 to 1.11.**
  - Decode what became stable since 1.7: the tool-call `name` (1.8), session compaction and session notices (1.11).
  - Update the capability tripwire and the "11 stable variants" claims.
  - If this slips, say plainly in the README that the inspector targets 1.7.
- [ ] **C5. Timestamp incoming frames when they are read.**
  - Today they are stamped when consumed, after waiting in the queue (`connection.rs:329-336`).
  - Stamp them in the reading pump, and use monotonic time for durations.
  - This makes the "the trace's order is the wire's" claim in `docs/trace-export.md` true.
- [ ] **C6. Cap the two memory growths an agent can drive.**
  - `conversed` keeps a frame of up to 10 MiB for every session id it sees (`client.rs:2275`). Key it only on known sessions, or store a flag and a frame id instead of the frame.
  - Pending permission and elicitation entries are exempt from retention and uncapped (`timeline.rs:652`, `:709-723`; `blocking.rs:165`). Cap them; past the cap, auto-refuse and record it. The current growth is also O(n²).
- [ ] **C7. Cap stderr lines at or below the console budget.**
  - The budget is 8 MiB (`inspector.rs:1099`) but lines may be up to 10 MiB (`stdio.rs:36`), so one long line wipes the whole console.
  - Cap at about 1 MiB and say so when a line is cut.
- [ ] **C8. Handle panics.**
  - Add a panic hook that logs to the state directory and moves the connection to Lost. Today a panicking read task leaves the status at "Connected" with every call hanging.
  - Add a Dioxus `ErrorBoundary` at the root.

### Security and privacy

- [ ] **S1. Restrict the elicitation URL link.**
  - Parse with `url::Url`, allow only http and https, display `host_str()`, and render anything else as inert text (`elicitation.rs:427-435`).
  - Tighten the navigation handler (`shell.rs:52`) to `dioxus://` plus http(s).
  - Make the link inert once the request is resolved.
  - The upstream side is in #22.
- [ ] **S2. Stop storing secrets silently.** `recent.json` stores the agent's env, API keys included, in plaintext. Do one of these:
  - store env names only;
  - offer "don't remember env";
  - at minimum, say so next to the env field and in the README.

### Interaction bugs

- [ ] **B1. Stop nested controls from triggering the row's reveal.** *(code; the cause is confirmed in source)*
  - Today clicking a permission option or a tool-call input/output disclosure also fires the row's "reveal frames" handler. That steals focus, clears the console filter, and at narrow widths switches the region.
  - Where: `permission.rs:166` and `update.rs:331-336` don't stop propagation; `elicitation.rs:187` already does.
  - Fix: stop propagation in those controls, or ignore row clicks that land on interactive elements or happen while text is selected. Add a regression test.
- [ ] **B2. Restore the turn-state badge tones.**
  - Streaming, Refused and Waiting all render as the same grey, because the unlayered `.badge` rule (`input.css:313-321`) beats daisyUI's tones.
  - Drive `--badge-color`, or move the repaint into `@layer components`.
  - Delete the dead `.badge-ok/-bad/-in/-vio/-acc` and `.turn-badge*` rules.
- [ ] **B3. Accept elicitation schemas without a root `type`.**
  - In `prepare()` (`elicitation_schema.rs:11-15`), set `type: object` when it is absent.
  - Add tests for `{}` and for a schema with no `type`. Today these get no form, and the inspector blames the agent.
  - The upstream side is U6 (#21).
- [ ] **B4. Make the rail's tabs keyboard-operable.** Add arrow-key handling (`rail.rs:102-117`) by reusing `console::tab_for_key` and `focus_tab`. The Capabilities tab is currently unreachable by keyboard.
- [ ] **B5. Use macOS shortcut conventions.**
  - Use ⌘ on macOS (`shell.rs:175-235`). Control clashes with text editing: Ctrl+K, Ctrl+E, Ctrl+N.
  - Rename the menu's "Stop" to "Disconnect agent".
  - Add "Cancel turn" on ⌘. / Ctrl+. and to the palette.
  - Show ⌘K, not "Ctrl K", on macOS (`rail.rs:155`).
- [ ] **B6. Make `session/delete` safer.** Confirm it inline in two steps (`agent.rs:1953-1965`). `session/close` stays one click.
- [ ] **B7. Stop favouring "allow" in permission styling.**
  - Don't fill the first `allow*` option (`permission.rs:96-111`).
  - Never fill `*_always`, and tag those options visually.

### First-run and recovery UX

- [ ] **X1. Show launch failures inside the Connect dialog.**
  - Pass the status, the problem and the latest transport diagnostic to `SpawnForm` (`main.rs:1035-1062`, `connect.rs:18-24`).
  - Render an alert with `role=alert`, keep the dialog open, and move focus to the alert or to the Command field.
  - Word spawn failures as "Couldn't start the agent: {os error}", not "the connection to the agent is gone" (`core/call.rs:62`).
- [ ] **X2. Give a Finder-launched app a usable `PATH`.**
  - A Finder-launched `.app` inherits a minimal `PATH`, so `npx` and Homebrew agents aren't found (`core/stdio.rs:117`).
  - Resolve `PATH` from the login shell at startup, with a timeout. At least show the effective `PATH` in the X1 alert.
  - Verify with `just bundle-macos`.
- [ ] **X3. Catch spawn-form mistakes.**
  - Detect a whole command line typed into Command, and offer to split it into Command and Arguments.
  - Warn about env lines without `=`, which are currently dropped silently (`core/command.rs:149-153`).
  - Make Enter launch: wrap the fields in a `<form>`.
- [ ] **X4. Add one-click Restart.**
  - Relaunch the last invocation and keep the trace.
  - Make it available when connected, Lost and FailedToStart.
  - Put it in the rail, the palette and the menu, on ⌘R / Ctrl+R.
  - Replace "reconnect", which today means "disconnect and open the dialog".
- [ ] **X5. Don't strand the user when auth is required.**
  - Show the login guidance and the Authenticate actions in the timeline's empty state, not only on the Capabilities tab (`agent.rs:1395-1430`, `timeline.rs:191-195`).
  - Re-check the claim that `authenticate` "does not perform one" against the spec.
- [ ] **X6. Put focus somewhere useful.** After launch, focus the prompt. After disconnect, focus Restart. Today focus falls to `<body>` in both cases.
- [ ] **X7. Rename the narrow-layout regions.**
  - Change "Details | Session | Messages" to "Details | Timeline | Trace". Rename the timeline header to match.
  - Hide the Split | JSON-RPC full toggle below 1080px.
  - Put a "waiting on you" badge on a hidden region that has a pending request.
- [ ] **X8. Improve the date-time note in elicitation (interim).**
  - Say the picker yields a local time with no zone, and that Raw is the way to send a conformant value.
  - The real fix is U5 (#21).

### Design quick wins

- [ ] **D1. Raise contrast and the type floor.**
  - Use at least 11px for text and 12px for control labels, and retire the 9px step.
  - Raise `--dim2` to about 4.5:1: light `#636c78`, dark `#808a96`. Never use it for clickable labels.
  - Update the contrast exception test in `style.rs`.
  - This resolves the deliberate sub-AA values from ADR 0009; record the decision.
- [ ] **D2. Use one button style.**
  - Sans 12px, weight 500, sentence-case verbs: Configure…, Disconnect, New session, Load, Close, Delete, Export, Clear.
  - Put wire method names in tooltips or small captions.
  - Keep monospace for data only.
- [ ] **D3. Settle the fonts.** Either bundle IBM Plex Sans and Plex Mono as woff2 (OFL, about 300KB, still offline), or remove Plex from the stack and tune for the system fonts. Today Plex is asked for and never shipped, so Linux falls back to DejaVu.
- [ ] **D4. Make it feel like a native window.**
  - Set the window background colour, so there is no white flash on a dark launch.
  - Don't style `::-webkit-scrollbar` on macOS, so overlay scrollbars work.
  - Use the default cursor on controls.
  - Make the "connected" dots blink only while streaming.
- [ ] **D5. Tell selected apart from focused.**
  - Draw selected as a tint plus a check or bar, and keep the lime ring for focus only.
  - Give trace filter chips a visible off state.
- [ ] **D6. Fix the Send icon.** It reads as ⚠. Rotate it, or use the v2 paper-airplane or arrow-up.
- [ ] **D7. Rewrite the worst copy.**
  - Remove wire names from labels: `0 available_commands` (hide it at zero), "driven and refused", "awaiting client".
  - Stop capitalising glossary nouns mid-sentence.
  - Remove the editorial asides from controls.
  - Make "configure…" and "Connect…" one name.
  - The UX review's rewrite table is the source.

### Documentation

- [ ] **DOC1. Rewrite the README opening.**
  - Lead with the pitch. Move the schemaform paragraph down.
  - Add a screenshot and a quick start: Rust version, `just`, python3, Linux packages, `just run`, and an agent to point at (check the `npx` package name is still current).
  - Add supported platforms, Gatekeeper instructions for the DMG, and how to report security issues.
  - Add known limitations:
    - fs and terminal are declined, so agents act directly in their cwd, unmediated;
    - descendants that leave the process group survive disconnect;
    - env is stored in `recent.json` (S2).
  - Fix the method list (add `session/set_mode` and `session/set_config_option`) and the Layout block.
- [ ] **DOC2. Remove links to private and host repositories.**
  - About 105 links point to the private `dioxus-chat.orig` repository: 87 in `architecture.md`, 13 in ADRs 0001–0004, and some in `trace-export.md`, `mock.rs`, `spawn.rs` and `tests/lifecycle.rs`.
  - Other host-repo leftovers:
    - relative links in `architecture.md`: `../../../docs/app-architecture.md`, `bridge-architecture.md`, `a11y-smoke-pass.md`, `docs/research/...`, `crates/dioxus-agent-ui-acp`;
    - comment citations in `stdio.rs:31` and `tests/stdio.rs:231`;
    - "ADR 0006" in `tests/headless.rs:3`, which collides with this repo's ADR 0006.
- [ ] **DOC3. Fix stale claims.**
  - `architecture.md` §12 says "9 methods"; 13 are driven.
  - §7.1 still says `mcpServers: []` and a "Text-only" composer.
  - §7.3 says "boolean config options and nothing else".
  - The broken sentence at `client.rs:539`.
  - The trace-ordering claim in `trace-export.md`; see C5.
  - The "No filesystem work runs on the UI executor" comment in `crates/app/Cargo.toml`; see L6.
- [ ] **DOC4. Add CHANGELOG and SECURITY files.** CONTRIBUTING is optional.
- [ ] **DOC5. Update PROVENANCE.md.**
  - Use the remote install, `--git … --rev 606352f` (merged as `affc7af`), instead of a local path.
  - Note that the local checkout must be pulled first.
  - Make ADR 0013's cited registry rev agree.

## 1.2 Soon after the release (0.1.x)

### Functionality

- [ ] **F1. Pair requests with responses.**
  - Jump between a call and its answer, with a duration on each answer.
  - Show "No response yet" with elapsed time on pending calls, and add a "pending" filter.
  - Annotate answers with the wrong id type: today `"3"` or `3.0` leaves the call pending forever (`rpc.rs:274-276`).
  - Classify `id: null` with `error` as an error response (`rpc.rs:297`).
- [ ] **F2. Add a raw request composer with edit and resend.**
  - Choose the method, the params JSON, and request or notification.
  - Show the answer paired underneath.
  - Add "Edit and resend" on every client→agent row.
  - Record these frames as hand-sent, keep them out of the typed stores, and have conformance rules skip them.
  - Revisit the §1.1 line "per-frame replay: Never" in the architecture doc.
- [ ] **F3. Let a human answer client services.**
  - Add a launch-time client-capability toggle, so what the inspector claims matches what it will answer.
  - Show a manual response editor (a result or error skeleton) for `fs/*`, `terminal/*` and any other request it doesn't service, like the MCP Inspector does for sampling.
  - Then add scripted mock responses (a rule file) on the same editor.
- [ ] **F4. Support `$/cancel_request` in both directions.** Retire a panel the agent cancelled; send one for stuck calls.
- [ ] **F5. Import traces and choose where exports go.**
  - Import and reopen a JSONL trace, rebuilding the timeline through the typed layer.
  - Export with Save As, defaulting to `~/Downloads/acp-trace-<agent>-<time>.jsonl`.
  - Optionally export stderr, and make the export notice dismissible.
- [ ] **F6. Shut agents down gracefully.**
  - On disconnect and reconnect: close stdin, wait a grace period, then SIGTERM, then SIGKILL, and record which step ended it (`stdio/process.rs:87-99`).
  - Don't wait forever for EOF after the leader exits.
  - Keep the synchronous SIGKILL for window teardown only.
- [ ] **F7. Control the spawn environment.** Allow unsetting or clearing inherited variables, and offer a minimal-env preset.
- [ ] **F8. Improve trace filtering.**
  - Filter by direction, method, id, session and connection.
  - Change the placeholder to "Filter rows…" and the "−N" badge to "N dropped".
  - Debounce the count announcement.
- [ ] **F9. Protect unexported traces.**
  - Clear: offer an Undo toast, or confirm when there are unexported frames.
  - Quit: warn when there are unexported frames.

### Forms (in this repo)

- [ ] **FM1. Move the spawn and MCP forms onto the registry components.**
  - Use the installed `Field` and `Input`, and install the registry `textarea`. That gives `<label for>`, `aria-describedby`, `aria-invalid`, `required` and error regions.
  - Use string keys for `autofocus` and `spellcheck` until U9 (#21).
  - Use plain names: "Command", "Arguments", "Environment", "Working directory".
- [ ] **FM2. Fix the MCP editor.**
  - Give rows stable ids in core and key on them, replacing the `key:"{index}"` and the focus `eval`.
  - Show findings only after a field is touched or a launch was attempted.
  - Label the argument, env and header name/value pairs.
  - Move the editor below cwd, collapsed as "MCP servers (N)", with the caveats inside.
- [ ] **FM3. Make Enter in elicitation forms consistent.** Today it submits only for single-field forms. Either never submit on Enter, or always submit via a hidden submit button.
- [ ] **FM4. Keep the "required" marker on properties that can't be rendered.** Carry the quarantined required names in `Prepared` and mark those rows "Required" (`elicitation_schema.rs:52-58`).
- [ ] **FM5. Word an unset required boolean clearly.** Say "not set (unchecked is not false)" until U11 (#21).
- [ ] **FM6. Settle button styling.**
  - Add an interim host CSS rule so the renderer's presence buttons take the `btn-quiet` look; U10 (#21) removes the need.
  - Decide between registry `Button` in screens and plain `button.btn`, and amend ADR 0005 accordingly.

### Accessibility and keyboard

- [ ] **A1. Name trace rows by what they contain.** Include the method, kind and id in each row's accessible name (`trace.rs:307`, `:520`). This satisfies WCAG 2.5.3 (label in name).
- [ ] **A2. Name the dialogs and fix the palette semantics.**
  - Give the three dialogs `aria-labelledby`.
  - Give the palette and the slash list `role=combobox`, `aria-activedescendant`, highlighted rows scrolled into view, and options with `tabindex=-1`.
- [ ] **A3. Fix reading and focus order.** Render the timeline and trace chronologically instead of `column-reverse`, and keep them anchored to the bottom. Verify with VoiceOver.
- [ ] **A4. Cut the number of Tab stops.** Use a roving tabindex in the timeline and trace lists (↑/↓/Home/End) instead of hundreds of Tab stops.
- [ ] **A5. Fix the smaller semantics.**
  - Give the Form/Raw tabs a `tabpanel` and arrow-key support.
  - Show toasts above modals, and let them linger longer.
  - Make the Console announcement work while a modal is open.
  - Use one disabled pattern, and give a reason for a disabled Launch.

### UX and copy

- [ ] **X9. Widen palette coverage.**
  - Add: Cancel turn, Restart, Focus prompt, List sessions, Authenticate, Copy session id, Go to waiting request, and Show Timeline/Details.
  - Show shortcuts on palette rows.
  - Relabel "Connect…" when an agent is already connected.
- [ ] **X10. Improve recent commands.** Launch directly from a row, allow removing rows, and stop repeating "stdio" on every row.
- [ ] **X11. Fix turn and tool-call details.**
  - Say "Done" instead of "Ended", and "Prompt failed" instead of "Ended without a reason".
  - Update the timeline header after the turn ends.
  - Show the tool-call id, and add Copy to raw input and output.
  - Say when copying fails.
- [ ] **X12. Finish the copy pass.** Work through the rest of the UX review's rewrite table.
- [ ] **X13. Make "New session" honest.** The menu item silently does nothing without an agent; disable it instead.

### Design

- [ ] **D8. Unify the icons.** Use Heroicons v2 or Lucide at 16px with 1.5 stroke everywhere, replacing the Unicode glyphs (☀ ☾ ⌘ § ◆ ~ ›).
- [ ] **D9. Spell identifiers the way the wire does.**
  - Stop uppercasing wire identifiers with `.eyebrow`.
  - Move dark-mode `--str` off the lime accent (they are 1.04:1).
  - In light mode, use a lime fill with dark ink in place of the olive accent.
- [ ] **D10. Redesign the first run.**
  - Add an onboarding card with recent invocations inline.
  - Hide session-scoped UI (the Capabilities tab, "SESSION none", the command count) while disconnected.
- [ ] **D11. Reduce trace noise.** Hide `#1` unless there are several connections, show kind pills, use zebra rows instead of hairlines, and hide the preview when the pane is narrow.
- [ ] **D12. Stop motion replays.** Rows fade in again whenever a region is shown; animate only rows inserted after mount.
- [ ] **D13. Fix component inconsistencies.**
  - Use one tab style and one tag/chip component.
  - Make connect-dialog fields full width.
  - Use the accent colour for field focus.
  - Remove the duplicated mode labels ("chat … Chat").
- [ ] **D14. Restyle the evidence cards.**
  - Give each side of a diff a tinted background.
  - Tone only the label on unrecognized rows, not the whole paragraph.
  - Make the stopReason a quiet divider.
  - Shrink the composer's empty height.
- [ ] **D15. Tidy the CSS architecture.**
  - Move repaints into `@layer components`, or drive daisyUI through its variables.
  - Delete the roughly 15 dead rules.
  - Declare the palette once and reference it with `var()` in the daisyUI themes.
  - Move the rules bolted on before the reset to where they belong.
  - Fix the stale prose in `shell.rs:69-83`.
- [ ] **D16. Extend the mock harness.**
  - Add fixtures for permission, elicitation, plan, diff, error and streaming states, and for open dialogs.
  - Make the fixtures consistent.
  - Capture with animations off.
  - Recapture `design/`.

### Engineering

- [ ] **L1. Fix trace and timeline performance.**
  - Don't clone and lowercase the whole retained trace on every render (`trace.rs:193-203`, `console.rs:391`).
  - Paginate the timeline.
  - Window large tool content (`update.rs:121`, `:160`, `:285-290`, `:341`).
- [ ] **L2. Make state-file writes safe.**
  - Use a unique temp name and fsync.
  - Don't silently overwrite a corrupt file; keep a `.bak` (`recent.rs:225-233`, `settings.rs:224-231`).

## 1.3 Later (roadmap)

- [ ] **F10. Headless CLI over core.** Run, prompt, export, and a permission policy. Combined with F3 mocks, this is a CI conformance smoke tool.
- [ ] **F11. Permission auto-policy.** For long agent runs.
- [ ] **F12. More prompt content.** ResourceLink prompts and non-PDF binary resources.
- [ ] **F13. Protocol-version override.** The version is currently hard-wired to 1.
- [ ] **F14. Merged stdout and stderr timeline.**
- [ ] **F15. Record line endings verbatim.** Today CRLF is stripped by `LinesCodec`.
- [ ] **F16. Real fs and terminal mediation.** Reads and writes scoped to the session roots, terminals as captured subprocesses. Only if users ask after F3.
- [ ] **D17. Native title bar.** A transparent title bar with full-size content and the toolbar inside it.
- [ ] **D18. Better narrow layout.** Two columns plus a rail drawer below about 1180px, a resizable rail, and Capabilities as a wide sheet.
- [ ] **D19. Spacing and radius scale.** Four radii and a 4px spacing grid.
- [ ] **L3. Release profile.** Add `[profile.release]` with `strip`, `lto = "thin"` and `codegen-units = 1`. The binary is 25 MB today.
- [ ] **L4. Split the large files and trim comments.**
  - Split `agent.rs` (4,991 lines) and `client.rs`.
  - Move tests out where it helps.
  - Keep rationale in ADRs and the spec, and limit call-site comments to 1–3 lines.
- [ ] **L5. Linux and Windows packaging.** cargo-dist is already in devenv.
- [ ] **L6. Move state-file I/O off the UI thread.**
  - The synchronous reads are at `main.rs:298` and `:311`.
  - The synchronous writes are at `:994`, `:1001`, `:1084` and `:1089`, plus a write-and-rename in a `spawn` on the UI executor at `:1051`.
  - Or correct the claim in `crates/app/Cargo.toml`.
- [ ] **L7. Git hygiene.** Add `.DS_Store` to `.gitignore` and remove the stale local `dist/`.

---

# Part 2: depends on other projects

## 2.1 Our own form libraries (tracked in #21)

Filed upstream on 2026-10-10. None of them blocks the release. The follow-up for each one, and the reinstall and bump procedure, are in #21.

| Id | Issue | When it lands, here we… |
|---|---|---|
| U1 | sagikazarmark/schemaform#57: opt-in format assertion | enable format findings and drop the "not checked here" notes |
| U2 | sagikazarmark/schemaform#58: public `ControlFacets::input_type()` | pick it up with the registry reinstall |
| U3 | sagikazarmark/schemaform#59: form-level read-only / disabled | delete the `FREEZE_FORM` script |
| U4 | sagikazarmark/schemaform#60: submission mode reachable from finding renderers | stop re-providing `AdvisoryPresentation` |
| U5 | sagikazarmark/schemaform#61: `date-time` can't produce RFC 3339 | render `date-time` as an RFC 3339 field and drop the X8 note |
| U6 | sagikazarmark/schemaform#62: optional object-root assumption | keep the B3 host fix or switch to the option |
| U7 | sagikazarmark/dioxus-field#20: public id registration | bump and reinstall |
| U8 | sagikazarmark/dioxus-daisyui-components#33: opt-in extra widgets | drop `select` and `radio_group` and narrow `#[allow(dead_code)]` |
| U9 | sagikazarmark/dioxus-daisyui-components#34: `Input` attribute ambiguity | drop FM1's string-key workaround |
| U10 | sagikazarmark/dioxus-daisyui-components#35: host styling for the renderer's own buttons | drop FM6's interim CSS |
| U11 | sagikazarmark/dioxus-daisyui-components#36: unset boolean looks like `false` | drop FM5's wording |
| U12 | sagikazarmark/dioform#122: relax the exact Dioxus pins | re-evaluate dioform for `spawn.rs` |

Do **not** adopt dioform before the release. It would help the spawn form moderately, fits the core-owned MCP draft poorly, and would freeze the Dioxus patch version until U12 lands. FM1 gets most of the benefit with libraries already installed.

## 2.2 Third-party projects (tracked in #22)

| Id | Item | Workaround here |
|---|---|---|
| U13 | `dioxus-primitives` is a git dependency with no crates.io release (DioxusLabs/components); unconditional `palette`/`time` deps | Fine while `publish = false`. Consider vendoring for release builds. |
| T1 | dioxus-desktop opens any link scheme through `webbrowser` without `hardened` | S1 (before tag) |
| T2 | `dx bundle` ignores `[application] name` | R6 (before tag) |
| T3 | `block 0.1.6` future-incompatibility, via `cocoa` ← dioxus-desktop | Track it, and bump when Dioxus or wry drop `cocoa` |
| T4 | Advisories we can't fix: `glib 0.18.5`, `rand 0.7.3` (unsound); `fxhash`, `paste`, `proc-macro-error` (unmaintained). Dependabot runs fail on these. | Commented ignores in `deny.toml` (R8); re-check on each Dioxus or wry bump |
| T5 | MPL-2.0 dependencies (`cssparser`, `selectors`, `dtoa-short`, `option-ext`) | Notices via R8 |

---

## Tag checklist

1. R1–R9, C1–C8, S1–S2, B1–B7, X1–X8, D1–D7 and DOC1–DOC5 are done or explicitly deferred, with the reason noted here.
2. CI is green on macOS and Ubuntu.
3. The signed, notarized DMG launches on a clean Mac running the oldest supported macOS, from Finder, against an `npx`-installed agent (X2).
4. Testy is pinned, and `just` passes in the devenv shell.
5. The CHANGELOG is written, the version is set, and the tag is pushed.
