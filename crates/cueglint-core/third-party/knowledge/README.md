# Knowledge ingestion dependencies

No upstream source code was copied or modified. The exact dependencies and
their original notices are retained here; `Cargo.lock` pins archive checksums.

| Package | Version | License | Upstream commit | Registry archive SHA-256 |
| --- | --- | --- | --- | --- |
| [zip](https://github.com/zip-rs/zip2) | 9.6.1 | MIT | `8773769c64ae576716f9f3d8599200f3c345d7cb` | `953a376f4930ed5af88e1e65948385f76866b45e7f75b21ac275327d0eeb9b46` |
| [quick-xml](https://github.com/tafia/quick-xml) | 0.41.0 | MIT | `4deda08abeffdc188c269360229cf47e12a77a9f` | `e660451e55124f798a69a5af3f49ccfbefbd41910eefd25caf2393e1f3473ec1` |
| [lopdf](https://github.com/J-F-Liu/lopdf) | 0.47.0 | MIT | `e9a0380b26d678b307e37d529ed8b8b04339727b` | `c6ad532ff9262dd4bfd6e085400cbb39959d48cb8d08518b01b68f77bdf6feca` |

Both packages already existed in the baseline lockfile. Core now directly uses
them; ZIP enables only `deflate-flate2-zlib-rs` with default features disabled.
This reuses locked flate2 1.1.11 (MIT OR Apache-2.0) and zlib-rs 0.6.8 (Zlib),
adds no package/version, and enables no encryption, native codec,
network, process, OCR, model or hosted service. quick-xml defaults are disabled.
The lazy ZIP iterator is an upstream public unstable API and is therefore pinned
with an exact version, as required by its documentation.

The PDF source candidate adds exactly pinned lopdf with all default features
disabled, excluding its optional Rayon pool, clocks, image/font embedding and
async runtime. It runs only inside Cueglint's allocation-budgeted native child;
see [the PDF contract](../../../../spec/features/native-knowledge-pdf.md).
The candidate also directly reuses locked libc 0.2.191 (MIT OR Apache-2.0) on
Unix and windows-sys 0.61.2 (MIT OR Apache-2.0) with
`Win32_System_Threading` on Windows for immediate allocation-free child exit.
No upstream code is copied or forked. The original lopdf notice SHA-256 is
`2fb01e6708ea9d53e8b7e74bd82af23bce5256f19f4de93915027a53173c50d8`.
The new transitive lockfile resolution and notices are **pending** the shared
Cargo lease; this dependency candidate is not ready for publication yet.
Direct flate2 1.1.11 reuses the locked pure Rust zlib-rs feature and checks actual
EOF/checksum errors without lopdf's partial-output fallback. Exact direct-source
and original-notice identities are in `pdf-source-ledger.json`; the windows-sys
MIT notice is retained under that license choice. New parser transitives must
be pinned and their notices reviewed before publication.

Stored notice identities (SHA-256): `zip-9.6.1-LICENSE`
`58545fed1565e42d687aecec6897d35c6d37ccb71479a137c0deb2203e125c79`;
`quick-xml-0.41.0-LICENSE-MIT.md`
`f0cf9b1c62bbe3bd3a69f5f79c7158f513f612b4940a0a812d1db39d605318bc`.
The quick-xml notice uses LF line endings here; upstream's CRLF original is
`5b2c207dcf571267ebe0bbdd5bf321f561ddc22ff597e7b363e96f4539507a29`.
