# Editor sounds

Open **Settings → Audio → Editor sounds**. Feedback has its own volume and does
not use music volume, chart hitsounds, skins, exported charts or video audio.
Preferences are saved automatically with the editor's existing settings.

- Choose Soft, Mechanical or Arcade for the built-in sound palette.
- Adjust master volume, pitch and the minimum interval for repeated feedback.
  Value changes and scrubbing have a minimum interval of 90 ms.
- Choose an action under **Customize each action** to enable/mute it, select one
  of six tones, or set its volume and pitch independently.
- **Import sound…** accepts audio supported by the existing decoder, including
  WAV, OGG, MP3 and FLAC, up to 2 seconds and 2 MiB. Mobile uses the system picker;
  desktop uses the editor's existing path dialog. A private copy lives under
  `editor-sounds/` in the application data directory. Selecting a built-in tone
  removes the custom assignment. Missing or invalid files fall back to a built-in
  sound and show a warning in these settings.
- **Test sound** previews the current action even when its enable switch or the
  master switch is off. It respects volume and playback muting.
- Reset one action or all sound preferences separately from other settings.
  Resetting assignments retains imported files in the application library.

Sounds cover buttons, toggles, numeric changes, menus, object selection, tools,
seeking, playback, successful editing commands, copy/paste, undo/redo, saves and
errors. Semantic actions take priority over their button click so one action
produces one cue. Canceled input is silent. Playback muting is on by default;
Playtest and application suspension always mute feedback and discard stale cues.

The feature is inspired by [Editor Sounds by Alphalaneous](https://github.com/Alphalaneous/Editor-Sounds).
Built-in tones are synthesized specifically for PhiStudio. No code or audio
assets from that mod are included.

## Validation

```sh
cargo test -p phistudio-core -p phistudio-prpr -p phistudio-app --lib --locked
cargo build -p phistudio-app --example ui-layout-probe --locked
xvfb-run -a -s '-screen 0 420x900x24' target/debug/examples/ui-layout-probe \
  artifacts/checks/editor-sounds sounds 420x900
```

The probe renders the real English and Russian settings using the shared GPU
adapter. Mixer tests cover gain, pitch, bounded polyphony and mute/resume without
replaying stale feedback. Device audio latency and system-picker behavior still
need checks on each supported mobile platform.
