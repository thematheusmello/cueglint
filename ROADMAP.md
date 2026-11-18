# Cueglint roadmap

The [active, resumable delivery goal](docs/delivery-goal.md) covers the complete
implementation and verification scope. `parity/features.yaml` remains authoritative:
73 required features, comprising 69 desktop and 4 companion contracts. Observation,
implementation and verification remain independent. An early gate, static mockup,
compilation, merged PR or first model answer is not completion.

The eight workstreams below crosscut the existing six delivery gates and their
issues/milestones; they do not renumber tracking. The goal document maps them to
the original gates, and each feature retains its existing gate and issue link.

1. **Reference, UI and windows:** preserve the chosen 2.1.28 baseline; review reachable
   states, measured typography/geometry/scale, native input, focus, shortcuts, displays,
   child lifecycle and motion. Fresh 2.1.29 metadata does not replace the baseline.
2. **Complete packaged local session:** distinct microphone/system audio; bounded local
   ASR, fresh screen understanding, frozen context and cancellable streaming; actual
   package stop/finalize/save/reopen and offline operation with network observation.
3. **Full workflows and integrations:** notes/editing/generated outputs/search/history
   chat; modes/templates/knowledge; working settings, calendar and observed integrations;
   persistence, errors and complete keyboard interactions.
4. **Models and intentional providers:** robust model lifecycle/backend selection;
   fixed/unseen/repeated local quality and latency evaluation; explicit BYOK reliability
   without silent cloud fallback. Preserve failed generation and quality checks.
5. **Privacy, recovery and sustained use:** recipient-side capture compatibility,
   credentials/logs, network behavior, migrations, deletion, export/backup/restore,
   cancellation and sustained resource/latency measurements with preset bounds.
6. **Distribution and public CI:** clean-install macOS/Windows packages, explicit Linux
   matrix, updates/rollback and data portability; exact-source public checks and retained
   artifacts without private commercial fixtures or account/release secrets.
7. **Companion and self-hosted coverage:** required browser/mobile companions,
   open-source sharing and self-hosted synchronization with independent client/service,
   conflict, deletion and portability evidence.
8. **Contract and acceptance closure:** resolve all 73 contracts, transitive dependencies
   and twelve acceptance families. Rerun stale evidence after implementation, reference,
   model, prompt, configuration or fixture changes. Keep missing, blocked, failed and
   stale checks visible until resolved; missing companions prevent whole-product closure.

Cueglint uses its own visual design, typography, assets and brand. Reference
observations define workflows and interactions to evaluate. The core runs free
and locally, with optional providers and services selected by the user.

The [local macOS arm64 package](parity/reports/macos-arm64-package-003.json) remains
recorded. A [hosted development archive](docs/macos-development-package.md) is now
available from [run 44400700716](https://github.com/thematheusmello/cueglint/actions/runs/44400700716).
Its [report](parity/reports/hosted-macos-arm64-development-001.json) binds exact
source `b78c9ac1`, signatures, notice trees and checksums, plus a first launch and
normal quit in a fresh app profile on macOS 27.2. Earlier failed attempts remain
recorded. Clean OS installation, Gatekeeper/notarization, updates/rollback, the
actual minimum supported macOS and the full macOS/Windows/Linux matrix remain
distribution work.

The audio increment adds synthetic-verified bandlimited resampling and exact16k
sample preservation. Actual devices, drift/echo, speech quality and long-meeting
calibration remain part of the complete live-session gate.
