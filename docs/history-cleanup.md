# Retired fixture history cleanup

The owner explicitly authorized a path-only rewrite on2026-11-17. Seven public branches
were updated atomically using exact current-head leases. Only
`parity/fixtures/clean-english.wav` was removed. Other source, v2 fixtures, reports, messages,
authors, committers, dates and ordered parents were preserved. Writable branch histories
contain25 mapped commits and five removed signature headers. The private verification
also examined a read-only PR merge, for26 mappings and six signature headers in that
comparison. No replacement signatures are claimed.

Original evidence reports keep their original commit IDs and hashes. Use the mapping below
when locating the corresponding source in cleaned history. This does not change what an
older test actually executed or restore its retired waveform. Private sealed backups and a
recoverable bundle preserve the original state and unpublished work.

Five closed-PR head refs still retained original objects on GitHub after the branch updates.
Read-only PR refs, host caches and external clones were not purged. Branch cleanup is not
a claim that every hosted copy has been deleted. No support message was sent.

See [the execution record](../parity/reports/history-path-cleanup-001.json).

The `3bb5d8d` PR merge row is a private comparison mapping, not a pushed branch commit.
Its cleaned comparison object is not promised to be reachable on GitHub. Runtime source
content for that merge is also present at cleaned feature head `25afff84`.


| Original commit | Cleaned comparison commit |
|---|---|
| `0b04d4882c31a56b81938b1ddc59fe8ed89b6ea3` | `ad94535b15be0ae4871c260510c46c928ad87cdf` |
| `2a25b5719fac555e27a864eb2a95f6d40eddf1b7` | `276e33ead76512df20272a5d470cbf7b6a6d7635` |
| `39894179cea55def69eac37b30f4ca1d29ecb643` | `ac3fb62b40ede0edb67cf83af2305cfdfb46d8d3` |
| `3bb5d8d3a9b5a652b7251be68741df68c3a68840` | `230e0f3f469a81e6a2bea1ecc33b875a079f2c7b` |
| `41cc436768ffd296282a28b92f22e28019641628` | `6a215f40e3b5354324bd77ffcf1fc01c9da99eea` |
| `4afddd67d1928566b4132be83d5f5e72bb208e04` | `bc7994a7e1fe0501311b3230ef91c31b29b62fc1` |
| `4f152790378233208ffe7f37604896c6f38b44fa` | `4be11a6f38e64b3d020973ae0f165ff46386cb0f` |
| `69eb16d09af5e01042796916e989a2eee459e120` | `25afff84e3cb6e46ed7aa9b34e6524e73c91d84a` |
| `6eb31ad132beb64e098239876090e30733e8f142` | `32eabc5f80837ea483831fdef9bfc84792ecb4b5` |
| `6ef73fd6540b5b9b5c136777a5401f1f3fdcd10c` | `97b87d7050e913ec36798941e79598b56e043441` |
| `70d28afe9d40c877a1b59531a2aeaa41da3bd3b4` | `0b35a55dc437c42d1bc2698c85db5b5a5bba7c4d` |
| `72d88e8356eea06a959543af442778cfd850b254` | `782d87fb802548f2a7fa51718cb58b3f331c24fe` |
| `80b30758046f4616afba52429e5825ff80a7e486` | `a56815ececf86769a16b07a6a6ab4f94e6fce18a` |
| `83cb59829702d73f423d194fa1eb99b0cd658aec` | `e6a25cd1a77d31ee7e8fd99717ab85c529612274` |
| `89c72c1edc9451ecf44bea8ae6946dd0a6e4d1ba` | `8fff9bb76d41fe406c2bec4c902f5f52479ec77f` |
| `8a5d3c6dffcb62b572eca3699c247ec7c25859eb` | `cb1dae5381b16e29df010ad2379958cacb196770` |
| `9540b8d5446500df6ee9c4935a415559b974661b` | `5b4856ed843ac0984d14790bd9720589a746b62a` |
| `9ee292067a07bcee9589116a27af91087f639606` | `459bfaa675168257a3a2fe9fc21c1539f0612426` |
| `a722c56463e092dbeccbdc37f6edf34ba2a2a42b` | `21aed171831832861906afe32f2b87fa6441872e` |
| `be5d064d60ea13374dcaa70b19ec501285b56bd2` | `d580fc30f4357875969d69753368087411b79ad9` |
| `c042c3aea49d4f2709bacf6d39f60ec9d39271fd` | `06927aa24cfbdce120001a7625aa34f7b2be1369` |
| `cdf38aff8100060aa2ce269e053a3f75a58d9098` | `2a18ffc95cf677c72b600242efa883b3c34b513f` |
| `d376d88f1e300845dc87d8c0a8b5f7fd81841a3c` | `c5f04c061af1f5708cf2c8932f7c54dd6f290bbf` |
| `d693c11274207acf18f4e04ac965ea2038fdaa76` | `9883cd8b4ad61d63648b9a5e6eb5cc51b5ef7f05` |
| `eb98aa08bee9866dd2d00bcc720b784131525e82` | `cbae556a0ec55a09fbe257ebaa700e09daab1fe1` |
| `fd53c3a09ebff827c56ede0774c1ced9f87c58ec` | `043e185e339c56948bc267a86c6f88885b8a0da3` |
