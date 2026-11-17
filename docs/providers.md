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
