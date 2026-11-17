# Answer providers

Cueglint starts with local answers. In General settings, the Answer provider card
can select OpenAI, Anthropic, Gemini, Perplexity, OpenRouter or your own
OpenAI-compatible server. Use provider applies the provider, manual model ID and
optional custom base URL to future questions. Saving a selection or a key does not
contact a model. Model listing and Test connection require separate clicks; a test
sends only a tiny synthetic instruction.

Enter a key in the native password field and use Save key. The field clears after
successful storage, and stays masked and read-only while saving. A failed save
keeps the masked draft for retry. The key stays in the operating system credential
store, bound to that provider and endpoint. Saved keys are never loaded back into
the password field. Keys are absent from SQLite settings, sessions, logical
backups, exports and logs. Native key persistence currently supports macOS and
Windows. Other platforms can explicitly use a keyless compatible endpoint; the UI
reports unavailable key storage and never stores a plaintext substitute.

The assistant shows the selected model and destination. Sending a remote question
shares its frozen prompt and selected context with that provider: recent finalized
transcript segments, completed chat turns, matching knowledge snippets, mode
instructions and screen text when enabled. Audio capture and transcription remain
local. Provider retention and billing depend on the provider account. Review its
terms before sending private context.

OpenAI-compatible base URLs are explicit, support HTTPS or loopback HTTP, and must
not contain credentials, query strings or fragments. Use the API base path your
server exposes (commonly `/v1`). Cueglint implements OpenAI-style chat completions
and model listing for compatible servers. Ollama, LM Studio and self-hosted open
models are possible targets when they expose that protocol; interoperability with
actual Ollama/LM Studio deployments remains unverified. Model architecture support
belongs to that server. This
does not make arbitrary weights compatible with Cueglint's pinned Candle runtime.

Manual model IDs remain available when a provider has no public listing endpoint
or a listing fails. A listed model is a provider response, not a guarantee that an
account can run it. Connection errors, missing keys, cancellation and rate limits
remain visible; there is no automatic fallback to another provider. Selecting a
different provider/model cancels the old answer, and a retry creates a new frozen
request.

Protocol and integration tests use original synthetic context and loopback mock
servers. A real account/key run is opt-in and remains unverified until performed;
tests with mock replies cannot establish live service availability or model quality.

All six protocol routes have controlled HTTP integration coverage, including fragmented
streaming, terminal/error handling, credentials, cancellation and size limits. The original provider
workspace passed195 tests with three model/oracle checks ignored; the later renderer/audio
workspace passed253 with the same three ignores. A separate macOS run used
the production CLI and a temporary synthetic key for native save/status/load/delete across
processes, an authenticated model listing and a synthetic connection test. Its15 commands
passed and the test key's removal was verified. That establishes the bounded CLI/Keychain
path, not GUI credential prompts, Windows runtime, a vendor account or actual model quality.

| Provider | Implemented route | Model selection | Live account check |
|---|---|---|---|
| OpenAI | Chat Completions streaming | Manual ID or explicit catalog | Not run |
| Anthropic | Messages streaming | Manual ID or explicit catalog | Not run |
| Gemini | GenerateContent streaming | Manual ID or explicit catalog | Not run |
| Perplexity | Agent streaming | Manual ID; documentation link | Not run |
| OpenRouter | Chat Completions streaming | Manual ID or explicit catalog | Not run |
| Compatible server | OpenAI-style Chat Completions | Manual ID or explicit catalog | Actual local llama.cpp/Qwen example; other servers unverified |

The adapters follow the current documented APIs; they do not promise every provider model
or account entitlement. Perplexity uses its current Agent route rather than inventing a
public catalog endpoint. Missing models, account access, quota or protocol changes produce
visible errors. There is no automatic cloud fallback and no claim of universal availability.

Current response budgets are256 tokens in the native assistant,128 by default in CLI
`ask` (configurable1–1024 with `--max-tokens`), and16 for the synthetic connection test.
Provider token limits can include internal reasoning. A listed model or successful protocol
test therefore does not prove these budgets produce a useful visible answer for that model.
Reasoning-model budget controls and authenticated quality checks remain open; no universal
minimum or automatic budget increase is assumed.

The same selection is available in the CLI:

```sh
cueglint-cli provider show
cueglint-cli provider select openai --model YOUR_MODEL_ID
cueglint-cli provider key-status
cueglint-cli provider models
cueglint-cli provider test
cueglint-cli provider select compatible --model YOUR_MODEL_ID --base-url http://localhost:11434/v1
cueglint-cli provider select local
```

`provider key-save` reads only piped stdin, so a password manager can supply a key
without placing it in command arguments, shell history or echoed terminal input.
`provider key-delete` removes the key for the selected provider and endpoint.
Both leave the saved provider choice unchanged. `ask SESSION QUESTION --stream`
uses the saved provider; its `--profile` override applies only to local answers.
The CLI does not claim remote availability without an explicit connection test.

## Real local compatible-server check

The frozen packaged CLI at sourcea9ca43a passed a separate original-fixture run with
official llama.cppb12359 and pinned Qwen2.5-1.5B-Instruct Q4_K_M weights. It used
a deliberately selected keyless loopback endpoint, a real catalog and two streamed
answers. Both answers were persisted with exact request/context/model provenance;
original human notes/transcript were unchanged. Independent blind review accepted
support, relevance and completeness for both. This is one unique question repeated
twice, not a broad model-quality or all-server guarantee.

Startup logs show0/29 offloaded layers and CPU model/KV/compute buffers. Both
prompts evaluated all305 tokens with cached n_tokens0. Prompt-prefix caching was
disabled, while context checkpoint capability and graph/model/OS caches remained.
The observed provider completion times were1273/1227ms; these n1+n1 functional
observations are not first-token or reference latency benchmarks. Unrelated host
Cargo work was observed afterward; overlap is unknown.

This optional server is external C/C++ development/user software. It is not a
Cueglint runtime dependency, Rust inference engine or automatic fallback. Normal
Cueglint local operation still uses Rust/Candle. No vendor account/key, cloud
request, default database or saved user credential was used. Four native-read
commands used the exact original test scope; only one explicit key-status Missing
result was observed, and OS API call counts were not instrumented. No credential
write ran. All22 owned child processes were reaped.

[Actual evaluation](../parity/reports/optional-llama-interop-004-actual-evaluation.json)
and [independent receipt](../parity/reports/independent-optional-llama-interop-004-actual-review.json)
retain identities, outputs and limits. The preceding003 run failed its CPU startup
evidence gate before either question; [that failure](../parity/reports/optional-llama-interop-003-actual-failure.json)
remains unchanged. The next run added documented bounded trace verbosity and exact
tagged log-emitter parsing, preserving the original prompt/model and CPU/quality
criteria. No generated answer was retried.
