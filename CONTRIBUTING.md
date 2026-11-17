# Contributing

Read [AGENTS.md](AGENTS.md), [research method](docs/research-method.md) and the feature
registry before changing a behavior. Keep patches small and complete, with meaningful
checks for nontrivial logic. Describe user-visible behavior, evidence and remaining gaps.

Never commit commercial binaries/source, credentials, model weights, personal meetings or
private reference screenshots. Reference investigators produce reviewed behavioral specs;
implementation workers receive fresh contexts containing those specs, not recovered code.
Preserve licenses and attribution for reused open-source components and models.

Run `cargo fmt --check`, `cargo test --workspace`, `cargo xtask verify-registry` and
`cargo xtask progress --check`. Real capture/model and matched-reference tests run in
controlled environments, without releasing account credentials to pull requests.

New contributions are licensed MIT OR Apache-2.0. Changes are reviewed through pull requests;
CI failures and unavailable platform checks remain visible. Do not rebaseline visual tests
or relax quality thresholds merely to make a failing result pass.
