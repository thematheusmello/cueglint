# Explicit local transcription language

This is an original Cueglint implementation contract, not a reference observation or
parity claim. `audio.language` remains unobserved and unverified; its implementation
is partial after the frozen-source checks: this increment implements explicit input-language selection only. Response
language, speech translation and UI localization remain separate, unimplemented choices.

Saved settings contain one canonical `transcription_language` code, defaulting to `en`.
The core, native Local models row and CLI share the same 99-code list from the pinned
Whisper base language-token metadata. These are accepted model tokens, not 99 languages
with tested quality. Aliases, regional codes, whitespace, unknown codes and `auto` are
rejected. There is no language detection, implicit translation or cloud fallback.

`whisper-base-en` and `whisper-tiny-en` retain their exact files, defaults and legacy
`SOT + NO_TIMESTAMPS` decoding. Non-English selection requires the separate optional
`whisper-base` profile. That profile uses `SOT + selected language + TRANSCRIBE +
NO_TIMESTAMPS`, including when English is explicitly selected. Choosing a language does
not select or download another model, alter the answer provider or change CPU/Metal.
An incompatible profile/language pair remains saved and is reported as unavailable;
listening and one-file transcription reject it before model-file or capture I/O.

The native row uses the existing General row dimensions and egui dropdown, with an
accessible label and normal keyboard/semantic actions. It extends the Cueglint Local
models setup card; the General heading, hero, Preferences and earlier capture rows
keep their geometry. This addition is not a measured Cluely Language-row reproduction.
Capture initialization, listening and finalization retain the existing settings guard.
Every capture worker owns the selection frozen at its start. New transcript events and
CLI output take `language` from the loaded transcriber, independently of current UI
preferences. The value records selected input language, not detected or quality-verified
output language. Existing session identities, bounded queues, stop/cancellation and final
transcript draining remain intact.

ASR selection follows the existing ASR model/backend contract: it applies to future
capture workers and does not cancel unrelated frozen answers/highlights or rewrite their
context. New finalized transcripts still advance context through the existing core path.
Language changes preserve historical transcripts and human notes. Missing legacy settings
and backups default to English without a schema bump; invalid values fail before database
or backup replacement. Existing audio provenance is never retagged after selection.

The managed API is `Transcriber::load_profile_with_language`. The existing
`load_profile_with_backend` signature remains an English-selection wrapper, and direct
directory constructors retain their legacy behavior. CPU remains the saved default;
explicit Metal availability/error handling and the absence of automatic fallback remain.
Model hashes and publisher declarations are recorded in
[the public-source metadata](../../models/whisper-base-sources.json). No multilingual
model loading, inference, accuracy, live latency, native-device, offline-package or matched
reference acceptance follows from token/source checks or synthetic unit tests.
