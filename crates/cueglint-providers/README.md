# Explicit provider requests

Cueglint defaults to the existing local Candle generator. Saving a key does not select a provider, list models, test a connection or send a session. Selecting a provider/model is separate; sending an answer request, refreshing models and testing a connection are deliberate actions. Missing local files and provider failures never trigger another provider.

This crate owns the network and native credential boundary; `cueglint-core` remains offline. The synchronous facade runs async requests on one current-thread Tokio runtime per client owned by the caller's bounded actor. It retains the same pending request while polling cancellation every 50 ms, with a 120-second total deadline and a 20-second connection deadline. Cancellation drops the pending connection/stream. It cannot guarantee that a hosted provider stops processing or billing after disconnect.

No redirects, HTTP retry policy, system proxy inheritance, cookies or execution tools are enabled. Built-in provider bases are fixed official HTTPS URLs. A compatible server requires an explicit HTTPS base, or HTTP on loopback. The base URL must have no userinfo, query or fragment. Keys are scoped to the provider plus the entire canonical API base, including its path; changing that scope never borrows the previous key. Compatible servers control their own execution and privacy, and a loopback server can itself forward to a hosted model.

`Secret` is transient, zeroized on drop, redacted in Debug and has no serialization implementation. macOS Keychain and Windows Credential Manager are accessed through pinned `keyring` native features, using only the Cueglint service and exact scoped entry. Other platforms return an explicit unavailable status, with no mock or plaintext fallback. An explicitly selected keyless compatible server still works there. A failed credential read on a supported platform never becomes a keyless request. Settings, requests, backups, exports and runtime snapshots contain configuration/status/provenance only.

Response bodies and transport errors are not included in public errors. Authorization values are marked sensitive. Incoming JSON that echoes the active key is rejected; accumulated text is checked across streaming deltas before emission. Hosted metadata records requested model, reported model/version when present, actual configured base, vendor response ID, optional vendor token counts, prompt hash, template revision, finish reason and citations. Unknown model weight revisions and artifact hashes are never invented. Local inference metadata and local Qwen prompt bytes are unchanged.

The original loopback protocol tests are mocks, using the real HTTP clients and SSE decoder. They do not establish live API access, vendor/model quality or reference parity. No keys were supplied and no live vendor success is claimed. Future live checks must be an explicit opt-in using synthetic prompts and dedicated user-supplied credentials, never private meetings or untrusted PR secrets.

## API contracts reviewed 2026-11-17

| Provider | Contract used | Catalog |
| --- | --- | --- |
| OpenAI | `POST /v1/chat/completions`, bearer key, `max_completion_tokens`, usage chunk and `[DONE]`; `store:false` | `GET /v1/models`; listing does not prove every ID supports text chat |
| Anthropic | `POST /v1/messages`, bearer key, `anthropic-version:2026-10-21`, top-level system, `max_tokens`; `message_stop` | `GET /v1/models`, bounded cursor pagination |
| Gemini | Documented GenerateContent API: `POST /v1beta/models/{id}:streamGenerateContent?alt=sse`, `x-goog-api-key`, `systemInstruction`, `generationConfig.maxOutputTokens`; terminal finish reason required | `GET /v1beta/models`, bounded pages, filter declared `generateContent` support |
| Perplexity | `POST /v1/agent`, bearer key, typed input messages, `max_output_tokens`, one step, no tools, `store:false`; `response.completed` | Manual IDs linked to the public Agent model documentation; no invented list endpoint or private-preview gateway |
| OpenRouter | `POST /api/v1/chat/completions`, bearer key, `max_tokens`, fallback routing disabled; repeated finish on usage frame accepted before `[DONE]` | `GET /api/v1/models`, bounded response |
| Compatible | Explicit base + `chat/completions`, optional scoped bearer key, `max_tokens`; `[DONE]` required | Base + `models`, or manual model ID |

Google currently labels GenerateContent as Legacy. Newer model families may require another API; an unsupported model is an explicit error, never a hidden protocol/provider switch. Perplexity's former Sonar route ended support on 2026-09-27, so this implementation uses the documented Agent contract. Anthropic sampling parameters removed by the 2026 API version are omitted. `store:false` controls only documented API storage behavior; provider retention/privacy policies still apply. Gemini receives no transposed top-level `store` option.

