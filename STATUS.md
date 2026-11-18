# Cueglint continuation record

The full [delivery goal](docs/delivery-goal.md) is **active**: all eight workstreams,
all 73 feature contracts and all twelve acceptance families. Desktop acceptance is
the first release milestone; the four companion contracts remain open. The
[registry](parity/features.yaml), [roadmap](ROADMAP.md) and generated
[progress](docs/progress.md) keep observation, implementation and verification
independent. No desktop or whole-product parity is claimed.

Repository: https://github.com/thematheusmello/cueglint.
Working branch: `feat/capture-input-selection`, [draft PR20](https://github.com/thematheusmello/cueglint/pull/20).
Inspect its [current checks](https://github.com/thematheusmello/cueglint/pull/20/checks)
at the exact returned head. The tested implementation is
`e4a46779d4c26f172066d41552df86bde5909319`; a later copy-only change at `71d5cba`
clarifies that audio listening and question/preview screen sampling are separate.
The frozen native test executable remains at e4a46779. Inspect the branch's actual
current PR/CI before merging; new-head hosted results remain separate.

The latest independently audited merged checkpoint is
[PR19](https://github.com/thematheusmello/cueglint/pull/19), main
`2d859e3982b0ac926c2a323c4683dff505e2cbba`. Its exact tree passed Ubuntu302,
macOS304 and Windows304 Rust tests, with zero failures/filtering and3/4/4 ignores.
Thirteen original native GPU fixtures and bounded child startup also passed.
[Artifact review](parity/reports/pr19-hosted-ci-artifact-001.json) binds the head,
CI merge and main merge. Five earlier concurrency cancellations and previous
PR16/17/18 evidence remain retained. These checks predate the new selectors.
No CI test opened a real Windows audio device.

The [current local workspace](parity/reports/capture-inputs-integrated-workspace-001.json)
passed **312 tests, zero failed, four explicit ignores, zero filtered** at e4a46779,
following [250 targeted checks](parity/reports/capture-inputs-targeted-workspace-001.json).
Explicit microphone/display selection, bounded catalog refresh, retained missing
IDs/errors, legacy recovery and common macOS display targeting are implemented.
Source endpoints were unchanged. Windows/Linux hardware and live route behavior
remain unverified. The previous [304-test menu check](parity/reports/provider-menu-workspace-001.json)
remains separate. Both provider/model menus close after semantic
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

The separate [native local text workflow](parity/reports/native-local-session-001.json)
uses the actual development-signed e4a46779 executable and verified Qwen1.5/Candle
CPU. One original question returns exact WREN-42. A manual note survives End,
normal quit, a separately parsed JSON export and history reopen in the same signed
file. Native input selection closes and persists its ID; the saved-ID fallback
label appears until a fresh catalog is requested after relaunch. Independent
review verifies47 capture hashes,427 source blobs and selected model bytes.
Recorded1384ms covers the generate call, not first-token or user latency. Audio,
screen, full offline, broad model quality and first-use acceptance remain open.

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
test-only. A [bounded used-font investigation](parity/reports/reference-used-font-bounded-001.json)
found no actual rendered-font evidence or existing debugging endpoint. Do not
repeat the same probes or silently change the selected baseline or tokens.

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
These historical runs do not establish true dual-source capture, broad quality,
current screen-target freshness or a full network-blocked meeting. Earlier failures
remain in [verification](docs/verification.md) and `parity/reports/`.

The stable `dist` package remains sourcea9ca43a: GUI SHA256
`0ff61d45dcffbb0d386acce169106fde072e26bdb6d630b6cef2b717718a8154`,
CLI `dbc1d13c74a1406c239c13090d39f84d8b25559f932da06f11dd0750969e9cff`.
It has not been overwritten by isolated tests. The new local development-signed
GUI is separately frozen at SHA256
`fd7568a3e4936e8e887aaaaf9df74e3a5023a0695e8cd61c7c93b7cd86b000ec`;
CLI `6b9a77a2c3ade925b4aba79f790e80ba801731d1cf499302b6cd39eecfe468d0`.
Strict signature checks pass; notarization, clean install, real capture, complete
offline, updates and release distribution remain open.

Computer control is reconnected. The current frozen app is idle with a saved,
stopped session and audio sources off. Its display query reports access required
despite both existing Cueglint permission switches being on after relaunch. A
targeted refresh now requires macOS Touch ID; the specific user intervention is
pending. Leave the signed app and OS authorization sheet untouched until that
handoff ends. Do not repeat the generic enable/reconnect request or treat elapsed
time as authorization. Exact private receipts, active build handles and agent state
are in `.private/continuation.json`; read `current_execution` first.

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
3. The six-run diagnostic has not executed models. Earlier preparations and the
   first missing-dependency build failure remain retained. Inspect its corrected
   build status, then separately validate parser/preparation and release execution.
   Keep model, prompt IDs, fixtures, limits and no-retry policy frozen.
4. Finish the specific pending OS authentication handoff only after the user
   responds. Verify the same frozen app; prepare controlled original screen/audio
   fixtures before capture. Then run distinct real microphone/system audio,
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
