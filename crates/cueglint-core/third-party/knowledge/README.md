# Knowledge ingestion dependencies

No upstream source code was copied or modified. The exact dependencies and
their original notices are retained here; `Cargo.lock` pins archive checksums.

| Package | Version | License | Upstream commit | Registry archive SHA-256 |
| --- | --- | --- | --- | --- |
| [zip](https://github.com/zip-rs/zip2) | 9.6.1 | MIT | `8773769c64ae576716f9f3d8599200f3c345d7cb` | `953a376f4930ed5af88e1e65948385f76866b45e7f75b21ac275327d0eeb9b46` |
| [quick-xml](https://github.com/tafia/quick-xml) | 0.41.0 | MIT | `4deda08abeffdc188c269360229cf47e12a77a9f` | `e660451e55124f798a69a5af3f49ccfbefbd41910eefd25caf2393e1f3473ec1` |

Both packages already existed in the baseline lockfile. Core now directly uses
them; ZIP enables only `deflate-flate2-zlib-rs` with default features disabled.
This reuses locked flate2 1.1.11 (MIT OR Apache-2.0) and zlib-rs 0.6.8 (Zlib),
adds no package/version, and enables no encryption, native codec,
network, process, OCR, model or hosted service. quick-xml defaults are disabled.
The lazy ZIP iterator is an upstream public unstable API and is therefore pinned
with an exact version, as required by its documentation. No PDF parser is linked.

Original notice identities (SHA-256): `zip-9.6.1-LICENSE`
`58545fed1565e42d687aecec6897d35c6d37ccb71479a137c0deb2203e125c79`;
`quick-xml-0.41.0-LICENSE-MIT.md`
`5b2c207dcf571267ebe0bbdd5bf321f561ddc22ff597e7b363e96f4539507a29`.
