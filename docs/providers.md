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
and model listing for compatible servers. The dev.3 macOS CLI ran an imported
Qwen GGUF through Ollama 0.40.16, listed the model, and streamed a grounded
answer. When the server stopped, Cueglint surfaced an error and cancelled the
request. LM Studio still needs a live check. Model format support depends on
the selected server.
[Ollama verification](../parity/reports/packaged-ollama-interop-001.json) ·
[Reproduce with a local model](ollama-verification.md)

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

Under **Hosted answer token limit**, enter a whole number from 1 to 1024 and
choose **Use provider**. The saved limit applies to future hosted answers on
desktop and CLI. A blank field restores the desktop default of 256 tokens
and the CLI default of 128. Each dispatched answer keeps its original limit. An explicit limit raises
the database and backup reader requirement to version 6 so an older app cannot
silently discard the cap. Clearing it permits lower-version backups if the
remaining data supports them. [Data format](data-format.md)

A larger allowance can increase provider charges. Some models spend output
tokens on internal reasoning, so even a completed request can return little
visible text. The synthetic connection probe always requests 16 tokens.
Local generation uses its own budget.

```sh
cueglint-cli settings set-hosted-answer-limit 384
cueglint-cli ask SESSION_ID "Summarize the discussion" --max-tokens 96
cueglint-cli settings reset-hosted-answer-limit
```

The explicit `--max-tokens` applies to that single CLI question. Model access,
minimum accepted request sizes and answer quality depend on the provider and
model. Authenticated quality checks remain open.

A local llama.cpp b12359 server loaded the pinned Qwen2.5 1.5B GGUF
and generated exactly 1, 12 and 4 tokens for two saved limits and a CLI
override. Fresh CLI processes reopened all three answers with the requested
limit and provider-reported `length` finish reason.
[Real compatible-model control](../parity/reports/hosted-limit-real-compatible-model-001.json)

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

The [native macOS UI check](../parity/reports/native-provider-ui-004.json) uses
the actual ad-hoc signed executable at `136a7c7`, a fresh isolated database and a
synthetic loopback compatible server. Pointer focus and keyboard entry fill the
endpoint, model and masked key. Saving clears the key field, confirms OS storage
and leaves Local active. Model listing and the explicit synthetic connection test
succeed. Only **Use provider** changes the active destination; the floating
assistant then displays one synthetic `OK` answer. The session is ended and saved,
the key is deleted and rechecked missing, and Local is explicitly restored before
normal quit. All five checked default application-data endpoints are unchanged.

This is one compatible-server route on one Mac, with no real inference or vendor
account. The [preceding fixture attempt](../parity/reports/native-provider-ui-003-failure.json)
was correctly rejected for missing terminal stream metadata and remains failed.
The earlier [native menu failure](../parity/reports/native-provider-ui-002-failure.json)
also remains recorded. Semantic menu activation now closes the provider menu;
AX activation followed by keyboard typing remains an unresolved accessibility and
automation check.
Native vendor flows, Windows vault behavior, other compatible servers and model
quality require their own evidence.
