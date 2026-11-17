# Verified local model inputs

This is an original Cueglint input-integrity contract, not an observed reference behavior
or a claim about model quality. Managed generation and transcription construct from the
same read-only file objects that supplied their catalog-pinned sizes and hashes.

The loader validates the profile and regular non-linked directory/artifact rules, opens
and hashes every required artifact, and retains those handles rewound to byte zero.
The catalog permits at most ten files per profile. Verification and owned reads use
128 KiB chunks, check cancellation, and reject unexpected sizes or hashes. Unknown
artifacts cannot be requested from the retained input set. No model work occurs in
drawing or audio callbacks.

Qwen parses metadata and reads tensors from its verified GGUF handle, without another
whole-GGUF buffer. Tokenizers/configuration/safetensors use owned buffers read from their
retained handles. All catalog artifacts still verify, including Qwen licenses. Metadata
is taken from the same resolved profile that supplied those inputs. Native transcript
events carry that bound profile through persistence; actual backend labels still come
from the selected Device.

ASR checks cancellation and selects its explicit CPU/Metal device before opening model
inputs. An uncompiled or unavailable Metal device fails with its original error and no
CPU or hosted fallback. Readiness/drawing only report compile support. Direct-directory
ASR entry points preserve their existing signatures, validation order and unmanaged
legacy backend behavior; they do not infer catalog pins or require coordination files.

No reader lock is added. Ordinary verification remains a read-only metadata snapshot,
and custom/read-only catalog roots without `.install.lock` remain usable. Existing
exclusive mutation locks, stable coordination inode and publication/removal rules stay
unchanged. The guarantee covers existing cooperative atomic rename/unlink publishers;
it does not cover uncooperative in-place writes or adversarial namespace replacement.
External benchmark locks must be identified as external controls.

Input handles close on construction success, failure or cancellation. Returned models
own their tensor/tokenizer data, so no handle/lease is retained in the generation and
highlights cache. Its root/profile key and current reset-after-file-operation behavior
remain unchanged. There is no interprocess cache unload or file-watcher guarantee.
An mmap/lazy-file backend would require a separate lifetime review.

CLI transcription `elapsed_ms` includes verification, construction and decoding. Existing
reports retain their exact source, configuration and timing scope. New source-only tests
cover tiny authored files and controlled invalid inputs; passing them cannot establish
actual model, Metal, packaged/live-capture or reference parity claims.
