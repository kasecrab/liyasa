# Third-party licences

Liyasa ships other people's work. This file lists it: the crates it is built from, and the fonts, icon sets, grammars and dictionaries it bundles or downloads.

It is generated. Run `cargo run -p xtask -- notices` and commit the result; the crate rows come from the resolved dependency graph and the asset rows from `xtask/assets.toml`, which is transcribed from the requirements' own inventory.

The licences Liyasa accepts are a fixed list, enforced by `cargo deny check licenses` on every pull request. `cargo run -p xtask -- licences` enforces the other direction: that the list `cargo deny` is given is still the list the requirements name.

## Assets

`Shipped` is whether the file travels in a Liyasa release. A row that is not shipped is fetched at run time and its licence is shown and accepted at the download.

<!-- assets: generated from xtask/assets.toml -->

| Asset | Source | Licence | Shipped | Licence text |
|---|---|---|---|---|
| Inter (variable) | rsms/inter | OFL-1.1 | yes | `crates/liyasa-theme/assets/fonts/Inter-LICENSE.txt` |
| JetBrains Mono (variable) | JetBrains/JetBrainsMono | OFL-1.1 | yes | `crates/liyasa-theme/assets/fonts/JetBrainsMono-LICENSE.txt` |
| Lucide icons | lucide-icons/lucide | ISC | yes | — |
| Phosphor icons | phosphor-icons | MIT | yes | — |
| Tabler icons | tabler/tabler-icons | MIT | yes | — |
| Font Awesome Free | FortAwesome/Font-Awesome | CC-BY-4.0 AND OFL-1.1 AND MIT | yes | — |
| TextMate grammars (audited subset) | per-language upstream repositories, catalogued by shikijs | MIT AND BSD-3-Clause AND Apache-2.0 | yes | — |
| Shiki themes (audited subset) | shikijs/textmate-grammars-themes | MIT | yes | — |
| Mermaid | mermaid-js/mermaid | MIT | yes | — |
| KaTeX fonts and CSS | KaTeX/KaTeX | MIT | yes | — |
| Snowball stemmers | snowballstem, through tantivy | BSD-3-Clause | yes | — |
| Emoji shortcode table | github/gemoji | MIT | yes | — |
| Country and region names | Unicode CLDR | Unicode-3.0 | yes | — |
| AI agent user-agent list | Liyasa-maintained | Apache-2.0 | yes | — |
| Lindera dictionaries | lindera-morphology/lindera | IPADIC AND BSD-3-Clause AND Apache-2.0 AND CC-BY-SA-3.0 | on demand | — |
| axe-core | dequelabs/axe-core | MPL-2.0 | on demand | — |
| Playwright-managed Chromium | the Chromium project, via Playwright | BSD-3-Clause | on demand | — |

<!-- end assets -->

## Crates

570 packages in the resolved graph, this workspace's own excluded. The graph is resolved for every target and feature, so a crate here may not be in any particular build.

