# Cueglint continuation record

The full [delivery goal](docs/delivery-goal.md) is **active**: complete all eight
workstreams, all 73 feature contracts and all twelve acceptance families. Desktop
acceptance is the first release milestone; required companions remain separate open
work. The [registry](parity/features.yaml), [roadmap](ROADMAP.md) and generated
[progress](docs/progress.md) retain independent observation/implementation/verification.
No desktop or whole-product parity is claimed.

Repository: https://github.com/thematheusmello/cueglint.
Published increment: [draft PR18](https://github.com/thematheusmello/cueglint/pull/18),
branch `feat/local-session-reliability`. Hosted checks are pending; inspect the
[latest PR checks](https://github.com/thematheusmello/cueglint/pull/18/checks) at its
exact head before merging. No new-head CI pass is claimed here.
Last merged main: `76a2d81926f25b2aec2d53572f79659dc62cc016`,
[PR17](https://github.com/thematheusmello/cueglint/pull/17), following
[PR16](https://github.com/thematheusmello/cueglint/pull/16). Both exact tested trees
passed all three hosted Rust jobs, thirteen native GPU fixture captures and child
startup. Independent artifact review is retained in the
[PR16](parity/reports/pr16-hosted-ci-artifact-001.json) and
[PR17](parity/reports/pr17-hosted-ci-artifact-001.json) reports. The initial PR16
zero-step runner-capacity cancellation remains recorded. Later code needs its own CI.

Current code checkpoint: `5845d00ffeff44bee05949f0c6f74218a4705d1b`, branch
`feat/local-session-reliability`. Its full locked/offline workspace suite passed
**299 tests, zero failed, four explicit ignores, zero filtered**. All 399 source
file hashes were unchanged across the run; formatting passed. See the
[workspace report](parity/reports/local-reliability-workspace-001.json).
This is implementation evidence, with no configured-model or native-device claim.

The increment includes managed generation/ASR loading from the same verified file
handles, bound transcription profile provenance, native Windows microphone and
render-loopback workers sharing a validated WASAPI bridge, and notes-only bounded
JSON token masking before ArgMax. Windows driver/permission/route behavior and
managed CPU/Metal loading still need real-platform checks. Cooperative atomic file
replacement is covered; uncooperative in-place mutation is outside the loader contract.
The legacy ASR entry points remain available. New notes provenance has an optional
decoder-policy revision; old records restore unchanged, while older strict readers
may reject records containing the new field.

The frozen release CLI for this checkpoint is SHA256
`d1b1c6e75aeb52d63fec27fbb254e77da81731033c7c08b4ff38c86eb7c84bb1`.
Its release build and `--help` passed; it is retained privately without replacing
the stable development package. It is not a packaged clean-install acceptance.

Actual [highlights execution002](parity/reports/highlights-quality-actual-002.json)
after the decoder/input-binding fix also **failed**, with valid source-bound output:
both cases selected the first eight utterances, scoring precision0.50/recall0.667
against the unchanged0.80 thresholds. Category/critical coverage also failed. Each
case generated once, with actual end-of-turn and the new decoder revision. Ready
drafts remained unaccepted; manual notes were preserved. This run fixes formatting,
not salience, and cannot establish general quality. The next isolated candidate adds
generic importance criteria and a new prompt revision; its model quality is unrun.
That isolated local candidate is `a7501e7331364555dc7f59ea397b5ec7a79e547e`;
independent source review found no material defect. It is not integrated or Cargo
tested. Review the exact retained candidate, then test it before a new source-bound
model evaluation; its longer prompt may change chunk boundaries.

Actual [highlights execution001](parity/reports/highlights-quality-actual-001.json)
at `d28b390` remains **failed**: both original cases exceeded eight selections.
Salience was not scored; acceptance/append did not run. Each case had one generation,
no retry/repair/fallback, no candidate/generated notes, and preserved user data.
The [independent audit](parity/reports/independent-highlights-quality-actual-001.json)
retains the original failure. A source repair cannot retroactively pass that result.

The selected reference is still macOS Cluely2.1.28. Fresh installed2.1.29
[metadata-only observation](parity/reports/reference-2.1.29-metadata-001.json)
records an860x670 Settings window on the current1x main display. Its screenshot is
fully transparent and AX has no candidates. Current typography and historical2.1.28
scale remain unknown. No old app was launched against the updated user profile.
Production fonts still use the legacy platform-file loader; the OS resolver is
test-only. Original Cueglint branding/translucency and own native screenshots are
implemented; exact reference fonts, blur, motion and matched visual acceptance remain open.

Working application paths include native compact/expanded windows and Settings,
sessions/history, editable notes, modes, bounded UTF-8 knowledge, versioned exports,
transactional backup/restore, CLI access and cancellable local generation. Optional
explicit BYOK routes cover OpenAI, Anthropic, Gemini, Perplexity, OpenRouter and
OpenAI-compatible endpoints. Controlled routing and scoped macOS CLI credential
checks passed; live vendor/model coverage, native credential UI and Windows vault
runtime remain open. Local/open models are the default; there is no automatic cloud
fallback. Keys belong in OS credential storage, never in chat or exported data.

Historical model evidence remains source-specific: the CPU ASR suite failed its
complete speed gates; a separate M3Max Metal worker passed fixed warm gates with
unchanged transcripts and a retained10.58-second first base warmup. Optional
llama.cpp/Qwen interop produced two persisted answers to one original question.
These results do not establish broad quality, current packaged inference or real
dual-source capture. Earlier failures, including screenshots, source assertions,
database-lock recovery, inference preflights and screen-target selection, remain in
[verification](docs/verification.md) and `parity/reports/`.

The stable `dist` development package remains sourcea9ca43a: GUI SHA256
`0ff61d45dcffbb0d386acce169106fde072e26bdb6d630b6cef2b717718a8154`,
CLI `dbc1d13c74a1406c239c13090d39f84d8b25559f932da06f11dd0750969e9cff`.
Its ad-hoc signature/plist/icon checks passed. Notarization, clean install, current
screen permission, full offline/live capture and release distribution remain open.
The user already enabled Cueglint's screen permission. A prior bundle still returned
Denied; freeze the intended replacement before diagnosing a specific OS refresh.
The default user database remains untouched by controlled tests.

Computer access remains unavailable after the user's reconnection reply and fresh
native-pipe/browser-host checks. The question is answered; no repeat generic access
or Screen Recording request is pending. Supported passive REA observation remains
available within its declared limits. The isolated provider GUI preview has not launched.

The owner-authorized path-only WAV history cleanup is complete. See
[history cleanup](docs/history-cleanup.md). Do not repeat the rewrite. Closed PR
objects/caches may retain old media; original evidence and mapping remain. Newly
authored code is MIT OR Apache-2.0; separate fixture/dependency/model terms remain.

Next exact actions:

1. Inspect this record, the actual Git head/status, goal state and private continuation.
   Reuse retained models/evidence and owned job handles; never restart a live job.
2. Verify the new public PR's exact head and hosted Windows/macOS/Linux checks. Preserve
   failures and artifacts. Compilation does not establish microphone/system capture.
3. Complete current local highlights quality and explicit review/accept/append gates.
   Keep frozen thresholds, prior failures and original cases. Add unseen evaluation
   before claiming general salience; summaries/decisions/action items remain open.
4. Freeze the intended native package; validate real distinct audio, selected fresh
   screen context, local streamed answer, stop/finalize/save/reopen, then the complete
   offline workflow with traffic observation. Resolve only actual missing OS/user access.
5. Complete remaining UI/window/IME/accessibility, knowledge/templates/history/settings,
   integrations, recovery, recipient capture and sustained resource checks. Continue
   signing/install/update, Windows/Linux hardware and companion/self-hosted tracks.

Before publication run affected tests, formatting, registry and generated-progress
checks. Reuse the shared Cargo target with two build jobs and a6GiB free-space floor;
ensure `xtask` was compiled for this worktree before generating reports. Raw commercial
artifacts, original private fixtures, credentials, personal captures and machine paths
stay outside the public repository. Exact private artifact locations and current owned
job/agent state are in `.private/continuation.json`.
