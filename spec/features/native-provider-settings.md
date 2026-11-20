# Native answer provider settings

This is an original Cueglint behavior contract for user-requested bring-your-own-key
providers. It is not a reference observation or a claim of reference parity.

General settings keeps the existing warm dark cards, scrolling content and sidebar.
An Answer provider card offers Local, OpenAI, Anthropic, Gemini, Perplexity,
OpenRouter and an OpenAI-compatible endpoint. Local remains the default. A manual
model identifier works independently of optional, explicitly requested model listing.
OpenAI-compatible endpoints target local services such as Ollama and LM Studio or
the user's own deployment when they expose that protocol. The packaged CLI exercised one local Qwen model through Ollama 0.40.16.
LM Studio and native Ollama UI interaction need separate verification.

Changing the draft, saving a provider or storing a credential sends no inference
request. Use provider applies the selected provider/model/endpoint to subsequent
questions. Refresh models and Test connection are separate intentional network
actions. Connection tests use only an original tiny synthetic instruction, never a
meeting, transcript, notes, screen context or knowledge document.

The hosted answer token limit accepts 1–1024 inclusive. Each submitted question
uses the saved limit in effect when the worker dispatches it. Blank restores
the original desktop and CLI defaults of 256 and 128. A CLI override lasts
for one question. Editing the limit leaves the endpoint credential scope
intact. A local restore retains the preference and selects Local. The
synthetic connection check always uses 16 output tokens.

The API key field is a native password input. Save key submits its contents in a
bounded worker command. The password stays read-only until storage acknowledges
success, then the field and its edit history clear; a failed save retains the
masked draft for retry. A changed provider/endpoint clears it immediately. Credentials are
bound to the selected provider and endpoint, and live only in the OS credential
store. App state, settings, sessions, backups, exports, diagnostics and UI snapshots
contain status alone. A failed or unsupported credential store is visible; there is
no plaintext fallback. Deleting a key leaves provider selection unchanged.

The assistant identifies its selected provider/model and context destination.
Audio transcription stays local. A deliberate question to a remote provider sends
its frozen prompt, selected transcript/chat/knowledge context and enabled screen
text to that destination. Merely enabling screen text performs local acquisition;
it does not send an inference request. Keys for official providers are never sent
to a custom endpoint.

Provider/model changes cancel prior answers and network model-list/probe jobs.
Reference-pinned credential operations finish independently; their acknowledgements
require the exact operation UUID and credential reference. Requests freeze their provider identity;
later configuration changes cannot reroute an accepted request. Existing bounded
worker, session/request/context identities, stream retirement and explicit Stop
response apply to local and hosted work. Failures remain failures, with an explicit
retry available by sending again. There is no automatic provider or local fallback.

Missing local answer files do not block a deliberately selected remote provider.
The pinned local import, download, verification and transcription paths remain
available. Hosted metadata records reported model/usage/version values; unavailable
values remain unknown. Mock integration evidence and optional live credentials are
separate: the absence of a real-key run must stay visible in delivery reporting.
