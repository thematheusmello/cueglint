# Cueglint implementation rules

Preserve the clean implementation boundary: raw Cluely artifacts, recovered source,
credentials and personal captures stay outside this repository. Implement from reviewed
behavioral contracts in `spec/features/` and measured tokens in `ui/`.

Use one native Rust UI stack (eframe/egui on wgpu). The default app cannot require Node,
Python, Electron, a webview or a hosted inference service. Never silently enable cloud
inference. The core stays usable from the CLI without the GUI.

`parity/features.yaml` is the feature inventory. Observation, implementation and
verification states are independent. Implementation-only tests do not establish reference
parity. Preserve missing, blocked, failed and stale checks. Do not weaken tests to improve
the progress report. Record model, fixture, source and configuration identities for claims.

Use bounded background jobs, stable session/request/context identities and explicit
cancellation. Never perform model inference or capture I/O in UI drawing or audio callbacks.
Validate inputs and preserve user notes and existing data through migrations and errors.

Before publishing run the appropriate tests, `cargo fmt --check`, and
`cargo xtask progress --check`. Keep `STATUS.md` current with exact continuation steps.

Keep each commit atomic: one complete logical change with its required tests,
fixtures and documentation. After the required checks pass, push the tracked
work branch immediately after every commit. Do not accumulate unpublished
commits; resolve a failed push before creating another commit.

For multiple worktrees, use a separate `CARGO_TARGET_DIR` for each worktree's
`cargo xtask` commands. The executable embeds its workspace path; a shared
top-level cached binary can validate another checkout. Verify the embedded tool
root matches the intended checkout and generate/check that checkout's reports.

Track active branches and open PRs in STATUS.md with their next landing action.
After exact-head checks and review pass, land the increment into main and verify
local and remote main agree. Reconcile superseded drafts against landed work
before replaying any older source.
