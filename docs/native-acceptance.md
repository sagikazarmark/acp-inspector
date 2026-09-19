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

Use a controlled Agent whose initialize Advertisements can be changed for A14;
Testy's all-advertised run alone cannot pass that row. Record the command and
initialize Frame for each combination. In every sending row record the Agent's
actual outcome; accepting the embedded shape is not proof of PDF interpretation.

## Report remaining blockers

Attach evidence to #9 or #10 using the IDs above and the matrix Elicitation step
names. For failures, include reproduction steps, expected/actual behavior and
whether focus, accessible metadata, generated speech or audible speech failed.
Do not paste complete private Frames or unreviewed screen-reader debug captures;
the fixtures above provide shareable evidence. Keep the issue open for unrun or
blocked native rows rather than treating Linux/SSR/browser evidence as a pass.
