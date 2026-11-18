# Cueglint continuation record

The full [delivery goal](docs/delivery-goal.md) is **active**: all eight workstreams,
all 73 feature contracts and all twelve acceptance families. Desktop acceptance is
the first release milestone; the four companion contracts remain open. The
[registry](parity/features.yaml), [roadmap](ROADMAP.md) and generated
[progress](docs/progress.md) keep observation, implementation and verification
independent. No desktop or whole-product parity is claimed.

Repository: https://github.com/thematheusmello/cueglint.

Main includes the [native Settings repairs](https://github.com/thematheusmello/cueglint/pull/32),
[macOS SDK root fix](https://github.com/thematheusmello/cueglint/pull/33),
[mode-scoped files](https://github.com/thematheusmello/cueglint/pull/34) and
[mode duplication](https://github.com/thematheusmello/cueglint/pull/35). The
mode-duplication merge `e1f655617acfd5171cca0cfc25370f1128f6e362` matches the tested
CI tree. Windows and macOS passed 394 tests each, Ubuntu passed 392, and the
native job passed 13 fixture frames and child startup. No tests failed. The
[mode scope report](parity/reports/native-mode-scope-001.json) and [duplicate
and reopen report](parity/reports/native-mode-duplication-001.json) record separate
native workflows with original fixtures.

Main also includes the [macOS development package evidence](https://github.com/thematheusmello/cueglint/pull/36)
at `12f052cdba1598c12724b1df2ed7308060cd1c4a` and
[separate chats and meetings](https://github.com/thematheusmello/cueglint/pull/37)
at `57384cecd2c33d5b4dcdd5213d2209ba6f9273be`. Both matched their tested
GitHub Actions trees. The [package guide](docs/macos-development-package.md)
records downloaded CLI help, native Settings and assistant startup, and a
normal quit on macOS 27.2. The [chat report](parity/reports/native-text-chat-lifecycle-001.json)
records two isolated local answers under network denial, an independent manual
meeting, and native history and CLI reopening.

This increment joins Stop and the history save marker in one SQLite transaction.
The [signed native fault and retry](parity/reports/native-atomic-session-save-001.json)
preserved every stored field after a simulated failure, then saved and reopened
the same meeting and edited notes. The release desktop and core suites passed
206 tests with one explicit ignore. Active device finalization and forced-quit
recovery need separate runs.

[History conversation entry](https://github.com/thematheusmello/cueglint/pull/40)
landed at `41e5c8540163a72739a1e2926cf6459305ece8c4`. The
[packaged History report](parity/reports/native-history-conversation-entry-001.json)
records title, icon and timestamp activation, distinct saved conversations, a
local follow-up under network denial, and note persistence across restart.
The [badge regression](parity/reports/history-lifecycle-badge-001.json) reproduced
an inflated duration after an edit and passed with the actual lifecycle label.
Accurate recorded duration and questions spanning prior meetings remain open.

The [Windows host qualification](https://github.com/thematheusmello/cueglint/pull/39)
landed at `e2a5884537c7427382494ee63e921b35ea5e36f1`. Its first
[manual hosted run](https://github.com/thematheusmello/cueglint/actions/runs/44418418630)
compiled both binaries and exposed a help check that expected a name without
`.exe`. The [corrected Windows run](https://github.com/thematheusmello/cueglint/actions/runs/44421629393)
passed the complete host probe. The [report](parity/reports/windows-host-qualification-001.json)
records both outcomes, exact imports, and the remaining Windows notice and
packaging checks.

Next, complete scoped window acquisition and repeat the full first-use meeting
with original microphone and system audio, screen context and local inference.
Grounded notes and template evaluation, clean installs, notarization and updates
remain open.

Previous language implementation checkpoint: [PR28](https://github.com/thematheusmello/cueglint/pull/28)
at main `92a6bb888455f1585081ce36b336fb44d461e2f8`. The
[independent hosted audit](parity/reports/pr28-hosted-ci-artifact-001.json) binds
head `9ae9063`, CI merge `57c1fc5` and main to tree `8add53db`. Windows and
macOS each pass 386 tests and Ubuntu passes 384, with zero failures or filtering,
four/four/three ignores and separate PDF supervision. The native job passed
13 fixture frames and child startup. These are implementation checks.

The [local macOS arm64 package](parity/reports/macos-arm64-package-003.json)
passed strict ad-hoc signature verification and independent checks of both notice
trees (347 files each), all 696 checksum entries and the CLI help command.
[Live pointer testing](parity/reports/native-language-pointer-002.json) selected
German in the packaged settings window and verified persistence across a normal
quit and reopen. Capture was off and the model directory was empty during this
test. A later native check verified the language menu keyboard correction.

The [packaged multilingual file trial](parity/reports/packaged-multilingual-asr-001.json)
verified installation and both explicit CPU language routes under process network
denial. The authored English control passed with 0/18 word errors. Portuguese
failed at 12/19 against the frozen 0.10 word error rate limit. The trial keeps
that failure and both measured results.

The `fix/native-settings-interactions` branch includes packaged Knowledge and
keyboard corrections and the Portuguese read-speech results. The [native keyboard
report](parity/reports/native-language-keyboard-004.json) records arrow movement,
Return, Escape, focus restoration and a normal restart. The [previous failure](parity/reports/native-language-escape-red-001.json)
shows why the application-wide Escape dispatcher needed a regression. The app
suite passed 138 tests with one explicit ignore. The [Portuguese report](parity/reports/packaged-human-portuguese-asr-001.json)
keeps one passing clip and one above the fixed error threshold. Mode-scoped files,
the full local capture workflow, model quality and clean installations remain open.

The preceding merged checkpoint: [PR27](https://github.com/thematheusmello/cueglint/pull/27)
at main `e9808eeebcf1613c3094fffc9769481935c11e1a`. The
[independent audit](parity/reports/pr27-hosted-ci-artifact-002.json) binds corrected
head `5587785`, CI merge `708abda` and main to tree `9236423d`. All three Rust jobs
pass: Windows/macOS376 and Ubuntu374, zero failures/filtering,4/4/3 ignores,
32 summaries and separate PDF supervision each. No native job ran at558: path
filters excluded its metadata correction. Prior7d1's13 frames/child startup are
reused only under exact unchanged-source/configuration proof; no new artifact is
claimed. Earlier stale-progress failures and the wrong-worktree local checks stay
visible. The scoped notice source gate permits a controlled arm64 package attempt;
actual package/copy/sign/install acceptance remains open. Do not repeat PR27 merge.
The complete goal remains active. A subsequent read-only backlog audit confirms
at basee980 all20 earlier repository PRs merged, zero earlier open PRs and all20
non-main origin heads already
in main; earlier source drafts are merged or superseded. PR28 landed subsequently.
Preserve historical evidence and the private diagnostic.

The preceding merged checkpoint: [PR26](https://github.com/thematheusmello/cueglint/pull/26)
at main `672dd3693ff4941a93a19d3cce88bc9d09b2e065`. Its
[independent hosted audit](parity/reports/pr26-hosted-ci-artifact-001.json) binds
head `c99041c` and CI merge `eae5350` to tree `81e1c00d`; later Git checks confirm
merged main has the same tree, with local and remote main synchronized.
Windows/macOS/Ubuntu pass 376/376/374 tests with zero failures/filtering, 4/4/3
ignores, 32 summaries and separate PDF supervision each. All 93 common named
regressions and four additional Windows checks pass. All 13 native frames and
child startup pass; ten original images were independently inspected. Five
cancellations remain retained. The audit's open/draft snapshot predates merge.
The earlier staged 376-test source remains separate; physical device formats,
callbacks, ASR quality and reference parity remain unverified. Do not repeat
PR26's merge.

The preceding merged checkpoint: [PR25](https://github.com/thematheusmello/cueglint/pull/25)
at main `5f8add88bbe4b3a0973728f557600c3498360f29`. Its
[independent hosted audit](parity/reports/pr25-hosted-ci-artifact-001.json) binds
head `522507a` and CI merge `ed3e075` to tree `b14eaba9`; later Git checks confirm
merged main has the same tree. Windows/macOS each pass 366 tests and Ubuntu 364,
with zero failures/filtering and 4/4/3 ignores across 32 summaries each. Separate
PDF supervision passes per platform. All 13 native fixture frames and bounded
child startup pass; ten unique originals were viewed. Five cancellations and
the earlier composed model-fixture failure are retained. This is implementation
and fixture evidence, with no hardware, model-quality or reference-parity pass.
PR25 is already merged; do not repeat its merge.

The preceding merged checkpoint: [PR24](https://github.com/thematheusmello/cueglint/pull/24)
at main `7b01211cceb6ae65e1ddf10b15b90d4f55775958`. Its
[independent hosted audit](parity/reports/pr24-hosted-ci-artifact-001.json) binds
head `aa7cc95` and CI merge `d3254d7` to tree `4ec7993a`; later Git checks confirm
merged main has the same tree. Windows/macOS each pass 352 tests and Ubuntu 350,
with zero failures/filtering and 4/4/3 ignores across 32 summaries each. Separate
PDF supervision passes once per platform. All 13 native fixture frames and child
startup pass; ten unique originals were viewed. Five cancellations remain
retained. The audit's draft/open snapshot predates the merge. Notice readiness
remains false with three blockers; no actual package acceptance is established.

The preceding merged checkpoint is [PR23](https://github.com/thematheusmello/cueglint/pull/23)
at main `57d81abf65d060d3ea475c41c4bc5948a859b007`. The
[independent hosted audit](parity/reports/pr23-hosted-ci-artifact-001.json) binds
head `9744d97c` and CI merge `cfc56fe4` to tree `f2ec0f8b`; a later Git check
confirms merged main has the same tree. Windows/macOS each pass 339 tests and
Ubuntu 337, with zero failures/filtering and 4/4/3 explicit ignores across 32
summaries each. Each platform also passes one separately counted PDF supervision
self-check. Formatting, workspace, registry and progress steps succeed. All 13
original native fixture frames and child startup pass; ten unique originals were
viewed. All five superseded/redundant cancellations remain retained. The audit's
draft/open snapshot predates the merge; do not rerun or merge PR23 again.
These are hosted implementation and fixture-rendering checks. Actual native
import/quit, hardware, model quality, offline and reference acceptance remain open.

The preceding merged checkpoint is [PR22](https://github.com/thematheusmello/cueglint/pull/22)
at main `a38f047239494ce3a15066448a624af004f37cc6`. The
[independent hosted audit](parity/reports/pr22-hosted-ci-artifact-001.json) binds
head `6183c223` and CI merge `c355912b` to tree `ebffc2b7`; a later Git check
confirms merged main has the same tree. Rust passes Ubuntu 328/macOS 330/Windows
330 tests with zero failures/filtering and 3/4/4 explicit ignores across 31
summaries each. Formatting, workspace, registry and progress steps succeed. All
13 original native fixture frames and child startup pass; ten unique originals
were inspected. Four concurrency cancellations remain retained. The audit's
draft/open snapshot predates the merge; do not rerun or merge PR22 again. These
checks establish implementation tests and fixture rendering, with no new model,
native input/device, offline or reference acceptance.

PR22's tested runtime is `56d885c9f7639c04c3058e6f04f26399ab049e05`.
[Phase-budget evidence](parity/reports/model-download-phase-budgets-001.json)
records the retained old-client RED failures, 20 model and 17 provider checks, and
330 passing full release-workspace tests with four explicit ignores and no filters.
The fixed client preserves all production timeout values. Only its pinned HTTP
client/dependencies and regression tests change.

[Actual model-install recovery](parity/reports/model-install-recovery-001.json)
completes one resumed 7B install in 126.13 seconds; all three pinned files verify
independently. Settings/defaults, source and the frozen native executable stay
unchanged. The initial 30.46-second transfer failure and separate strict-canary
preflight failure remain recorded. The first optional 7B eight-case candidate
attempt, A001, stops in process-monitor preflight: all eight cases are `not_run`,
with zero CLI/model calls and no quality result. The v3 controller-only process
monitor correction passes source/synthetic review; any new attempt still requires
a separate release and all predeclared gates. Initialization/quality/memory remain unverified, and no default
model is promoted. Fixed/unseen scoring and the predeclared resource gates remain
separate; native stalled-download cancellation remains open.
A002 remains unreleased: the post-reopen 2026-11-18 12:05 UTC memory estimate is
12,285,526,016B (11.4GiB), below the unchanged 18GiB reserve. The user has been asked for one specific memory action and has not
replied. That readiness block is separate from the pending OS authorization
handoff; neither elapsed time nor hosted CI releases a model run.

The preceding merged checkpoint is [PR21](https://github.com/thematheusmello/cueglint/pull/21)
at main `18551b82305cbacf82b1f97968f92a1278e1371c`. Reviewed head `882b23e6`,
CI merge `3cbc4262` and main share tree `43774786`.
[Independent hosted audit](parity/reports/pr21-hosted-ci-artifact-001.json) records
Ubuntu 323/macOS 325/Windows 325 passing Rust tests, zero failures/filtering and
3/4/4 explicit ignores across 31 summaries per platform. All 13 original native
fixture frames and separate child startup pass; ten unique original images were
inspected. Five superseded or redundant concurrency cancellations remain retained.
The audit's draft/open snapshot predates the merge; do not rerun or merge PR21 again.
The source-bound [local integration review](parity/reports/knowledge-docx-integrated-workspace-001.json)
at `1236843` separately checks 452 source blobs and 325 passing workspace tests
with four ignores. Its six reviewed candidate commits preserve the preceding
font/readiness repair. Source/data/fixture and prior failed checks remain retained.

DOCX main-body extraction and unchanged UTF-8 now share one Rust core path for
CLI and desktop import, with one cancellable parser/result slot. Original fixtures
exercise actual parsing, retrieval, deletion, backup/data preservation, malformed
ZIP/XML and disabled/stale UI actions. [Source review](parity/reports/knowledge-docx-source-001.json)
resolves empty-paragraph loss, spurious tab-stop text and the Import enabled state.
The initial export-timestamp assertion failure remains recorded. Full PDF, wider OOXML,
interactive native import, hardware/platform runtime and matched reference
acceptance are still open. Hosted fixtures do not exercise native DOCX import.

The [strict PDF candidate](spec/features/native-knowledge-pdf.md) at `14142ba2`
passes [six named CLI document tests and a separate supervision self-check](parity/reports/knowledge-pdf-targeted-runtime-001.json).
This checks one DOCX and five PDF tests, including extraction, Unicode, retrieval
offsets, deletion, backups/notes, hostile inputs and actual allocation-budget exit.
The separate native executable checks pipe errors, cancellation, the unchanged
ten-second deadline and child reaping. It is not an extra Rust named-test count.
The 2 MiB input/text limits and child-only 256 MiB cumulative Rust allocation-work
budget are not a process RSS limit. Forms/images, scans/OCR, annotations, wider
fonts/CMaps/filters and full PDF/reference coverage remain unsupported or unverified.
The [earlier query-assertion and external-stream RED failures](parity/reports/knowledge-pdf-external-stream-red-001.json)
remain retained; the shared guard now rejects external-stream fields before decoding.
The [integrated workspace](parity/reports/knowledge-pdf-integrated-workspace-001.json)
at `778ada4a7b3e5e2442ae79a92cd850f401c90856` preserves PR22's HTTP repair,
defaults and existing source. It passes 339 tests, zero failures/filtering and
four explicit ignores across 32 summaries, plus the separate actual PDF
supervision self-check. Both headless document-import quit and failed-snapshot
start-order regressions pass. The test-only quit RED at `43c87f0` remains failed:
zero passed, one failed and 132 filtered, before its caller repair. Actual native
window/import/quit and Windows desktop/release acceptance remain unrun. Binary
packaging then lacked third-party notice bundling. The source collection and
consumer increment below preserve that checkpoint; actual release packaging
and notice-bundling acceptance remain open.

The [audio implementation checks](parity/reports/audio-resampling-runtime-001.json)
cover exact16k first/last samples, packet partitioning, bounded filtering, final
flush and continuity across internal utterance cuts. All 60 original synthetic
spectral rows meet the frozen0.25dB/40dB gates: worst passband deviation 0.002193dB,
weakest alias suppression 80.869811dB and weakest first-image suppression 78.282515dB.
[Independent targeted review](parity/reports/audio-resampling-independent-targeted-runtime-001.json)
checks all 543 source identities, unchanged original assertions and model/config
inputs. The baseline353-test run remains bound to source 1f65 and separate from
later PR24 composition. No devices, ASR quality, hardware drift/echo, live capture,
permissions, sustained performance or reference acceptance is established here.

The preceding distribution increment: the [notice consumer checks](parity/reports/runtime-notice-consumer-001.json)
and [independent review](parity/reports/runtime-notice-consumer-independent-001.json)
bind source `1dc46ed`. Formatting and all 26 xtask tests pass with no failures,
ignores or filtering. The retained actual RED has five failed regressions; the
first compile-only fixture error and later 25/1 public-README omission remain
recorded separately. Twelve configuration identities, 342 exact source payloads,
899 association references and both synthetic output trees are checked.

The actual headless `xtask package` readiness probe exits 1 with all three expected
blockers, invokes neither its Cargo nor codesign child, and creates no `dist`.
The [442-candidate source inventory](third-party/runtime/README.md) remains
conservative, with two missing original texts (`dispatch` and `hexf-parse`) and
mandatory coverage/choice/applicability review. `bundle_ready` stays false.
Source includes locked Cargo JSON artifact paths, post-build input validation,
identical app-resource/CLI notice copies before signing and exact post-sign hashes.
A successful build/signature, clean installation and Windows/Linux distribution
remain unrun. Failed future packaging may leave partial `dist` output; staged
release publication remains required. This increment changes no app/model default,
reference observation, feature state or acceptance tolerance.

The [notice provenance supplement](parity/reports/runtime-notice-provenance-supplement-001.json)
adds the two archive/Git-matched publisher Cargo declarations and an exact readable
Noto/Ubuntu font-copyright transcription. Existing MIT/CC0 instrument associations
are explicitly labelled as canonical terms, not newly found package originals.
All existing original notices, omission proof and historical reports are preserved.
The 35 expression choices, mandatory envelopes, conditional 442-candidate graph
and exact objc2 SDK caveat are recorded for review. The original supplement ran source-data checks only. Later Root checks pass
26 xtask tests, formatting, registry and progress at the separately bound staged
source above; no actual package/signing, native or model action runs.
The original false-gate report remains source-bound. The later source decision
adopts the declared terms, choices and repaired mandatory attribution. Actual
app/CLI package output and signing validation remain a separate next step. No consumer
exception, dependency/configuration change or automatic promotion is introduced.

The [ring source-header repair](parity/reports/runtime-notice-ring-source-headers-001.json)
retains one 7,860-byte file containing eight exact original ISC legal comments
from ten selected-family sources. The missing Google/Intel/Clarke/Sapin/Judd
attribution finding is preserved; all earlier notices and omission/history facts
remain. Its source-data review binds the locked published archive, not an
unfetched Git revision marked dirty. All original gap definitions and
omission facts remain, with the later explicit nonblocking source dispositions. The [publication whitespace failure](parity/reports/runtime-notice-ring-wrapper-red-001.json)
is retained; only one authored separator outside every original legal range was
removed. No runtime or actual package acceptance follows from these excerpts.

[PR20](https://github.com/thematheusmello/cueglint/pull/20) is already merged at main
`3c299d31295bdb66c9bc3363ee1ce41ac0164399`. Exact reviewed head abf1765, CI merge 73497ee
and main share tree 4b39931d. [Independent hosted audit](parity/reports/pr20-hosted-ci-artifact-001.json)
records Ubuntu 310/macOS 312/Windows 312 successful Rust tests with 3/4/4 ignores,
all 13 native frames and child startup. The provider panel is fully visible after
font adoption. Earlier head 9a4's native timeout, local RED/stale trace-count failures
and seven concurrency cancellations remain retained. The earlier timeout cause is
unproven. Do not rerun or merge PR20 again. This predates the new DOCX code.
The permission-handoff app remains the unchanged development-signed e4a46779 file.

The preceding independently audited merged checkpoint is
[PR19](https://github.com/thematheusmello/cueglint/pull/19), main
`2d859e3982b0ac926c2a323c4683dff505e2cbba`. Its exact tree passed Ubuntu302,
macOS304 and Windows304 Rust tests, with zero failures/filtering and3/4/4 ignores.
Thirteen original native GPU fixtures and bounded child startup also passed.
[Artifact review](parity/reports/pr19-hosted-ci-artifact-001.json) binds the head,
CI merge and main merge. Five earlier concurrency cancellations and previous
PR16/17/18 evidence remain retained. These checks predate the new selectors.
No CI test opened a real Windows audio device.

The preceding [capture-input workspace](parity/reports/capture-inputs-integrated-workspace-001.json)
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
sessions/history, editable notes, modes, bounded UTF-8/DOCX and strict text-PDF knowledge, versioned exports,
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

Computer control is reconnected. At the last observation, the frozen app was idle
with a saved, stopped session and audio sources off. Its display query reported access required
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
2. The [macOS packaging workflow](https://github.com/thematheusmello/cueglint/pull/30)
   is merged. Its [first dispatch](https://github.com/thematheusmello/cueglint/actions/runs/44384847084)
   reached an arm64 runner and failed during the Swift bridge build before upload.
   The merged repair selects Xcode 26.3 and checks the SDK before building.
   Reuse the [corrected run](https://github.com/thematheusmello/cueglint/actions/runs/44391784828).
   Once it succeeds, download the expected commit-named archive and verify the
   checksum, both notice trees, executable modes and strict signatures. The
   [failure record](parity/reports/macos-hosted-sdk-failure-001.json) preserves the
   original result. Clean installation and distribution remain open.
3. The [neutral-penalty six-arm diagnostic](parity/reports/selection-diagnostic-actual-v5-001.json)
   completes every control but matches zero exact targets. Forward unmasked outputs
   agree, reverse outputs differ; no sole model-capacity or backend-bug conclusion.
   V4's wrong-cache-field failure and five unrun arms remain failed, and quality003
   and model defaults are unchanged. The fixed-client 7B artifact recovery now
   passes, with prior transfer/probe failures retained. Preserve A001's preflight
   failure and eight unrun cases; retain the completed v3 controller-only monitor
   source/synthetic review before any separately released attempt.
   A002 stays unreleased while the fresh 18GiB reserve is unmet and the specific
   memory handoff is unanswered. Keep frozen model/tokenizer/source/prompt/configuration
   identities and all resource bounds. Installation and runner
   preparation imply no inference or quality result. Do not repeat unchanged
   1.5B cases or loosen scoring.
4. Finish the specific pending OS authentication handoff only after the user
   responds. Verify the same frozen app; prepare controlled original screen/audio
   fixtures before capture. Then run distinct real microphone/system audio,
   fresh selected screen context, local streamed answer, stop/finalize/save/reopen;
   then the complete offline workflow with traffic inspection. Resolve only a
   specific actual missing OS/user action after independent work is prepared.
5. The [first native Knowledge check](parity/reports/native-knowledge-flow-001.json)
   verified narrow DOCX/PDF imports, malformed rejection, exact persisted
   retrieval and confirmed deletion. The [follow-up](parity/reports/native-knowledge-help-001.json)
   verified corrected help and PDF import in a signed arm64 development build.
   Continue broader document support, mode-scoped reference files and the full
   knowledge/history scenario. Continue UI/window/IME/accessibility, notes/templates/history,
   settings/calendar/integrations, recovery, capture recipients and sustained use.
   Continue macOS/Windows clean install/signing/update, explicit Linux runtime,
   companion and optional open-source sharing/sync contracts through full closure.

Before publishing run affected tests, `cargo fmt --check`, registry and generated
progress checks. Reuse the shared target with two build jobs and a6GiB free-space
floor; ensure `xtask` was compiled for this worktree. No proprietary binaries/raw
captures, credentials, personal content or private machine paths belong in public CI.
