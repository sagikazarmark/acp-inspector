# Phase 5 performance / retention evidence

Measured in the Linux development environment, **unoptimized test/debug profiles**, September
2026. These are comparison probes, not release throughput promises. Build target:
`/tmp/opencode/inspector-final`; `CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`.

| Probe | Before | After |
|---|---:|---:|
| 100 summaries of one 9 MiB immutable Frame (includes cold first read) | 2,279 ms | 21 ms |
| SSR of 10,000 retained Trace Frames, 512-byte payloads | 957 ms | 79 ms |
| Trace SSR HTML / rendered rows | 12,728,945 bytes / 10,000 | 250,367 bytes / 200 |
| Real stdio: 15,000 streaming updates + 15,000 1-KiB stderr lines + eight 9-MiB Frames | 1.66 s ingest | 2.09 s ingest |
| 100 complete store snapshot rounds after that traffic | 975 ms | 139 ms |
| Retained Trace / Timeline / diagnostics after stress | 10,000 / 15,008 / 15,001 | 7 / 1 / 7,785 |

Ingest has additional retention bookkeeping and is slightly slower in this probe; the gains are
bounded evidence memory and much less UI work. Timings include process startup and scheduling;
the two core probes ran concurrently. Raw byte budgets do not count decoded allocation overhead
or promise an RSS ceiling. Budgets and their exceptions are in architecture §10.

Reproduce:

```sh
CARGO_TARGET_DIR=/tmp/opencode/inspector-final CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test -p acp-inspector-core --test performance -- --ignored --nocapture
CARGO_TARGET_DIR=/tmp/opencode/inspector-final CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test -p acp-inspector rendering_probe -- --ignored --nocapture
```

## Native WebKitGTK / Xvfb

Debug binary with software rendering, Xvfb `:94`, WebKit inspector `127.0.0.1:9223`.
Launch `/usr/bin/python3`, arguments `scripts/performance-agent.py` as an absolute path, cwd
`/tmp/opencode`. Prompt `stream`, then `large`. The first emits 15,000 chunks and noisy stderr
(100 chunks every 10 ms); the second emits eight 9-MiB raw Frames. The Agent stays connected.

Observed with `/tmp/opencode/performance-native.mjs`:

- Stream settled at 200 rendered Trace rows, 769,971 bytes of complete document HTML; Timeline
  explicitly reported 14,000 removed entries/Frames and 5,896,890 raw bytes. Largest observed
  interval for a 50-ms WebKit heartbeat: 202 ms (includes streaming and rendering).
- Older twice reached older ordinals; Timeline evidence navigation revealed an older paged
  Frame (ordinal 14605), rather than merely selecting invisible content.
- After large Frames: eight retained Trace rows, 242,402 bytes of document HTML; Timeline
  reported 15,007 removed entries/Frames and 72,379,745 bytes. Raw detail uses byte windows.
- Background export completed with header `frames: 8`, `dropped: 15008`. Full-frame Copy of
  a large Frame carried 9,437,263 UTF-16 characters including the final `🦀END`; its length and
  whole-string checksum matched a raw Frame in that exported file. Core regression separately
compares complete exported raw strings byte-for-byte after rotation and Clear.

## Regression coverage and limits

`retention.rs`: byte-triggered Trace rotation, exact raw export, immutable activation snapshot,
and pending permission + elicitation still answerable after 2,500 updates, with counted holes.
Timeline unit tests: merged tool evidence leaves whole, later patch has a new index, and empty
Turn retention never reuses ids. App tests: bounded SSR, older ordinal reveal, literal previews,
shared export pending/dedup/error/retry, and a blocked writer leaving the single-thread executor
free. Existing numeric, conformance, exit-drain and navigation regressions remain relevant.

Executed checks: app suite **294 passed / 1 ignored** at the main implementation checkpoint;
subsequent focused export, Console and raw-window interaction tests passed. Focused core run
covered **112 passing tests** across unit, conformance, elicitation, exit-drain, export, numbers,
permission, retention, turns and typed decoding; follow-up tests additionally cover diagnostic
byte pressure, annotation/evidence eviction, and late completion after eviction. Envelope tests
and all six numeric regressions also passed with both `arbitrary_precision` and `float_roundtrip`
enabled. `cargo fmt` and `git diff --check` passed. No release-profile claim is made.

Remaining limits: filtered search scans retained raw text; first envelope summary still scans a
Frame; subscriptions copy bounded snapshots rather than deltas; large decoded content and many
simultaneous Blocking Requests can still be expensive. Timeline keeps newest eligible evidence
rather than archiving it; exports only contain what Trace held on activation. Large raw Frames
are intentionally unformatted in byte windows. Release/platform latency and Save dialogs are
outside this work.
