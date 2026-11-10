# Editor sounds

**Settings → Audio → Editor sounds** contains the master enable switch, volume,
selected sound pack and **Editor Sounds Manager** button. Feedback is independent
of music, chart hitsounds, skins, exported charts and video audio. Preferences
apply across projects and are saved automatically.

## Sound packs

Soft, Mechanical and Arcade are immutable built-in packs. Duplicate one to edit
it, or create a new user pack. The **Manage sound packs** section supports naming, switching, copying,
deleting, importing and exporting user packs. Up to 32 user packs are retained.
Deleting a pack retains imported audio in the private library.

The original single-action preferences migrate to one selected **My sounds** pack,
including disabled actions, custom files, palette, pitch and repeat interval.
An unchanged legacy palette selects its corresponding built-in pack. Switching
packs never overwrites another pack's rules or changes chart data/history.

The manager groups the original 19 actions into Interface, Notes and objects,
Navigation and playback, and History and files. Five additional placement actions
allow different feedback for Tap, Hold, Flick, Drag and line events. They inherit
**Place objects** until explicitly customized; resetting an override restores
inheritance. Select an action directly from the grouped list, then adjust its
volume, pitch, built-in fallback tone and audio variants.

## Variants and playback

Each action accepts up to four WAV, OGG, MP3 or FLAC variants, each at most 2 seconds
and 2 MiB. Imports use the existing mobile system picker or desktop path dialog
and create private copies under `editor-sounds/`. The manager displays duration,
file size and a waveform prepared by the audio worker. Preview one variant or the
whole action. Preview ignores enable switches; volume and playback muting apply.

Random selection avoids immediately repeating the same variant when more than
one is available. Disable it for sequential playback. Missing or invalid files
fall back to the action's built-in tone with a visible warning.

Pack playback settings contain the palette, pitch and repeat interval. Each
category can override that interval (20–500 ms); it applies across actions in the
category. Numeric changes and scrubbing retain a minimum of 90 ms. Decode work is
bounded to 96 unique files and 64 MiB of decoded samples per active pack.

**Move and resize objects** plays the same cue while dragging notes/events,
changing Hold/event endpoints and moving/scaling selections. A cue is emitted
when the staged preview crosses a grid position, including moving back to a
previous cell. Staying within one cell and releasing at the same position are
silent. Free movement uses visible grid crossings for feedback. Selection scaling
uses small scale steps. The gesture still commits once and has one Undo entry;
canceling drops staged edits and emits no additional feedback.

Semantic actions take priority over button clicks. Playback muting is enabled
by default. Playtest, lost application focus and suspension mute feedback and
discard stale cues.

## Portable format

**Export sound pack…** writes a `.phisounds.json` file through the existing save
workflow. The versioned `phistudio.soundpack` document embeds all custom audio as
base64, so it works on another device without the original paths. Shared files
are stored once. Master enable/volume and playback muting remain local preferences.

**Import sound pack…** validates the format, references and every audio file on a
worker before committing the private library directory. Imports always receive a
fresh local pack ID. References are data IDs, never archive extraction paths.
Limits are 24 MiB per document, 16 MiB of embedded audio and 96 unique files.
Unsupported formats, invalid audio and incomplete packs leave existing packs and
assignments unchanged.

The feature is inspired by [Editor Sounds by Alphalaneous](https://github.com/Alphalaneous/Editor-Sounds).
Built-in tones are synthesized for PhiStudio. No code or audio assets from the
mod are included.

## Validation

```sh
cargo test -p phistudio-core -p phistudio-prpr -p phistudio-app --lib --locked
cargo build -p phistudio-app --example ui-layout-probe --locked
xvfb-run -a -s '-screen 0 420x900x24' target/debug/examples/ui-layout-probe \
  artifacts/checks/editor-sounds sounds 420x900
```

The GPU probe renders compact settings and the English/Russian manager, including
decoded variant waveforms, without editing a chart. Tests cover live grid feedback,
Undo/cancellation, migration, pack isolation, portable audio round-trips, rejected
imports and variant ordering. Device audio latency, system pickers and lifecycle
still need physical-device checks on each mobile platform.
