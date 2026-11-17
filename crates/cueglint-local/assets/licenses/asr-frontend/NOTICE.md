ASR frontend source attribution proposal

Newly authored Cueglint feature assembly in src/asr_frontend.rs is offered under
MIT OR Apache-2.0. The FFT/DFT helper adaptation is identified separately in
src/asr_frontend/fft.rs and retains the inherited notices below.

Hugging Face Candle 0.11.1 offers MIT OR Apache-2.0 in its pinned workspace Cargo
metadata. The copied helper bodies come from candle-transformers/src/models/
whisper/audio.rs at commit 59f8e903ca80a32ea7f8514b39748fc979e69e82.
Exact source SHA256:
ed5f1c81a0bebf0e67f8066f51d87cf8312a48aadb3a9a3a126ab7c09714880b
Pinned license files are retained verbatim as LICENSE-MIT.candle and
LICENSE-APACHE.candle. Candle's MIT file contains no named copyright line;
this proposal does not invent or substitute a copyright holder for it.

Candle's audio source attributes its FFT/DFT functions to whisper.cpp at commit
4774d2feb01a772a15de81ffc34b34a1f294f020. That commit's MIT LICENSE is retained
verbatim as LICENSE-MIT.whisper.cpp, including:
Copyright (c) 2023 Georgi Gerganov
The complete inherited MIT permission/disclaimer notice is also reproduced in
the adapted fft.rs source, rather than retaining its URL alone.

Adaptation: generic Candle helpers specialized to f64, with parent-only helper
visibility for the new frontend. The source assembly, centered reflect padding,
one-sided power and log/mel normalization are newly authored Cueglint code.

Exact pinned root-tree listings contained no root NOTICE file in either project.
This records the inspected files, not an assurance about legal compliance or
about other code/assets outside this small adaptation.

Upstream source and license locations:
LICENSE-MIT.whisper.cpp
  https://raw.githubusercontent.com/ggerganov/whisper.cpp/4774d2feb01a772a15de81ffc34b34a1f294f020/LICENSE
  SHA256 ef78c7e6659e34c798194f0d344e807c722de70cbca1359ef54772661e11ca38
LICENSE-MIT.candle
  https://raw.githubusercontent.com/huggingface/candle/59f8e903ca80a32ea7f8514b39748fc979e69e82/LICENSE-MIT
  SHA256 23f18e03dc49df91622fe2a76176497404e46ced8a715d9d2b67a7446571cca3
LICENSE-APACHE.candle
  https://raw.githubusercontent.com/huggingface/candle/59f8e903ca80a32ea7f8514b39748fc979e69e82/LICENSE-APACHE
  SHA256 c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4

Candle source:
https://github.com/huggingface/candle/blob/59f8e903ca80a32ea7f8514b39748fc979e69e82/candle-transformers/src/models/whisper/audio.rs