Sources: [OpenAI chat](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create), [OpenAI models](https://developers.openai.com/api/reference/resources/models/methods/list), [Anthropic API migration](https://platform.claude.com/docs/en/api/migration-2026-10-21), [Anthropic streaming](https://platform.claude.com/docs/en/build-with-claude/streaming), [Anthropic models](https://platform.claude.com/docs/en/api/models/list), [Gemini GenerateContent](https://aistudio.google.com/api-reference/generate-content), [Gemini SSE example](https://ai.google.dev/gemini-api/docs/generate-content/text-generation?hl=en), [Gemini models](https://aistudio.google.com/api-reference/models), [Perplexity Agent schema](https://docs.perplexity.ai/api-reference/agent-post.md), [Perplexity migration](https://docs.perplexity.ai/docs/agent-api/migrate-from-sonar/overview), [Perplexity models](https://docs.perplexity.ai/docs/agent-api/models), [OpenRouter streaming](https://openrouter.ai/docs/api_reference/streaming), [OpenRouter routing](https://openrouter.ai/docs/guides/routing/provider-selection), [OpenRouter models](https://openrouter.ai/docs/api/api-reference/models/list-all-models-and-their-properties), [Ollama compatibility](https://docs.ollama.com/api/openai-compatibility), [LM Studio compatibility](https://lmstudio.ai/docs/developer/openai-compat), [keyring 3.6.3](https://docs.rs/keyring/3.6.3/keyring/), [reqwest 0.12.28](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html).

Limits are 256 KiB serialized request/event, 2 MiB streamed body/text, 16,384 SSE events, 64 citations, 512 models, four catalog pages and 1–1,024 output tokens. Manual model IDs remain available when a bounded catalog is truncated. System DNS cannot be interrupted in libc; one process-wide slot bounds residual DNS work after cancellation, and an unavailable slot fails explicitly rather than queuing more DNS threads.

## Windows credential fixture

The Rust CI Windows job exercises the real credential store with a unique
synthetic loopback scope. The fixture checks that the scope starts empty,
stores and reads the exact synthetic value, then removes it and checks
that it is empty again. The ordinary test suite leaves it ignored.

To opt in on a disposable Windows account with the locked dependencies cached:

```powershell
$env:CUEGLINT_TEST_WINDOWS_VAULT = '1'
cargo test --offline --locked -p cueglint-providers --lib credentials::tests::windows_native_credential_round_trip -- --ignored --exact --nocapture
```

The test uses the local credential vault and requires no provider account or
network request. Its scope is printed for recovery if the test process is
terminated during a write.

## Google Calendar transport

The calendar module uses a Google Desktop OAuth client with the read-only
scope for events on owned calendars. The provider offers only the primary
calendar. The production requests use fixed Google HTTPS endpoints. The
authorization callback binds IPv4 loopback on an available port, validates
OAuth state and uses S256 PKCE. Connect requests offline access so a worker
can refresh without reopening the browser. [Google Desktop OAuth](https://developers.google.com/identity/protocols/oauth2/native-app) · [Google Calendar Events.list](https://developers.google.com/workspace/calendar/api/v3/reference/events/list)

A desktop integration can construct one GoogleCalendar on a bounded worker
with a Tokio runtime and an app-configured Google Desktop OAuth client ID.
Provide a desktop client secret through Secret if the OAuth client requires
one. Use a dedicated Calendar OAuth project because revoking a grant can
revoke other clients and scopes in the same Google project.

- Connect calls begin_connect, opens its authorization URL in the system
  browser, and awaits finish_connect. Cancel by dropping the pending listener
  or the in-flight future. Save the returned refresh token in the native vault
  before adopting the connection. Keep the access token in memory.
- Refresh loads the stored refresh token on demand, or uses the current
  session. Save the refresh token returned by the refreshed session before
  replacing the old one. When Events.list requests refresh, try one token
  refresh and one fetch. A rejected offline grant requires explicit Connect.
  Keep the last successful event page visible as stale after a fetch failure.
- Disconnect first cancels pending work. Request Google revocation with revoke
  and remove the local entry with forget_saved_refresh_token. Report each
  result separately and discard the in-memory session and events after local
  removal. If revocation fails, allow a retry with the existing credential or
  an explicit local-only disconnect. Scope each worker result to the active
  connection so an old result cannot restore disconnected events.

A fetch returns one page of up to 50 events overlapping the next 30 days. The
page reports whether Google has more results. Timed values retain the UTC
instant and any known numeric offset from the response for each endpoint. The
app uses that offset for display, including across daylight-saving transitions.
All-day dates keep their exclusive end. Offsetless timed values surface as an
invalid response. Reminder scheduling needs separate implementation.
[Google Calendar event dates and time zones](https://developers.google.com/workspace/calendar/api/concepts/events-calendars)

The desktop worker accepts explicit Inspect, Connect, Refresh, Reconnect and
Disconnect actions. Its Calendar queue is independent of inference. Startup
and backup restore do not read the vault or contact Google. Inspect reads the
local connection only. Connect takes a Google Desktop client ID and uses the
documented optional-client-secret flow. Cueglint stores the chosen ID in the
native Calendar vault under a separate service from its offline token. The
client ID, access and refresh tokens, authorization URL and event pages are
excluded from logical backups. One Google Calendar profile is scoped to the
device's OS account. The optional integration is available where Cueglint has
a supported native vault and browser launcher.

The worker adopts only the newest active action's completion. Cancel before
the new grant starts saving closes the authorization listener. Disconnect
clears the visible events immediately and retires any earlier job before
changing credentials. A first retryable failure can report remote revocation
failure while keeping the stored grant so the user can retry or remove it from
this device. An expired offline grant prompts Reconnect. Refresh uses the
last successfully fetched event page as stale data after a transient failure.

The synthetic protocol tests exercise the actual callback and HTTP requests
over loopback. Scripted desktop worker tests exercise consent retirement,
refresh rotation, expired grants and disconnect outcomes with an in-memory
vault and no browser launch. A live Google account and native vault/browser
interaction require separate platform checks.