| Crate | Version | Licence |
|---|---|---|
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| aead | 0.6.1 | MIT OR Apache-2.0 |
| aes | 0.9.3 | MIT OR Apache-2.0 |
| aes-gcm | 0.11.1 | Apache-2.0 OR MIT |
| ahash | 0.8.12 | MIT OR Apache-2.0 |
| aho-corasick | 1.1.5 | Unlicense OR MIT |
| aligned-vec | 0.6.4 | MIT |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 |
| anstream | 1.0.0 | MIT OR Apache-2.0 |
| anstyle | 1.0.14 | MIT OR Apache-2.0 |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 |
| anstyle-query | 1.1.5 | MIT OR Apache-2.0 |
| anstyle-wincon | 3.0.11 | MIT OR Apache-2.0 |
| arc-swap | 1.9.2 | MIT OR Apache-2.0 |
| argon2 | 0.6.0 | MIT OR Apache-2.0 |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 |
| asn1-rs | 0.7.2 | MIT OR Apache-2.0 |
| asn1-rs-derive | 0.6.0 | MIT OR Apache-2.0 |
| asn1-rs-impl | 0.2.0 | MIT/Apache-2.0 |
| async-channel | 2.5.0 | Apache-2.0 OR MIT |
| async-compression | 0.4.47 | MIT OR Apache-2.0 |
| async-trait | 0.1.92 | MIT OR Apache-2.0 |
| atoi | 2.0.0 | MIT |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT |
| autocfg | 1.5.1 | Apache-2.0 OR MIT |
| axum | 0.8.9 | MIT |
| axum-core | 0.5.6 | MIT |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| base64 | 0.23.1 | MIT OR Apache-2.0 |
| base64ct | 1.8.3 | Apache-2.0 OR MIT |
| bincode | 1.3.3 | MIT |
| bit-set | 0.8.0 | Apache-2.0 OR MIT |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT |
| bit-vec | 0.9.1 | Apache-2.0 OR MIT |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| bitpacking | 0.9.3 | MIT |
| blake2 | 0.11.0 | MIT OR Apache-2.0 |
| blake3 | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 |
| boa_ast | 0.22.0 | Unlicense OR MIT |
| boa_engine | 0.22.0 | Unlicense OR MIT |
| boa_gc | 0.22.0 | Unlicense OR MIT |
| boa_interner | 0.22.0 | Unlicense OR MIT |
| boa_macros | 0.22.0 | Unlicense OR MIT |
| boa_parser | 0.22.0 | Unlicense OR MIT |
| boa_string | 0.22.0 | Unlicense OR MIT |
| bon | 3.10.1 | MIT OR Apache-2.0 |
| bon-macros | 3.10.1 | MIT OR Apache-2.0 |
| borrow-or-share | 0.2.4 | MIT-0 |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 |
| bytecount | 0.6.9 | Apache-2.0/MIT |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT |
| byteorder | 1.5.0 | Unlicense OR MIT |
| byteorder-lite | 0.1.0 | Unlicense OR MIT |
| bytes | 1.12.1 | MIT |
| caseless | 0.2.2 | MIT |
| cc | 1.4.6 | MIT OR Apache-2.0 |
| census | 0.4.2 | MIT |
| cfg-if | 1.0.4 | MIT OR Apache-2.0 |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 |
| cipher | 0.5.2 | MIT OR Apache-2.0 |
| clap | 4.6.7 | MIT OR Apache-2.0 |
| clap_builder | 4.6.7 | MIT OR Apache-2.0 |
| clap_complete | 4.6.11 | MIT OR Apache-2.0 |
| clap_derive | 4.6.7 | MIT OR Apache-2.0 |
| clap_lex | 1.1.1 | MIT OR Apache-2.0 |
| cmov | 0.5.4 | Apache-2.0 OR MIT |
| colorchoice | 1.0.5 | MIT OR Apache-2.0 |
| combine | 4.6.8 | MIT |
| compression-codecs | 0.4.42 | MIT OR Apache-2.0 |
| compression-core | 0.4.33 | MIT OR Apache-2.0 |
| comrak | 0.55.0 | BSD-2-Clause |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT |
| const-oid | 0.10.2 | Apache-2.0 OR MIT |
| const-str | 1.1.0 | MIT |
| constant_time_eq | 0.4.2 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| convert_case | 0.6.0 | MIT |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 |
| cow-utils | 0.1.3 | MIT |
| cpubits | 0.1.1 | MIT OR Apache-2.0 |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 |
| crc | 3.4.0 | MIT OR Apache-2.0 |
| crc-catalog | 2.5.0 | MIT OR Apache-2.0 |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 |
| critical-section | 1.2.0 | MIT OR Apache-2.0 |
| crossbeam-channel | 0.5.17 | MIT OR Apache-2.0 |
| crossbeam-deque | 0.8.8 | MIT OR Apache-2.0 |
| crossbeam-epoch | 0.9.21 | MIT OR Apache-2.0 |
| crossbeam-queue | 0.3.14 | MIT OR Apache-2.0 |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 |
| crunchy | 0.2.4 | MIT |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 |
| cssparser | 0.37.0 | MPL-2.0 |
| cssparser-color | 0.5.0 | MPL-2.0 |
| cssparser-macros | 0.7.1 | MPL-2.0 |
| ctr | 0.10.1 | MIT OR Apache-2.0 |
| ctutils | 0.4.2 | Apache-2.0 OR MIT |
| curve25519-dalek | 5.0.0 | BSD-3-Clause |
| curve25519-dalek-derive | 0.1.1 | MIT/Apache-2.0 |
| darling | 0.24.1 | MIT |
| darling_core | 0.24.1 | MIT |
| darling_macro | 0.24.1 | MIT |
| dashmap | 6.2.1 | MIT |
| data-encoding | 2.11.1 | MIT |
| datasketches | 0.2.0 | Apache-2.0 |
| der-parser | 10.0.0 | MIT OR Apache-2.0 |
| deranged | 0.5.8 | MIT OR Apache-2.0 |
| digest | 0.10.7 | MIT OR Apache-2.0 |
| digest | 0.11.3 | MIT OR Apache-2.0 |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 |
| dotenvy | 0.15.7 | MIT |
| downcast-rs | 2.0.2 | MIT OR Apache-2.0 |
| dtoa | 1.0.11 | MIT OR Apache-2.0 |
| dtoa-short | 0.3.5 | MPL-2.0 |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 |
| dynify | 0.1.2 | MIT OR Apache-2.0 |
| dynify-macros | 0.1.2 | MIT OR Apache-2.0 |
| ed25519 | 3.0.0 | Apache-2.0 OR MIT |
| ed25519-dalek | 3.0.0 | BSD-3-Clause |
| either | 1.18.0 | MIT OR Apache-2.0 |
| email_address | 0.2.9 | MIT |
| emojis | 0.8.2 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| entities | 1.0.1 | MIT |
| equator | 0.4.2 | MIT |
| equator-macro | 0.4.2 | MIT |
| equivalent | 1.0.2 | Apache-2.0 OR MIT |
| erased-serde | 0.4.10 | MIT OR Apache-2.0 |
| errno | 0.3.14 | MIT OR Apache-2.0 |
| event-listener | 5.4.2 | Apache-2.0 OR MIT |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT |
| fancy-regex | 0.16.2 | MIT |
| fancy-regex | 0.19.2 | MIT |
| fast-float2 | 0.2.4 | MIT OR Apache-2.0 |
| fastdivide | 0.4.2 | zlib-acknowledgement OR MIT |
| fastrand | 2.5.0 | Apache-2.0 OR MIT |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 |
| fiat-crypto | 0.3.0 | MIT OR Apache-2.0 OR BSD-1-Clause |
| file-id | 0.2.3 | MIT OR Apache-2.0 |
| find-msvc-tools | 0.1.12 | MIT OR Apache-2.0 |
| finl_unicode | 1.4.0 | (MIT OR Apache-2.0) AND Unicode-DFS-2016 |
| fixedbitset | 0.5.7 | MIT OR Apache-2.0 |
| flate2 | 1.1.10 | MIT OR Apache-2.0 |
| fluent-uri | 0.4.1 | MIT |
| flume | 0.12.0 | Apache-2.0/MIT |
| fnv | 1.0.7 | Apache-2.0 / MIT |
| foldhash | 0.2.0 | Zlib |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 |
| fraction | 0.17.0 | MIT OR Apache-2.0 |
| fs4 | 0.13.1 | MIT OR Apache-2.0 |
| fsevent-sys | 4.1.0 | MIT |
| fst | 0.4.7 | Unlicense/MIT |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 |
| futures-concurrency | 7.7.1 | MIT OR Apache-2.0 |
| futures-core | 0.3.34 | MIT OR Apache-2.0 |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 |
| futures-intrusive | 0.5.0 | MIT OR Apache-2.0 |
| futures-io | 0.3.34 | MIT OR Apache-2.0 |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 |
| generic-array | 0.14.7 | MIT |
| getrandom | 0.2.17 | MIT OR Apache-2.0 |
| getrandom | 0.3.4 | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |
| ghash | 0.6.0 | Apache-2.0 OR MIT |
| h2 | 0.4.19 | MIT |
| hashbrown | 0.14.5 | MIT OR Apache-2.0 |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| hashlink | 0.11.1 | MIT OR Apache-2.0 |
| heck | 0.5.0 | MIT OR Apache-2.0 |
| hex | 0.4.3 | MIT OR Apache-2.0 |
| hickory-net | 0.26.3 | MIT OR Apache-2.0 |
| hickory-proto | 0.26.3 | MIT OR Apache-2.0 |
| hickory-resolver | 0.26.3 | MIT OR Apache-2.0 |
| hmac | 0.13.0 | MIT OR Apache-2.0 |
| htmlescape | 0.3.1 | Apache-2.0 / MIT / MPL-2.0 |
| http | 1.5.0 | MIT OR Apache-2.0 |
| http-body | 1.1.0 | MIT |
| http-body-util | 0.1.5 | MIT |
| httparse | 1.10.1 | MIT OR Apache-2.0 |
| httpdate | 1.0.3 | MIT OR Apache-2.0 |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 |
| hyper | 1.11.1 | MIT |
| hyper-rustls | 0.27.9 | Apache-2.0 OR ISC OR MIT |
| hyper-util | 0.1.20 | MIT |
| icu_collections | 2.3.0 | Unicode-3.0 |
| icu_locale_core | 2.3.0 | Unicode-3.0 |
| icu_locale_fallback | 2.3.0 | Unicode-3.0 |
| icu_locale_fallback_data | 2.3.0 | Unicode-3.0 |
| icu_normalizer | 2.3.0 | Unicode-3.0 |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 |
| icu_properties | 2.3.0 | Unicode-3.0 |
| icu_properties_data | 2.3.0 | Unicode-3.0 |
| icu_provider | 2.3.1 | Unicode-3.0 |
| icu_segmenter | 2.3.0 | Unicode-3.0 |
| icu_segmenter_data | 2.3.0 | Unicode-3.0 |
| ident_case | 1.0.1 | MIT/Apache-2.0 |
| idna | 1.1.0 | MIT OR Apache-2.0 |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT |
| image | 0.25.10 | MIT OR Apache-2.0 |
| image-webp | 0.2.4 | MIT OR Apache-2.0 |
| indexmap | 2.14.2 | Apache-2.0 OR MIT |
| inotify | 0.11.5 | ISC |
| inotify-sys | 0.1.8 | ISC |
| inout | 0.2.2 | MIT OR Apache-2.0 |
| instant-acme | 0.8.5 | Apache-2.0 |
| intrusive-collections | 0.10.3 | MIT OR Apache-2.0 |
| inventory | 0.3.24 | MIT OR Apache-2.0 |
| ipconfig | 0.3.4 | MIT/Apache-2.0 |
| ipnet | 2.12.2 | MIT OR Apache-2.0 |
| is_ci | 1.2.0 | ISC |
| is_terminal_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| itertools | 0.10.5 | MIT/Apache-2.0 |
| itertools | 0.14.0 | MIT OR Apache-2.0 |
| itertools | 0.15.0 | MIT OR Apache-2.0 |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| jetscii | 0.5.3 | MIT OR Apache-2.0 |
| jni | 0.22.4 | MIT OR Apache-2.0 |
| jni-macros | 0.22.4 | MIT OR Apache-2.0 |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 |
| js-sys | 0.3.105 | MIT OR Apache-2.0 |
| jsonschema | 0.56.0 | MIT |
| jsonschema-regex | 0.56.0 | MIT |
| jsonschema-value | 0.56.0 | MIT |
| kqueue | 1.2.1 | MIT |
| kqueue-sys | 1.1.2 | MIT |
| lazy_static | 1.5.0 | MIT OR Apache-2.0 |
| levenshtein_automata | 0.2.1 | MIT |
| libc | 0.2.189 | MIT OR Apache-2.0 |
| libsqlite3-sys | 0.37.0 | MIT |
| lightningcss | 1.0.0-alpha.72 | MPL-2.0 |
| lightningcss-derive | 1.0.0-alpha.43 | MPL-2.0 |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| litemap | 0.8.3 | Unicode-3.0 |
| lock_api | 0.4.14 | MIT OR Apache-2.0 |
| log | 0.4.34 | MIT OR Apache-2.0 |
| lru | 0.16.4 | MIT |
| markdown | 1.0.0 | MIT |
| matchers | 0.2.0 | MIT |
| matchit | 0.8.4 | MIT AND BSD-3-Clause |
| measure_time | 0.9.0 | MIT |
| memchr | 2.8.3 | Unlicense OR MIT |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 |
| memo-map | 0.3.4 | Apache-2.0 |
| micromap | 0.3.0 | MIT |
| miette | 7.6.0 | Apache-2.0 |
| miette-derive | 7.6.0 | Apache-2.0 |
| mime | 0.3.17 | MIT OR Apache-2.0 |
| minijinja | 2.24.0 | Apache-2.0 |
| minimal-lexical | 0.2.1 | MIT/Apache-2.0 |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 |
| mio | 1.2.3 | MIT |
| moka | 0.12.16 | (MIT OR Apache-2.0) AND Apache-2.0 |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 |
| murmurhash32 | 0.3.1 | MIT |
| ndk-context | 0.1.1 | MIT OR Apache-2.0 |
| nom | 7.1.3 | MIT |
| notify | 8.2.0 | CC0-1.0 |
| notify-debouncer-full | 0.6.0 | MIT OR Apache-2.0 |
| notify-types | 2.1.0 | MIT OR Apache-2.0 |
| num | 0.4.3 | MIT OR Apache-2.0 |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 |
| num-bigint | 0.5.1 | MIT OR Apache-2.0 |
| num-cmp | 0.1.0 | MIT/Apache-2.0 |
| num-complex | 0.4.6 | MIT OR Apache-2.0 |
| num-conv | 0.2.2 | MIT OR Apache-2.0 |
| num-integer | 0.1.47 | MIT OR Apache-2.0 |
| num-iter | 0.1.46 | MIT OR Apache-2.0 |
| num-rational | 0.4.2 | MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| num_enum | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 |
| num_enum_derive | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 |
| num_threads | 0.1.7 | MIT OR Apache-2.0 |
| oid-registry | 0.8.1 | MIT OR Apache-2.0 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| once_cell_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| oneshot | 0.1.13 | MIT OR Apache-2.0 |
| oneshot | 0.2.1 | MIT OR Apache-2.0 |
| openssl-probe | 0.2.1 | MIT OR Apache-2.0 |
| ordered-float | 5.5.0 | MIT |
| outref | 0.5.2 | MIT |
| ownedbytes | 0.9.0 | MIT |
| owo-colors | 4.4.0 | MIT |
| parcel_selectors | 0.28.3 | MPL-2.0 |
| parking | 2.2.1 | Apache-2.0 OR MIT |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 |
| password-hash | 0.6.1 | MIT OR Apache-2.0 |
| pastey | 0.1.1 | MIT OR Apache-2.0 |
| pastey | 0.2.3 | MIT OR Apache-2.0 |
| pathdiff | 0.2.3 | MIT/Apache-2.0 |
| pem | 4.0.0 | MIT |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| phc | 0.6.1 | Apache-2.0 OR MIT |
| phf | 0.11.3 | MIT |
| phf | 0.13.1 | MIT |
| phf | 0.14.0 | MIT |
| phf_codegen | 0.11.3 | MIT |
| phf_codegen | 0.13.1 | MIT |
| phf_generator | 0.11.3 | MIT |
| phf_generator | 0.13.1 | MIT |
| phf_generator | 0.14.0 | MIT |
| phf_macros | 0.13.1 | MIT |
| phf_macros | 0.14.0 | MIT |
| phf_shared | 0.11.3 | MIT |
| phf_shared | 0.13.1 | MIT |
| phf_shared | 0.14.0 | MIT |
| pin-project | 1.1.13 | Apache-2.0 OR MIT |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 |
| png | 0.18.1 | MIT OR Apache-2.0 |
| polyval | 0.7.3 | Apache-2.0 OR MIT |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT |
| potential_utf | 0.1.6 | Unicode-3.0 |
| powerfmt | 0.2.0 | MIT OR Apache-2.0 |
| precomputed-hash | 0.1.1 | MIT |
| prefix-trie | 0.8.4 | MIT OR Apache-2.0 |
| prettyplease | 0.3.0 | MIT OR Apache-2.0 |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| pulldown-cmark | 0.13.4 | MIT |
| pulldown-cmark-escape | 0.11.0 | MIT |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 |
| quick-error | 2.0.1 | MIT/Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| rand | 0.10.2 | MIT OR Apache-2.0 |
| rand | 0.8.8 | MIT OR Apache-2.0 |
| rand_core | 0.10.1 | MIT OR Apache-2.0 |
| rand_core | 0.6.4 | MIT OR Apache-2.0 |
| rayon | 1.12.0 | MIT OR Apache-2.0 |
| rayon-core | 1.13.0 | MIT OR Apache-2.0 |
| rcgen | 0.14.10 | MIT OR Apache-2.0 |
| redox_syscall | 0.5.18 | MIT |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 |
| referencing | 0.56.0 | MIT |
| regex | 1.13.1 | MIT OR Apache-2.0 |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 |
| regress | 0.12.0 | MIT OR Apache-2.0 |
| reqwest | 0.13.5 | MIT OR Apache-2.0 |
| resolv-conf | 0.7.6 | MIT OR Apache-2.0 |
| ring | 0.17.14 | Apache-2.0 AND ISC |
| rust-stemmers | 1.2.0 | MIT/BSD-3-Clause |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 |
| rusticata-macros | 4.1.0 | MIT/Apache-2.0 |
| rustix | 1.1.4 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| rustls | 0.23.45 | Apache-2.0 OR ISC OR MIT |
| rustls-native-certs | 0.8.4 | Apache-2.0 OR ISC OR MIT |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 |
| rustls-platform-verifier | 0.7.0 | MIT OR Apache-2.0 |
| rustls-platform-verifier-android | 0.1.1 | MIT OR Apache-2.0 |
| rustls-webpki | 0.103.15 | ISC |
| rustversion | 1.0.23 | MIT OR Apache-2.0 |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 |
| ryu-js | 1.0.3 | Apache-2.0 OR BSL-1.0 |
| same-file | 1.0.6 | Unlicense/MIT |
| schannel | 0.1.29 | MIT |
| schemars | 0.8.22 | MIT |
| schemars | 1.2.2 | MIT |
| schemars_derive | 0.8.22 | MIT |
| schemars_derive | 1.2.2 | MIT |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 |
| security-framework | 3.7.0 | MIT OR Apache-2.0 |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 |
| semver | 1.0.28 | MIT OR Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde-wasm-bindgen | 0.6.5 | MIT |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive_internals | 0.29.1 | MIT OR Apache-2.0 |
| serde_derive_internals | 0.30.0 | MIT OR Apache-2.0 |
| serde_json | 1.0.151 | MIT OR Apache-2.0 |
| serde_norway | 0.9.42 | MIT OR Apache-2.0 |
| serde_path_to_error | 0.1.20 | MIT OR Apache-2.0 |
| serde_spanned | 1.1.1 | MIT OR Apache-2.0 |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| sha2 | 0.11.0 | MIT OR Apache-2.0 |
| sharded-slab | 0.1.7 | MIT |
| shlex | 2.0.1 | MIT OR Apache-2.0 |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 |
| signature | 3.0.0 | Apache-2.0 OR MIT |
| simd-adler32 | 0.3.10 | MIT |
| simd_cesu8 | 1.2.0 | Apache-2.0 OR MIT |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 |
| siphasher | 1.0.3 | MIT/Apache-2.0 |
| sketches-ddsketch | 0.4.1 | Apache-2.0 |
| slab | 0.4.12 | MIT |
| small_btree | 0.1.0 | Unlicense OR MIT |
| smallvec | 1.16.1 | MIT OR Apache-2.0 |
| socket2 | 0.6.5 | MIT OR Apache-2.0 |
| spin | 0.9.9 | MIT |
| sqlx | 0.9.0 | MIT OR Apache-2.0 |
| sqlx-core | 0.9.0 | MIT OR Apache-2.0 |
| sqlx-macros | 0.9.0 | MIT OR Apache-2.0 |
| sqlx-macros-core | 0.9.0 | MIT OR Apache-2.0 |
| sqlx-sqlite | 0.9.0 | MIT OR Apache-2.0 |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 |
| strsim | 0.11.1 | MIT |
| strum | 0.28.0 | MIT |
| strum_macros | 0.28.0 | MIT |
| subtle | 2.6.1 | BSD-3-Clause |
| supports-color | 3.0.2 | Apache-2.0 |
| supports-hyperlinks | 3.2.0 | Apache-2.0 |
| supports-unicode | 3.0.0 | Apache-2.0 |
| syn | 1.0.109 | MIT OR Apache-2.0 |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.5 | MIT OR Apache-2.0 |
| sync_wrapper | 1.0.2 | Apache-2.0 |
| synstructure | 0.13.2 | MIT |
| synstructure | 0.14.0 | MIT |
| syntect | 5.3.0 | MIT |
| system-configuration | 0.7.0 | MIT OR Apache-2.0 |
| system-configuration-sys | 0.6.0 | MIT OR Apache-2.0 |
| tag_ptr | 0.1.0 | Unlicense OR MIT |
| tagptr | 0.2.0 | MIT/Apache-2.0 |
| tantivy | 0.26.2 | MIT |
| tantivy-bitpacker | 0.10.0 | MIT |
| tantivy-columnar | 0.7.0 | MIT |
| tantivy-common | 0.11.0 | MIT |
| tantivy-fst | 0.5.0 | Unlicense/MIT |
| tantivy-query-grammar | 0.26.0 | MIT |
| tantivy-sstable | 0.7.0 | MIT |
| tantivy-stacker | 0.7.0 | MIT |
| tantivy-tokenizer-api | 0.7.0 | MIT |
| tap | 1.0.1 | MIT |
| tempfile | 3.27.0 | MIT OR Apache-2.0 |
| terminal_size | 0.4.4 | MIT OR Apache-2.0 |
| textwrap | 0.16.4 | MIT |
| thin-vec | 0.2.19 | MIT OR Apache-2.0 |
| thiserror | 2.0.20 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.20 | MIT OR Apache-2.0 |
| thread_local | 1.1.10 | MIT OR Apache-2.0 |
| time | 0.3.55 | MIT OR Apache-2.0 |
| time-core | 0.1.9 | MIT OR Apache-2.0 |
| time-macros | 0.2.32 | MIT OR Apache-2.0 |
| tinystr | 0.8.4 | Unicode-3.0 |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT |
| tokio | 1.53.1 | MIT |
| tokio-macros | 2.7.2 | MIT |
| tokio-rustls | 0.26.5 | MIT OR Apache-2.0 |
| tokio-stream | 0.1.19 | MIT |
| tokio-util | 0.7.19 | MIT |
| toml | 1.1.6+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_datetime | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_edit | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_parser | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_writer | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 |
| tower | 0.5.3 | MIT |
| tower-http | 0.6.11 | MIT |
| tower-http | 0.7.1 | MIT |
| tower-layer | 0.3.3 | MIT |
| tower-service | 0.3.3 | MIT |
| tracing | 0.1.44 | MIT |
| tracing-attributes | 0.1.31 | MIT |
| tracing-core | 0.1.36 | MIT |
| tracing-serde | 0.2.0 | MIT |
| tracing-subscriber | 0.3.23 | MIT |
| try-lock | 0.2.5 | MIT |
| two-face | 0.5.2+bat-0.26.1 | MIT OR Apache-2.0 |
| typed-arena | 2.0.2 | MIT |
| typeid | 1.0.3 | MIT OR Apache-2.0 |
| typenum | 1.20.1 | MIT OR Apache-2.0 |
| typetag | 0.2.23 | MIT OR Apache-2.0 |
| typetag-impl | 0.2.23 | MIT OR Apache-2.0 |
| typify | 0.8.0 | Apache-2.0 |
| typify-impl | 0.8.0 | Apache-2.0 |
| ulid | 3.0.0 | MIT |
| unicase | 2.9.0 | MIT OR Apache-2.0 |
| unicode-general-category | 1.1.0 | Apache-2.0 |
| unicode-id | 0.3.6 | MIT OR Apache-2.0 |
| unicode-ident | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-normalization | 0.1.25 | MIT OR Apache-2.0 |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 |
| unicode-width | 0.1.14 | MIT OR Apache-2.0 |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 |
| universal-hash | 0.6.1 | MIT OR Apache-2.0 |
| unsafe-libyaml-norway | 0.2.15 | MIT |
| untrusted | 0.9.0 | ISC |
| url | 2.5.8 | MIT OR Apache-2.0 |
| utf16_iter | 1.0.5 | Apache-2.0 OR MIT |
| utf8-ranges | 1.0.5 | Unlicense/MIT |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT |
| uuid | 1.26.1 | Apache-2.0 OR MIT |
| uuid-simd | 0.8.0 | MIT |
| valuable | 0.1.1 | MIT |
| vcpkg | 0.2.15 | MIT/Apache-2.0 |
| version_check | 0.9.5 | MIT/Apache-2.0 |
| vsimd | 0.8.0 | MIT |
| walkdir | 2.5.0 | Unlicense/MIT |
| want | 0.3.1 | MIT |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasm-bindgen | 0.2.128 | MIT OR Apache-2.0 |
| wasm-bindgen-futures | 0.4.78 | MIT OR Apache-2.0 |
| wasm-bindgen-macro | 0.2.128 | MIT OR Apache-2.0 |
| wasm-bindgen-macro-support | 0.2.128 | MIT OR Apache-2.0 |
| wasm-bindgen-shared | 0.2.128 | MIT OR Apache-2.0 |
| web-sys | 0.3.105 | MIT OR Apache-2.0 |
| web-time | 1.1.0 | MIT OR Apache-2.0 |
| webpki-root-certs | 1.0.9 | CDLA-Permissive-2.0 |
| widestring | 1.2.1 | MIT OR Apache-2.0 |
| winapi | 0.3.9 | MIT/Apache-2.0 |
| winapi-i686-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 |
| winapi-util | 0.1.11 | Unlicense OR MIT |
| winapi-x86_64-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-registry | 0.6.1 | MIT OR Apache-2.0 |
| windows-result | 0.4.1 | MIT OR Apache-2.0 |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 |
| windows-sys | 0.59.0 | MIT OR Apache-2.0 |
| windows-sys | 0.60.2 | MIT OR Apache-2.0 |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 |
| windows-targets | 0.53.5 | MIT OR Apache-2.0 |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_aarch64_gnullvm | 0.53.1 | MIT OR Apache-2.0 |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_aarch64_msvc | 0.53.1 | MIT OR Apache-2.0 |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnu | 0.53.1 | MIT OR Apache-2.0 |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnullvm | 0.53.1 | MIT OR Apache-2.0 |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_msvc | 0.53.1 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.53.1 | MIT OR Apache-2.0 |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnullvm | 0.53.1 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.53.1 | MIT OR Apache-2.0 |
| winnow | 1.0.4 | MIT |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| write16 | 1.0.0 | Apache-2.0 OR MIT |
| writeable | 0.6.4 | Unicode-3.0 |
| x509-parser | 0.18.1 | MIT OR Apache-2.0 |
| yasna | 0.6.0 | MIT OR Apache-2.0 |
| yoke | 0.8.3 | Unicode-3.0 |
| yoke-derive | 0.8.2 | Unicode-3.0 |
| zerocopy | 0.8.57 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerocopy-derive | 0.8.57 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerofrom | 0.1.8 | Unicode-3.0 |
| zerofrom-derive | 0.1.7 | Unicode-3.0 |
| zeroize | 1.9.0 | Apache-2.0 OR MIT |
| zerotrie | 0.2.5 | Unicode-3.0 |
| zerovec | 0.11.8 | Unicode-3.0 |
| zerovec-derive | 0.11.6 | Unicode-3.0 |
| zlib-rs | 0.6.7 | Zlib |
| zmij | 1.0.23 | MIT |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib |
