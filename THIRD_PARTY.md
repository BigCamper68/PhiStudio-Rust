# Third-party software and assets

PhiStudio Rust combines original editor code with the software and resources
identified below. The application is distributed under **GPL-3.0-only**; the
complete license is in [LICENSE](LICENSE). Separate dependency and asset
licenses remain applicable. This document does not relicense third-party work.

## Renderer and reference implementation

- **Phira / prpr**, by Mivik and the Phira contributors, is used under
  GPL-3.0-only. The parser, animation system, renderer, judgment logic, particles,
  shaders and companion crates in `vendor/phira` come from
  [TeamFlos/phira](https://github.com/TeamFlos/phira) at commit
  `0afd16621dd66557841a8cffe6813afff63335ec`. Its license is
  [vendor/phira/LICENSE](vendor/phira/LICENSE). `vendor/phira/UPSTREAM.json`
  records the original Git blobs and SHA-256 hashes. The outer vendored workspace
  member list is adjusted for this application; `UPSTREAM-Cargo.toml` preserves
  the upstream file. Editor integration is in `crates/phistudio-prpr`. A local
  Noise Field input-filter hook is documented in `vendor/phira/PHISTUDIO-PATCH.md`
  and `vendor/phira/patches/noise-touch-filter.patch`. The source verifier reverses
  that patch before checking the original upstream hashes in `UPSTREAM.json`.
- **PhiStudio Java**, by BigCamper68 and its contributors, is an MIT-licensed
  reference at commit `bb2e9b34520a97b10e9012c26101aec061b9a004` in
  [BigCamper68/PhiStudio](https://github.com/BigCamper68/PhiStudio).
  Conversion rules, selected authoring behavior and the demonstration fixture
  derive from that reference. The original notice is retained in
  [licenses/PhiStudio-Java-MIT.txt](licenses/PhiStudio-Java-MIT.txt).
  Conversion fixture provenance is recorded in
  `crates/phistudio-core/tests/fixtures/conversion/SOURCE.json`.
  The Java project's license is unchanged.

## UI, graphics and audio dependencies

| Component | Source / version | License and retained notices |
| --- | --- | --- |
| Macroquad | [Mivik/prpr-macroquad](https://github.com/Mivik/prpr-macroquad), revision `2150130` | MIT OR Apache-2.0; consult the pinned source |
| Miniquad | [Mivik/prpr-miniquad](https://github.com/Mivik/prpr-miniquad), revision `138679c` | MIT OR Apache-2.0; `licenses/Miniquad-LICENSE-MIT.txt` and `licenses/Miniquad-LICENSE-APACHE.txt` |
| egui / epaint | [emilk/egui](https://github.com/emilk/egui), 0.21.0 | MIT OR Apache-2.0 for code; bundled fonts have separate terms below |
| egui-miniquad | 0.14.0, vendored with local changes | MIT OR Apache-2.0; both license texts and `PHISTUDIO-PATCH.md` are in `vendor/egui-miniquad` |
| glam | [bitshifter/glam-rs](https://github.com/bitshifter/glam-rs), 0.30.10 | MIT OR Apache-2.0; shared pure vector/matrix math, matching Macroquad |
| RustFFT | [ejmahler/RustFFT](https://github.com/ejmahler/RustFFT), 6.4.1 | MIT OR Apache-2.0; MIT notice in `licenses/RustFFT-MIT.txt` |
| sha2 | [RustCrypto/hashes](https://github.com/RustCrypto/hashes), 0.10.9 | MIT OR Apache-2.0; MIT notice in `licenses/sha2-MIT.txt` |
| fastblur | [fschutt/fastblur](https://github.com/fschutt/fastblur), 0.1.1 | MIT; `licenses/fastblur-LICENSE.txt` |
| fs2 | [danburkert/fs2-rs](https://github.com/danburkert/fs2-rs), 0.4.3 | MIT OR Apache-2.0; consult the pinned crate source |
| chrono | [chronotope/chrono](https://github.com/chronotope/chrono), 0.4.45 | MIT OR Apache-2.0; consult the pinned crate source |
| Wasmi | [wasmi-labs/wasmi](https://github.com/wasmi-labs/wasmi), 2.0.0 | MIT OR Apache-2.0; MIT notice in `licenses/Wasmi-LICENSE-MIT.txt`; portable plugin interpreter, including Wasmi companion crates |
| libloading | [nagisa/rust_libloading](https://github.com/nagisa/rust_libloading), 0.8.9 | ISC; notice in `licenses/libloading-LICENSE.txt`; native plugin library loading |
| reqwest | [seanmonstar/reqwest](https://github.com/seanmonstar/reqwest), 0.12.28 | MIT OR Apache-2.0; asynchronous host HTTP with rustls TLS |
| tokio-tungstenite / tungstenite | [snapview/tokio-tungstenite](https://github.com/snapview/tokio-tungstenite), 0.30.0 | MIT / MIT OR Apache-2.0; bounded host WebSocket transport |
| keyring | [open-source-cooperative/keyring-rs](https://github.com/open-source-cooperative/keyring-rs), 4.2.0 | MIT OR Apache-2.0; desktop protected credential backends |
| Oboe bindings | [katyo/oboe-rs](https://github.com/katyo/oboe-rs), 0.6.1 | Apache-2.0; consult the dependency source and its bundled Oboe notices |
| sasa | [Mivik/sasa](https://github.com/Mivik/sasa), revision `e76229b` | The pinned manifest does not declare an SPDX license; consult its upstream source and notices |

`Cargo.lock` records the complete resolved dependency graph, registry checksums
and Git revisions, including transitive dependencies. This table highlights
major integrations and is not a substitute for individual dependency licenses.
Notices from newly resolved networking, TLS and credential-store dependencies
are retained in [`licenses/sdk-network/`](licenses/sdk-network/), with package
names and versions in filenames. This includes their platform backend notices.

Android `MainActivity` and `QuadNative` adapt the pinned Miniquad templates.
Application-specific changes cover package and library names, storage, input,
keyboard handling, display density, lifecycle, document pickers and safe areas.
The vendored egui-miniquad adapter contains compatibility, buffer lifetime, touch
input and DPI changes documented in its retained patch notice.

The Gradle wrapper and build tooling use Gradle's Apache-2.0 licensed software.
Android packages also include the selected NDK's runtime notices for the bundled
C++ runtime. The Android and iOS packaging scripts include this document, the
application license, and the retained `licenses` directory.

## Fonts

- **Ubuntu Regular**, copyright Canonical Ltd., is taken from the Phira 0.8.2
  release and distributed under Ubuntu Font Licence 1.0. See
  `licenses/Phira-Ubuntu-Font-Licence.txt`.
- **PGR / phigros.ttf**, copyright Mivik, is taken from the pinned Phira source
  under GPL-3.0-only.
- **Noto Emoji**, used by the UI and as a native emoji fallback, is distributed
  under SIL Open Font License 1.1. See `licenses/egui-fonts/OFL.txt`.
- egui's other embedded fonts, including **Ubuntu Light**, **Hack**, and
  **emoji-icon-font**, retain their own notices in `licenses/egui-fonts`.

Exact source locations, revisions, hashes and copyright information for the
application's font files are recorded in `assets/fonts/SOURCE.json`.

## Phigros Official resource pack

The images, sounds and metadata in `assets/skins/phigros` are identified by the
supplied pack as **Phigros Official**, authored by **PigeonGames**. These files
are retained unchanged. `assets/skins/phigros/SOURCE.json` records their hashes
and original archive provenance; `info.yml` retains the pack attribution.

These assets retain their original rights. Their inclusion does not place them
under the application's GPL license or grant additional redistribution rights.
No license grant for these assets is asserted by this document. No third-party
music or illustration collection is bundled. User-imported charts and resources
remain subject to their respective owners' terms.

## Noise Fields

Noise Fields in `crates/phistudio-prpr/src/noise_fields` use shaders, material
parameters and decoded textures extracted from the user-supplied Phigros Android
build 160 XAPK. These game assets retain their original ownership; the project's
GPL license does not relicense them. The geometry adapter was informed by
[Phira PR #901](https://github.com/TeamFlos/phira/pull/901) at
`0afa0fa11320a7c457f2fcce536c446df4b898ed` (2923fzyxfkj, GPL-3.0), then checked
against the supplied build's `PreviewBlockControl` and `GetEase` ARM64 methods.
The source manifest and adaptation details are in
`crates/phistudio-prpr/src/noise_fields/SOURCE.json` and `docs/NOISE-FIELDS.md`.

The optional color material mapping adapts PhiStudio Java's
`NoiseFieldColorShaders.java` at the MIT-licensed revision listed above. Field
forms and the Line2Area protocol were also studied at that revision; retained
source data and generated marker guides are separate from the extracted Phigros
resources. The 1×1 marker PNG in `assets/noise-fields` is generated by this project.

Noise editor behavior and Next 220 schema were studied in rpe-next at
`a9c5bc2b776513bac015516e916258be2f452f81`; the Rust implementation does not include
rpe-next implementation code. Regression data in
`crates/phistudio-core/tests/fixtures/noise-fields` comes from GPL-3.0 Phira Pro at
`d4b62e04d7d827d481812e49cee0b69d0fab46ad`. Its `SOURCE.json` retains original paths
and hashes; the samples record the rpe-next generator revision separately.

## Independently implemented features

The audio-guide analyzer and project version-control implementation are original
Rust code. They do not bundle a neural model or an external analysis program.
Feature behavior was informed in part by
[rpe-next](https://github.com/kclg-YSJ/rpe-next); no rpe-next source code or model
weights are included. This reference does not imply that rpe-next is licensed
under this project's GPL license.

Application icon variants share the original PhiStudio Rust IPA artwork through
`tools/icon_art.py`; they are not taken from a third-party icon pack.

## Verification

Run these commands from the repository root to verify retained source and asset
provenance:

```sh
python3 tools/verify-prpr-source.py
python3 tools/verify-note-skin.py
python3 tools/verify-preview-assets.py
python3 tools/verify-noise-fields.py
python3 tools/generate-icons.py --check
```
