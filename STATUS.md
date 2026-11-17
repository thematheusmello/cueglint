# Cueglint continuation record

The full [delivery goal](docs/delivery-goal.md) is **active**: all eight workstreams,
all 73 feature contracts and all twelve acceptance families. Desktop acceptance is
the first release milestone; the four companion contracts remain open. The
[registry](parity/features.yaml), [roadmap](ROADMAP.md) and generated
[progress](docs/progress.md) keep observation, implementation and verification
independent. No desktop or whole-product parity is claimed.

Repository: https://github.com/thematheusmello/cueglint.
Working branch: `feat/notes-salience-v2`. The current implementation checkpoint is
`136a7c7ac6b330c310a5f07734960f7ccb49cdc2`; subsequent changes in this increment
record documentation and evidence. Check the branch's actual current PR/CI before
merging; later-head hosted results are separate from the receipts below.

The latest independently audited merged checkpoint is
[PR18](https://github.com/thematheusmello/cueglint/pull/18), main
`d78732f778c144690ffd5fd33cb824a9ce70b59e`. Its exact tree passed Ubuntu297,
macOS299 and Windows299 Rust tests, with zero failures/filtering and3/4/4 ignores.
Thirteen original native GPU fixtures and bounded child startup also passed.
[Artifact review](parity/reports/pr18-hosted-ci-artifact-001.json) binds the head,
CI merge and main merge. Previous PR16/17 failures, capacity cancellations and
reviews remain retained. No CI test opened a real Windows audio device.

The [current local workspace](parity/reports/provider-menu-workspace-001.json)
passed **304 tests, zero failed, four explicit ignores, zero filtered** at136a7c7.
Source endpoints were unchanged. Both provider/model menus close after semantic
activation; eight AX/Enter new/reselect combinations are covered. Custom database
paths also isolate eframe UI persistence in the corresponding `.ui.ron` sidecar.
The earlier [303-test storage check](parity/reports/ui-storage-workspace-001.json)
and [300-test prompt check](parity/reports/salience-v2-workspace-001.json) remain
separate source-bound implementation evidence.

The [real native macOS provider flow](parity/reports/native-provider-ui-004.json)
uses one isolated ad-hoc signed app and a synthetic loopback compatible server.
It checks masked key entry, Keychain save/cleared field/stored status, model list,
connection probe, explicit provider selection and one floating-assistant `OK`
response. The session is ended/saved, the key deleted/rechecked missing and Local
explicitly restored before normal quit. Both owned fixture servers stop; all five
checked default application-data endpoints are unchanged. Independent review
checks original captures and the persisted completed response. There is no real
inference, vendor account, audio/screen capture or offline-meeting claim here.

The [native002 menu/AX failure](parity/reports/native-provider-ui-002-failure.json)
and [native003 incomplete-fixture failure](parity/reports/native-provider-ui-003-failure.json)
remain failed. The corrected fixture adds its missing terminal event without
changing application validation. Physical pointer focus plus keyboard entry works;
AX activation followed by keyboard entry remains an unresolved accessibility and
automation check. No global deadlock was established. Private diagnostic
instrumentation is parked and must not be merged or published by default.

[Actual highlights003](parity/reports/highlights-quality-actual-003.json) at9484829
**still fails** the unchanged salience gates after generic promptv2. Both original
cases select0..7: precision0.50, recall2/3, category coverage0.60 and critical
coverage0.75; source support and planner coverage are1.0. One generation per case,
no retries/repair/fallback. Ready drafts remain unaccepted, manual notes intact;
accept/append checks did not run. [Independent audit](parity/reports/independent-highlights-quality-actual-003.json)
retains all owned-process cleanup and identities. Actual001/002 remain failed.
Do not replay unchanged models/cases or loosen scoring. A separately reviewed,
bounded decoder/ordering/runtime diagnostic is the next quality investigation;
its new fixtures cannot establish held-out quality or change defaults by themselves.

The selected reference remains **macOS Cluely2.1.28**. The installed2.1.29 has a
separate [readable General handoff](spec/features/reference-general-visible-2.1.29.md)
and [measurements](ui/measurements/reference-general-visible-2.1.29-001.json):
supported Undetectability was temporarily off, then restored on. Raw commercial
captures remain private. The original transparent observation is retained. Used
font metrics, historical2.1.28 scale, blur/motion and matched visual acceptance
remain unknown. Production fonts still use the legacy loader; the OS resolver is
test-only. Do not silently change the selected baseline or tokens.

Working application paths include native compact/expanded windows and Settings,
sessions/history, editable notes, modes, bounded UTF-8 knowledge, versioned exports,
transactional backup/restore, CLI access and cancellable local generation. Explicit
BYOK routes cover OpenAI, Anthropic, Gemini, Perplexity, OpenRouter and compatible
endpoints. All six controlled protocols and the narrower native/CLI checks are
recorded. Live vendor/model coverage and Windows vault runtime remain open.
Local/open models are default; there is no automatic cloud fallback. Keys stay in
OS credentials, never in chat or exported data.

Historical model results are source-specific: English base.en file-ASR passes one
12-speaker accuracy/realtime check, while the complete CPU suite fails speed and
the licensed synthetic waveform still fails WER. A separate Metal worker passes
fixed warm gates with unchanged transcripts and a retained10.58-second first
warmup. Optional llama.cpp/Qwen produces two persisted answers for one question.
These do not establish packaged inference, true dual-source capture, broad quality,
current screen-target freshness or a full network-blocked meeting. Earlier failures
remain in [verification](docs/verification.md) and `parity/reports/`.

The stable `dist` package remains sourcea9ca43a: GUI SHA256
`0ff61d45dcffbb0d386acce169106fde072e26bdb6d630b6cef2b717718a8154`,
CLI `dbc1d13c74a1406c239c13090d39f84d8b25559f932da06f11dd0750969e9cff`.
It has not been overwritten by isolated tests. The current GUI test artifact is
separately frozen in its report. Notarization, clean install, real capture, offline
acceptance, updates and release distribution remain open. The user's earlier
Screen Recording enablement is recorded; freeze the intended next bundle before
any specific permission diagnosis. Do not repeat the broad permission question.

Computer control is reconnected and native interaction has run. No app, fixture
server, Cargo or model job from this native check remains live. Exact private
receipts, current agent state and continuation handles are in
`.private/continuation.json`; read `current_execution` before historical sections.

The authorized path-only WAV history cleanup is complete. See
[history cleanup](docs/history-cleanup.md). Do not repeat it. Closed PR refs/caches
may retain old media; mapping/evidence remains private. Newly authored code is
MIT OR Apache-2.0, with separate fixture, dependency and model terms preserved.

Next exact actions:

1. Inspect actual Git status/head, active goal and `current_execution`. Reuse live
   handles if any; do not restart a completed job or overwrite user preferences.
2. Finish current increment's formatting, registry/progress and relevant checks,
   commit/push and inspect the exact returned PR/CI. Independently review hosted
   artifacts before merging; source tests do not establish device capture.
3. Review the proposed six-run decoder/ordering/official-runtime diagnostic before
   any new inference. Keep model, prompt IDs, fixtures, limits and no-retry policy
   frozen. Diagnose the failed selection without weakening the quality gate.
4. Freeze the intended package and run distinct real microphone/system audio,
   fresh selected screen context, local streamed answer, stop/finalize/save/reopen;
   then the complete offline workflow with traffic inspection. Resolve only a
   specific actual missing OS/user action after independent work is prepared.
5. Complete remaining UI/window/IME/accessibility, notes/knowledge/templates/history,
   settings/calendar/integrations, recovery, capture recipients and sustained use.
   Continue macOS/Windows clean install/signing/update, explicit Linux runtime,
   companion and optional open-source sharing/sync contracts through full closure.

Before publishing run affected tests, `cargo fmt --check`, registry and generated
progress checks. Reuse the shared target with two build jobs and a6GiB free-space
floor; ensure `xtask` was compiled for this worktree. No proprietary binaries/raw
captures, credentials, personal content or private machine paths belong in public CI.
