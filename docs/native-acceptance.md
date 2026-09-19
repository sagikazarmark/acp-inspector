# Native desktop acceptance runbook

Use this alongside the [desktop smoke matrix](desktop-smoke-matrix.md). It makes
the remaining work in [#9](https://github.com/sagikazarmark/acp-inspector/issues/9)
and [#10](https://github.com/sagikazarmark/acp-inspector/issues/10) repeatable.
An unchecked row is pending, not passed by another platform's evidence.

## Record the environment

Copy this into the issue comment or a dated execution section:

```text
Inspector commit/build:
OS/version and desktop/session (X11/Wayland where applicable):
WebView version (WebKitGTK / macOS and WKWebView / WebView2 runtime):
Screen reader/version, verbosity, interaction mode:
Speech engine/voice and audio output device:
Display scaling and actual content viewport sizes:
Agent command/version:
Fixture folder and SHA-256 hashes:
Results per row: pass / fail / blocked / not run
Evidence: Trace export, screenshots, speech capture/listener notes
```

Run at **1440 × 880** and **960 × 640 content viewports**. Native menu/title bars
may require a larger outer window; record the measured content size. Repeat the
visual checks in Inspector Light, Dark, and System with OS light and dark.
Use VoiceOver on macOS, NVDA or Narrator on Windows, and Orca on Linux. State which
reader was used; one reader's pass is not a claim about another.

Build Testy with `just testy` and the inspector with `just run`. In the inspector's
Connection form select the built Testy executable, no arguments, and an existing
absolute working directory. Record the native executable path rather than copying
Linux paths onto macOS/Windows. Check its `initialize` result in Trace: the current
fixture advertises image, audio and embedded context.

For attachment and MCP scenarios, use the portable Agent below instead. Testy
remains the fixture for Elicitation, permission, Session Settings and other matrix
rows; the portable Agent implements none of those callbacks.

## Portable attachment/MCP acceptance Agent

### Launch fields and export retrieval

- Command contains only the executable (`npx` is the example); its package
  `@zed-industries/claude-code-acp` belongs on one Arguments line. Flags and separate values
  need separate lines. Paths with spaces need no shell quotes. Verify the Command, Arguments
  and Working directory descriptions are reachable through their accessible descriptions.
- With the portable Agent below, launch once with empty cwd and once with `.`. Inspect
  `session/new`: both resolve against the inspector's working directory and send an absolute
  path. Check a multi-line flag/value pair actually reaches the Agent.
- Export through the Console, palette or native menu. Pending writing must leave the window
  responsive. The result names **temporary storage** and shows the complete selectable path.
  Activate **Copy export path**, paste into a text field, and compare with the displayed path.
  Check focus stays on Copy while feedback updates. Activate **Open containing folder** and
  locate the file; a missing/refusing desktop handler must produce visible failure with Copy
  still usable. Repeat an export: the earlier file must remain intact.
- Copy/move the file elsewhere for durable retention. Repeat keyboard/viewport checks at both
  supported sizes. Native folder success, clipboard failure and spoken feedback need platform
  evidence; callback tests alone do not establish them.

September 2026 Linux WebKitGTK/Xvfb check (Phase 6, uncommitted build): corrected fields launched
the Python fixture with separate `--prompt-capabilities` / `none` lines and cwd `.`; exported
`session/new` carried `/home/laborant/acp-inspector/.`. Copy used the real clipboard, confirmed
success, retained focus, and Ctrl+V pasted the exact JSONL path. The isolated desktop's `gio open`
failed and that failure appeared beside the path. Injecting refusal into both WebView clipboard
mechanisms displayed Copy failure; a second export kept the first file intact. At measured
960 × 640 and 1440 × 880 viewports, the launch dialog stayed within the viewport and the document
had no horizontal overflow; retrieval controls were visible at the smaller size. No native
folder-success or spoken-feedback claim is made. Build/check target: `/tmp/opencode/inspector-final`,
`CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`.

### Fixture launch

`scripts/acceptance-agent.py` needs **Python 3.9+**, with no packages, Testy build,
shell scripts or network access. Find the interpreter's absolute path by running
`python3 -c "import sys; print(sys.executable)"` on Linux/macOS or
`py -3 -c "import sys; print(sys.executable)"` in Windows PowerShell. Check its
version. Enter that printed path as the inspector's **Command**.

Set **Arguments**, one argument per line, to:

```text
-u
/absolute/checkout/scripts/acceptance-agent.py
--prompt-capabilities
all
--mcp-capabilities
all
```

On Windows the script line is an absolute Windows path, for example
`C:\work\acp-inspector\scripts\acceptance-agent.py`; on macOS it might be
`/Users/reader/acp-inspector/scripts/acceptance-agent.py`. Each path is a single
argument line even when it contains spaces: do not add shell quotation marks in
the inspector's Arguments field. Set **Working directory** to the absolute fixture
folder, and leave **Environment** empty. The Agent communicates on stdin/stdout
as UTF-8 JSONL; do not launch it by opening the `.py` file in an editor.

| Scenario | Argument lines to change/add | Drive and expected evidence |
|---|---|---|
| All attachment kinds | `--prompt-capabilities` then `all` | A1–A13: ordinary prompts return an ordered receipt and `end_turn` |
| No attachments | `--prompt-capabilities` then `none` | A14: attachment input absent; text prompts still work |
| Independent attachment gates | Value `image`, `audio`, or `embeddedContext` | A14: each claim independently controls admission; PDF belongs to embedded context |
| Mixed subset | Value `image,embeddedContext` | Images/text/PDF offered without audio |
| MCP gates | `--mcp-capabilities` then `none`, `http`, `sse`, or `all` | Stdio is baseline in all cases; unsupported HTTP/SSE drafts block first Session in inspector |
| Load replay | `--restore` then `load` | Send a prompt, open a second Session, list and reopen the first; saved receipt updates replay before load answer |
| Resume only | `--restore` then `resume` | List/reopen without replay; MCP definitions supplied by this request replace the fixture's receipt summary |
| No restore | `--restore` then `none` | Listing is available, neither restore method advertised |
| Delayed initialize | `--delay` then `initialize=2` | Edit MCP draft during handshake; first setup uses captured input, next opening uses edited input |
| Delayed setup | `--delay` then `session/new=2` (or load/resume) | Observe pending setup; response and Session mutation happen after delay |
| Delayed Turn | `--delay` then `session/prompt=2` | Stop before completion; exactly one `cancelled` answer, no later receipt/end_turn |
| Held Turn | `--hold-prompts` | Prompt waits until Stop, Session switch cancellation or close/delete; listing/new requests stay responsive |
| Refusal | `--refuse` then `session/new`, `session/load`, `session/resume` or `session/prompt` | Deterministic error with no requested operation applied; combine with delay for a slow refusal |

Capability values can also be comma-separated subsets. Defaults are `all` for
both capability groups and `both` for restore (the inspector prefers load).
`--delay` and `--refuse` may repeat for different methods; a repeated delay for the
same method uses the last value. Delays accept 0–30 finite seconds. A refusal uses
`-32602` by default; `--error-code` accepts a signed 32-bit code. Refusal takes
precedence over hold behavior, so `--hold-prompts --refuse session/prompt` refuses
rather than holding. A delayed prompt refusal still represents a pending Turn:
Stop cancels it and suppresses the scheduled error. Refusal remains active for that process; reconnect with it
removed to test recovery. Run `python scripts/acceptance-agent.py --help` for all
supported methods, or `--version` to record the fixture version.

The fixture never starts MCP subprocesses, opens URLs, or persists Sessions.
Reconnect starts again at `acceptance-1`; restoration is within one process only.
Close cancels any pending Turn but keeps the Session in the fixture's listing;
delete also removes it. Empty/missing MCP lists mean no definitions here, not a
claim about other Agents' inheritance semantics. EOF stops immediately, abandoning
pending delayed replies; diagnostic/CLI errors use stderr, never protocol stdout.
Incoming Frames above 12 MiB are refused and the process exits (the inspector's
outgoing prompt limit is 10 MiB). This fixture's choices are test behavior, not ACP
conformance assertions about other Agents.

### Reading receipts

The reply is a text update containing JSON: Session ID, ordered `blocks`, and an
ordered MCP summary. Each content block has decoded UTF-8/binary `bytes` and
`sha256`, plus MIME/URI where supplied. Compare those hashes with the fixture
folder's hashes. The Agent does not decode media or infer its MIME; it reports
what it received. A valid base64 payload is not proof of a valid PDF/image/audio.

MCP summaries include name, transport and argument/environment/header **counts**.
Use the outgoing setup Frame to verify exact URLs, commands, empty arguments,
duplicate names and header values. The receipt deliberately avoids echoing full
attachments or credential values; Trace still contains the original request.
Load replays saved receipt updates before answering; resume emits none. Cancelled
or refused Turns add no receipt to replay.

Run the real-pipe tests from the checkout root:

```sh
python3 -m unittest discover -s scripts -p test_acceptance_agent.py -v
```

Use `py -3` instead of `python3` on Windows, or `just test-acceptance-agent` where
`python3` is available. These checks exercise the portable protocol process, not
native screen readers or platform WebViews.

## Linux remaining acceptance (#10)

Run on a desktop with working speech output. Confirm Orca can audibly announce an
ordinary application control before starting. Debug `SPEECH OUTPUT` lines show
generated utterances but do not establish audio delivery or usable interruption
timing. Do not change global screen-reader preferences without recording/restoring
them.

1. Prompt Testy with `elicitations`. Reach the ten-field form using the keyboard.
2. Enter Age `999`, leave the field, then refocus it. Hear its label, value and
   **Above the maximum of 120.** finding. Record automatic live announcements
   separately from descriptions spoken on refocus.
3. Reach Email and hear **Format email: not checked here.**
4. Activate the Age summary link. Confirm focus moves to Age without submitting.
   Change it to `20`, leave and refocus. The maximum finding and description should
   disappear; the old finding must not be announced as current.
5. Restore `999`, leave the field and activate Accept. It must send the committed
   value despite the advisory finding. Confirm focus remains on Accept while it
   becomes unavailable, then activate the resolved Age summary link. Its field
   remains focusable and read-only with value `999`.
6. Complete remaining requests, recording the answers; verify `end_turn` in Trace.
7. Select System appearance and change the desktop's own appearance light → dark
   → light **without restarting** the inspector. Verify backgrounds, fields,
   findings, focus and scrollbars update. Inspecting the DOM, if available, should
   show no `data-theme` and changing `prefers-color-scheme`. Explicit Inspector
   Light/Dark must override the opposite desktop preference.

The recorded XSettings run establishes live GTK/WebKit propagation in an isolated
X11 session. Repeat step 7 with the actual GNOME/KDE/other desktop setting and
record the integration used. Audible steps remain open until a listener verifies
them.

## macOS/Windows Elicitation acceptance (#9)

Run the Linux steps above with the native reader and desktop settings, plus the
matrix's Elicitation rows. In particular:

- Confirm numeric bounds, Unicode string length, required presence, choice counts
  and parse-buffer findings have usable labels/descriptions and summary targets.
- Verify a parse-buffer finding explains which committed value would be sent.
- Switch Form → Raw → Form; edits persist independently. Accept from Raw can send
  a type the schema forbids, as shown in the outgoing Frame.
- Exercise URL consent before completion, opening in the external browser, and
  the Agent's later `elicitation/complete`. Record focus and speech at each step.
- Run `callbacks`, answer its permission and Elicitation requests, and verify the
  final `end_turn`. Check both sizes and all appearance combinations.

## Prepare attachment fixtures

Use a dedicated local folder containing no private documents. Use native apps to
create a small PNG, JPEG, GIF and WebP, a playable WAV and MP3, and a valid one-page
PDF. Record hashes before selection. Give at least one file spaces, `#`, and
non-ASCII characters, for example `report # café.pdf`; avoid `?`, which is not a
portable Windows filename character. Include the same basename in two folders.

Use Python 3 to create the boundary/text fixtures below. Run the snippet from the
fixture folder (paste it into Python or save it as a temporary script). Sizes are
original bytes, not base64 or JSON length:

```python
from pathlib import Path
import hashlib

Path("context # café.rs").write_bytes(
    b"\xef\xbb\xbf" + '// café\r\nfn main() { println!("<literal>"); }\r\n'.encode("utf-8")
)
Path("invalid-utf8.txt").write_bytes(b"\xff\xfe\x00")
Path("broken.png").write_bytes(b"not a PNG")
Path("five.txt").write_bytes(b"x" * (5 * 1024 * 1024))
Path("one.txt").write_bytes(b"y" * (1024 * 1024))
Path("extra.txt").write_bytes(b"z")
Path("too-large.txt").write_bytes(b"x" * (5 * 1024 * 1024 + 1))
Path("escaped.txt").write_bytes(b"\t" * (5 * 1024 * 1024))
for path in sorted(Path(".").iterdir()):
    if path.is_file():
        print(path.name, path.stat().st_size, hashlib.sha256(path.read_bytes()).hexdigest())
```

`escaped.txt` fits the per-file budget but exceeds the complete 10 MiB Frame limit
after JSON escaping and envelope overhead. Large source files show only a bounded
preview; do not infer content truncation from that preview.

## Native attachment checklist

For each row, record **macOS** and **Windows** results separately. Use Finder and
Explorer for native drops; synthetic JavaScript drop events are not substitutes.
On Windows this is especially important because Dioxus synthesizes DOM drop events
from its native dispatcher. On macOS enable keyboard navigation to all controls as
needed and record that preference.

| ID | Drive | Required observation/evidence |
|---|---|---|
| A1 | Open Attach media with the keyboard; select PNG + WAV + source text + PDF | Native picker closes, four rows appear in the order returned by the platform. Names, MIME and original byte counts are announced/readable; each Remove control names its file. Do not assume click order equals returned order. |
| A2 | Drop another image and MP3 from Finder/Explorer onto the composer | Appends through the same draft; no navigation. Repeat selection of a file to confirm intentional duplicates. |
| A3 | Remove first/middle/last rows, including while a large file is reading | Only intended rows disappear and no late completion restores them. Remaining order is unchanged. |
| A4 | Paste a native screenshot/image, then ordinary text, then a source offering both text and an image | Image adds one attachment per supplied file; text edits the composer normally. Record the source application and representations. Clipboard bytes may differ from the original disk file; compare against bytes the platform supplied. |
| A5 | Paste an image then immediately Send; repeat with a subsequent picker/drop | Pending discovery/reads prevent partial Send; arrival order is preserved. Try text-only paste with seven/eight ready rows: it must not consume an attachment slot or leave a false overflow notice. |
| A6 | Add `broken.png`, invalid UTF-8 and `too-large.txt` with valid files | Failed rows remain visible, valid files remain, and Send waits for failures to be dismissed. Add a ninth row and dismiss the count notice explicitly. |
| A7 | Add `five.txt` + `one.txt`, then `extra.txt` | Exactly 6 MiB is admitted. The extra byte causes a visible failed row; no silent omission. |
| A8 | Add `escaped.txt`, then Send | Complete-Frame refusal crosses no `session/prompt`; draft remains editable. Remove it and send a small file successfully on the same Connection. |
| A9 | Play WAV and MP3 by keyboard, then pause/seek/remove | No autoplay. Player controls and state are accessible; record audible playback separately from time progression. A decode failure explains unavailable playback while original bytes remain sendable. |
| A10 | Expand source preview; inspect PDF row | Source markup is literal, BOM/line endings remain in outgoing text, preview is bounded. PDF shows metadata with no embedded viewer/navigation. |
| A11 | Send text plus image/audio/source/PDF; export Trace | One prompt text block precedes ordered attachments. Images/audio carry correct MIME/base64; text resources carry exact UTF-8; PDF carries `application/pdf` blob. Decode base64 and compare SHA-256 with fixture hashes. Native file URIs escape spaces/`#`/Unicode and retain the selected path (including Windows drive/UNC form where available). |
| A12 | Open a new Session while a file is reading or clipboard discovery is pending | New Session starts with an empty draft; old work cannot repopulate it. Trace retains earlier traffic; a previously admitted prompt stays bound to its original Session. |
| A13 | Drop a URL/text; drop a file outside composer; drag an existing preview within the page | No replay of cached native paths, no unexpected attachment, no window navigation. |
| A14 | Repeat with Agents advertising none, only image, only audio, only embedded context | Offered controls/hints follow each Advertisement. Wrong-kind drops become failed rows where attachment input is available; none advertised offers no attachment input. PDF uses embedded context, not the image claim. Save each initialize Frame. |
| A15 | Traverse draft, previews, Remove and Send using the native reader at both sizes/themes | Names/descriptions, visible focus, local scrolling and failure feedback remain usable; document has no horizontal scrollbar. |

Use the portable Agent's `--prompt-capabilities` combinations for A14;
Testy's all-advertised run alone cannot pass that row. Record the command and
initialize Frame for each combination. In every sending row record the Agent's
actual outcome; accepting the embedded shape is not proof of PDF interpretation.

## MCP transport selector keyboard checks

Prepare one named MCP row in Launch. With no next Agent Advertisement known, all
three transports are selectable. Navigate using Tab/Shift+Tab, then verify:

1. Select stdio, HTTP and SSE; Tab reaches Command for stdio and URL for HTTP/SSE.
   Enter a URL and ordered headers by keyboard. Switch transports and back: values
   survive, while only the selected transport's fields are sent.
2. Launch Testy with SSE selected. Testy advertises HTTP but not SSE: initialize
   answers, no Session-opening Frame crosses, and Sessions shows the retained SSE
   row and finding. Use the keyboard to select HTTP and open a Session. The
   finding and stale launch failure clear, Sessions closes, and Trace contains HTTP.
   Alternatively use the portable Agent with `--mcp-capabilities` / `http` for the
   same gate. Use `sse` to test the inverse or `none` to correct to stdio.
3. Reconnect: the Launch selector allows SSE again despite the previous Agent's
   claims. The next initialize determines whether that draft may open a Session.
4. Repeat at 960 × 640 and 1440 × 880 content sizes. Verify field labels, header
   controls, visible focus and local scrolling. Repeat with the platform reader;
   DOM labels and keyboard reachability alone do not establish spoken acceptance.

On Linux WebKitGTK 2.52.4, an arrow on a **closed** select commits immediately.
Follow it with Tab, **not Enter then Tab**: that extra Enter opens the popup and
Tab stays there. To choose within the popup, use Space → arrow → Enter → Tab.
Escape closes an accidentally opened popup without changing the committed value.
Record the native conventions on macOS/Windows rather than assuming this behavior.

### Repeatable Linux probe

Requirements: Node with global WebSocket (tested Node 24), `xdotool`, a focused
Linux inspector window, and WebKit remote inspection enabled at launch with
`WEBKIT_INSPECTOR_HTTP_SERVER=127.0.0.1:9223`. Keep inspection local and stop it
afterward. The script uses real X11 keyboard events and read-only DOM inspection;
it never focuses controls or dispatches DOM input events itself. It exits after
12 seconds if inspection cannot complete. It is an opt-in acceptance tool, not a
portable CI test. `DISPLAY` must identify the inspector's X11 session; override
`WEBKIT_DEBUG_SOCKET` if the WebKit page socket differs from the default.

With the launch dialog open, one server row present, stdio selected, and focus on
its Command field:

```sh
node scripts/check-mcp-keyboard.mjs http shift+Tab Down Tab
node scripts/check-mcp-keyboard.mjs sse shift+Tab Down Tab
node scripts/check-mcp-keyboard.mjs http shift+Tab space Up Return Tab
node scripts/check-mcp-keyboard.mjs stdio shift+Tab Up Tab
```

Each command asserts the committed transport, visible field family and final
focus. A deliberate reproduction from stdio's Command field is
`node scripts/check-mcp-keyboard.mjs http shift+Tab Down Return Tab`: it must fail
the focus assertion on this WebKitGTK version. Recover with
`node scripts/check-mcp-keyboard.mjs http Escape Tab`. The failed sequence is a
driver mistake, not an application regression to "fix" by overriding native keys.

## Focused navigation check — 2026-09-19

On Linux/X11 under Xvfb with the devenv WebKitGTK, the phase4 working tree passed a focused
navigation probe using `xdotool` input and read-only remote DOM inspection (the same mechanism as
the MCP probe above). Actual content widths were **921px** and **1382px**. The portable acceptance
Agent supplied a prompt and a received update.

- Palette Capabilities and Session settings revealed Details and focused the selected rail tab.
- Palette Diagnostics and native Ctrl+2 revealed Messages and focused the Diagnostics tab.
- Toolbar JSON-RPC full selected Messages and retained focus on the layout control.
- Mobile Session after full Wire selected Split, revealed Timeline and focused its region.
- Trace `turn` from narrow/full Wire and wide/full Wire revealed and focused the marked entry.
- Timeline evidence revealed Trace and focused the marked Frame row.
- Native Ctrl+backquote from the composer moved focus only when hiding it; returning to Split
  retained Console focus and preserved the typed draft.

This establishes Linux WebView visibility and focus behavior only. It is not a screen-reader,
macOS/Windows, exact-size or full acceptance walkthrough. The session's probe and launch helper
are `/tmp/opencode/check-navigation.mjs` and `/tmp/opencode/launch-navigation.py`; the build is in
`/tmp/opencode/inspector-final`. Those temporary files are local evidence, not portable test tooling.

**Waiting-request regression, same environment:** launch `.testy/bin/testy`, send `callbacks`,
and leave its request unanswered with Timeline visible. Before visiting that row from Trace, run:

```sh
DISPLAY=:94 node scripts/check-waiting-navigation.mjs
```

Use the actual X11 display and the remote-inspection setup above. The inspector window must have
native keyboard focus. The probe Tabs to the real **go to it** control, activates it, and asserts
that an unmarked request row receives focus. This failed when the shared focus helper required
`.sought` (focus stayed on the button), then passed after readiness became row visibility alone.
The bounded paint wait still covers cross-screen navigation into the always-mounted Timeline.

## Report remaining blockers

Attach evidence to #9 or #10 using the IDs above and the matrix Elicitation step
names. For failures, include reproduction steps, expected/actual behavior and
whether focus, accessible metadata, generated speech or audible speech failed.
Do not paste complete private Frames or unreviewed screen-reader debug captures;
the fixtures above provide shareable evidence. Keep the issue open for unrun or
blocked native rows rather than treating Linux/SSR/browser evidence as a pass.
