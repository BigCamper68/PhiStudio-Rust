# Noise Fields

Choose **Noise Fields** in the timeline workspace selector. The workspace follows
Events/Storyboard: two simultaneous timelines, an inspector, and the shared
transport and Preview. Fields occupy the left grid; selecting one exposes all
of its animation columns in the right grid. Both surfaces share time, zoom and
the draggable divider. The mobile inspector can float
and uses the same touch scrolling and dropdown controls as the other editors.

- Create a field with **+ Field**, or select that tool and draw its lifetime
  upwards in the left grid. Place normal and subtract fields anywhere along X.
  Empty-space drags pan horizontally or scrub time; horizontal wheel gestures
  and Shift+wheel also pan. The left grid extends in both X directions without
  a boundary and uses the same vertical grid count as Events, configured in
  **Grid → Vertical grid lines**. This adjusts only the thin grid and X snapping
  step. Numbered X columns and field widths stay fixed; labels begin at X1.
  Creation and moves snap to the fine vertical grid and beat divisions. The Grid
  window also controls X/time snapping. Changing density never moves existing
  fields or resizes their numbered columns. The first controls above the
  left grid, **< X >**, can jump to distant columns. All animation columns fit the right
  grid's width, like Events, with horizontal navigation disabled. The workspace
  selector is first above the right grid, followed by the field list. Choosing
  a field in that list centers its column on the left and seeks to its lifetime,
  including when choosing the current field again after manually panning away.
- Edit its name, appearance/activation/disappearance, rectangle and RGB color.
  Geometry, RGB and the miniature rectangle preview appear first in the inspector.
  **Edit field in Preview** opens a live full-screen geometry editor: drag either
  corner to resize, or the center handle to move the rectangle. Live drafts keep
  their ordinary lifetime, fade, activation and subtraction behavior. All fields
  share the original mask/parity pass while editing; no visibility is forced.
  Geometry and point edits both preview the actual animation at the current time.
 Swipe elsewhere
  vertically to scrub chart time, including inside a full-screen field. The
  spatial grid uses the main vertical count and horizontal division; X/time snap
  switches control its X/Y alignment. **Field preview grid**, available below
  horizontal settings in **Grid** and in the full-screen toolbar, controls both
  full-screen and miniature grid visibility independently of snapping.
  **Apply field** commits the full-screen session as one Undo entry; Cancel,
  Escape or Back restores the inspector's original draft. Interrupted touch
  gestures restore the current drag without discarding earlier staged edits.
  **Apply field** in the inspector commits its pending edits;
  Save, playback, export and workspace navigation also apply valid pending edits.
- The right grid exposes native Move/Scale/Rotate point keys, or Next
  Activation/Move X/Move Y/Scale X/Scale Y/Rotate interval tracks, plus Color.
  With the placement tool active, drawing in a column creates that track's key.
  Blocks share Storyboard's interval frames and selection grips. Animation value
  captions use the same renderer as Events: intervals up to 28 pixels hide their
  captions, narrow/tall blocks show endpoints separately, and other blocks use
  one compact start-to-end label. Captions are clipped to their own block.
  Native keys show outgoing interpolation intervals between real point keys; a
  terminal point is a short endpoint cap. New native animations use two sampled
  point keys one Grid tick apart. New Next animations also start at one Grid
  tick. Draw upwards without releasing to set their length. Upper native grips
  move the following point (or create an endpoint for a terminal point), while
  lower grips move the start. No interval-duration extension is added to native
  JSON. Dragging the middle moves the entire animation interval; native moves
  shift both real endpoints, with shared points shifted only once for groups.
  Short blocks keep their middle available for moving. Resize previews and
  cancellation use the same reversible gesture path.
  Selection, box selection, clipboard and erasing act on the focused field or
  animation column. Native keys have separate outgoing X/Y easing and scale/rotation anchors.
  Next intervals include 29 easings, easing windows, Bezier, instant values,
  anchors, transform references/steps and link-group metadata. **Pick anchor**
  and **Pick position** open the shared Preview grid with a labeled coordinate
  point. Tap to place it or drag its marker; swiping elsewhere scrubs chart time.
  The point remains staged until **Apply field** confirms the session as one
  Undo entry. Cancel/Back/Escape restores the previous draft, including implicit
  anchors. Geometry changes appear in live Preview without writing the chart.
  Color uses the
  shared RGB palette for static colors and both key endpoints.
- Select, cycle overlapping intervals, box-select, drag/resize, erase, reorder,
  copy/cut/paste, duplicate, select all, and Undo/Redo use the project history.
  **+ Field** / **+ Keyframe** select the drawing tool. **Add field at cursor** /
  **Add key at cursor** immediately insert at the transport position. Cursor
  field creation places field #N exactly in XN, bypassing fine X snapping and
  current panning. If that column is offscreen, it is revealed immediately;
  the transport position is preserved. Visible columns keep their camera position. A timeline
  drag, including a group move or paired native endpoints, creates one history entry. Canceling
  touch input discards the gesture. New keys begin at the current channel value.

## Saved formats and Phira Pro

The new-field format selector lists rpe-next first and defaults to it for new
projects. Selecting an existing field never converts its source format.

Native fields retain `bottomLeftPercentage`/`topRightPercentage`, four timestamps
in chart seconds, `isSubtract`, and native keys/eases 0..14. The editor displays
beats through the current BPM map without rounding unchanged source seconds.
Native lifetime moves shift native animation by the same seconds delta.

Next fields retain `bottomLeft`/`topRight` in the centered 1350×900 canvas,
beat triples, multiple `activeIntervals`, `readyFlash` seconds, `isInvert` and
independent interval tracks. Authoring Next data raises `META.RPEVersion` to at
least 220; Undo restores the former version. Animation runs in chart time, does
not apply the audio offset twice, and retains values across gaps. Cumulative
center transforms apply X scale, Y scale, clockwise rotation, then absolute
movement. Standard Next easing includes its own Elastic InOut period; ordinary
RPE line playback still uses the pinned prpr tween table. The optional
**Single inversion only** setting edits `noiseAreaOptions.ignoreTripleInversion`.
Next 230 custom shader skins/layers are retained as unknown JSON, not authored.

Saving retains the source dialect, raw key order and unknown properties. Names
use `phiStudioName` for native data and `Name` for Next. `editorPositionX` stores
only a field column's layout, matching rpe-next's editor metadata; moving it
horizontally never moves the playable rectangle. View offsets are independent
of chart edits. RGB authoring uses
`phiStudioColor`; `phiStudioColorEvents` contains standard beat-based RGB interval
keys. These editor extensions are not part of the original Phigros protocol.

Phira Pro's native field parser does not read these color extensions. When a
project contains custom field colors, **Export chart** creates a snapshot with
Line2Area marker lines named `isSubtract0.png` / `isSubtract1.png`, the required
1×1 guide resources, geometry/scale tracks, RGB `extended.colorEvents`, and
lifecycle speed codes 1/2/3/4. Geometry and RGB curves are adaptively baked to
linear events with byte-valued RGB endpoints; guide names avoid resource-folder collisions. Native colors and
Next colors share this adapter. Uncolored exports retain the original fields.
The working document and editor Undo history are never converted during export.

`phiStudioNoiseSource` stores the source fields and hashes of the generated lines
and BPM map. Reimport restores exact authoring data only when those generated
tracks are unchanged. External edits are kept when either hash differs.

Line2Area uses the player's immediate show behavior and fixed 0.5-second Ready
material. Colored export rejects custom Ready durations that cannot be represented,
reversed/out-of-lifetime native phases, and excessive sample counts. Keep a
project copy for authoring; a JSON Save alone does not package Phira Pro guides.
The adapter bakes the native geometry on a 3:2 canvas, as the Java reference does;
non-3:2 rotated percentage fields can differ from the native projection in Phira
Pro. This is a player protocol limit, not a change to Preview's native geometry.

## Preview and Playtest

The integration renders HiddenBefore, Disabled, Ready, Active and HiddenAfter,
including the 0.5-second show fade and 0.5-second Ready interval. A short
pre-enable interval carries the show fade across the phase boundary. It evaluates
move, scale and rotation keys with their separate anchors, in scale → rotate →
move order. Keys are sorted in the renderer's copy. Easing follows GetEase's
101-sample power tables, including Zero and One. Seeking evaluates geometry and
visual time directly; it does not advance a stateful animation incrementally.

The GPU path follows the supplied Phigros Android build 160 gameplay prefab:
six camera masks at one eighth of canvas dimensions, native subtraction masks,
BlockCompose, EdgeMask pass 1, additive DisabledBlock and premultiplied
ActiveBlock composition. Disabled cameras also include Ready objects, preserving
the translucent red base beneath the Ready shine. This prefab has `edgeSize=1`; the glow-ring shader in a
newer public Phira port is deliberately not substituted for that pass.

The original GLES3 programs are adapted to GLES2 syntax and Macroquad vertex
attributes. Four scalar masks are packed into one RGBA texture to fit mobile
sampler limits. Textures have no mipmaps, so explicit LOD-zero samples become
ordinary texture samples. Material numbers, UV transforms, shader arithmetic,
noise textures and import settings are retained: Point filtering for the three
noise maps, mirrored repetition for displacement/cell noise, repetition for spark
noise, and Bilinear/Clamp for the hover sprite. Texture PNG rows use
Unity's GPU orientation, rather than the vertically flipped image-export view.
All work shares Preview's existing GL context and prpr ping-pong targets.

Noise Fields are drawn over the chart and hit particles, before the HUD. The
chart shader checkbox only controls chart-authored effects. Autoplay, scrubbing,
fullscreen Preview and offline frame export use the same field renderer.
Charts without fields allocate no field materials or intermediate targets.

Playtest follows build 160 JudgeControl: active normal rectangles form a union,
subtract rectangles toggle parity, and blocking requires both the original mask
and the adjusted mask. Normal rectangles shrink by 5% of the world screen height;
subtract rectangles expand by the same amount. Tests use the animated, rotated
rectangles, with degenerate axes rejected. Entering a field latches that finger
until release/cancellation, even if it leaves the field or the field disappears.
Other fingers remain usable. Ready/Disabled fields never block new contacts.

A documented, reversible prpr patch provides a callback after native input merging,
viewport normalization and timestamping, before all four note kinds are judged.
Native blocking rules live in the integration layer. Next input uses the same
continuous geometry as its masks, with the selected inversion mode. Pause, seek, retry, project replacement
and exit clear contact state and restore audio. The original ten touch-hover slots,
11.5 sprite scale and 0.1-second animation use the extracted tight sprite mesh and
shader effects. Music cutoff moves to 1500 Hz over 0.1 seconds while any finger is
blocked and returns to normal on release. The actual output rate is observed from
the audio backend. sasa supplies a one-pole filter; its DSP waveform is not identical
to Unity's AudioLowPassFilter.

A real-device, side-by-side capture against Phigros has not been performed;
automated checks do not establish pixel identity across different GPUs or Unity's
entire scene. Android physical multitouch validation remains necessary.

## Source evidence

The user supplied `com.pigeongames.phigros_160.xapk`. Its data asset pack contains
`sharedassets12.assets` with the shaders, materials and gameplay prefabs.
`config.arm64_v8a-160.apk` contains `libil2cpp.so`; the base APK contains IL2CPP v31
metadata. `SOURCE.json` records hashes, object IDs, method RVAs and adaptations.
No APK, complete binary or unrelated game resources are included in the project.
`tools/extract-noise-fields.py` reproduces the bundled assets from the assembled
Unity file with UnityPy 1.25.4; `tools/verify-noise-fields.py --original-assets
/path/to/sharedassets12.assets` compares every generated byte with the manifest.

Geometry initially used the implementation in Phira PR #901, revision
`0afa0fa11320a7c457f2fcce536c446df4b898ed`, as a reference. The supplied binary
shows that animation keys do not apply before the first key; the Preview adapter
preserves that behavior. The renderer uses the supplied shaders directly instead
of that PR's different glow pipeline. Attribution is recorded in THIRD_PARTY.md.

## Verification

```sh
cargo test -p phistudio-prpr --lib --locked
cargo test -p phistudio-core --test noise_fields --locked
cargo test -p phistudio-app noise_ui::tests --locked
cargo build -p phistudio-prpr --example noise-fields --example playtest-input --locked
python3 tools/verify-noise-fields.py
# With a working GL context (for example private Xvfb with Mesa llvmpipe):
xvfb-run -a target/debug/examples/noise-fields
xvfb-run -a python3 tools/playtest-smoke.py
xvfb-run -a target/debug/examples/ui-layout-probe artifacts/checks/noise-ui noise-fields 873x393
```

The GPU probe checks five phases, hidden-field cleanup, exact repeated seeks,
subtract cancellation/parity, animated geometry, chart replacement/removal, shader toggling,
portrait/square/wide canvases and target resizing. Its PNGs and report are written
to `artifacts/checks/noise-fields`. Unit tests cover phase endpoints, anchors,
pre-first-key behavior, zero scales, easing table interpolation and untouched
source JSON. The conversion test covers `blockAreaList: null` and seconds/unknown
extension preservation through native import and RPE round-trips.

The real-input probe compares ordinary Tap/Hold/Drag/Flick play with active fields,
inactive warnings, matching subtract fields, a latched contact after disappearance,
and a larger render target with the same physical input viewport. It checks native
judgement counts and low-pass transitions. Pure tests cover independent fingers,
release/cancel/lost-contact cleanup, animated hit geometry and edge tolerances.
The GPU probe can also take an extracted RPE JSON as its second argument to capture
Desultory Signals around beats 322, 331.5, 332, 333 and 333.5 (202 BPM).

The editor regression tests also cover sparse history, exact undo of missing/null
containers, invalid-batch rollback, native seconds versus Next beats, new-key
sampling, resource packaging, animated RGB export and safe source restoration.
The Next fixture contains 61 fields and 2,074 samples generated upstream, checked
at six aspect ratios and in reverse time order. The GPU probe covers colored
Disabled/Ready/Active states, color animation, canceled color masks, Next XOR
and the exactly-one-inversion option. UI captures exercise phone/tablet widths,
three locales, native/Next inspectors and the shared palette.

Reference revisions for authoring and interoperability:

- [rpe-next](https://github.com/kclg-YSJ/rpe-next/tree/a9c5bc2b776513bac015516e916258be2f452f81):
  format and editor behavior, independently implemented; no implementation source
  is copied from this reference.
- [PhiStudio Java](https://github.com/BigCamper68/PhiStudio/tree/bb2e9b34520a97b10e9012c26101aec061b9a004):
  NoiseField forms, Line2Area export and color-material mapping (MIT notice retained).
- [Phira Pro](https://github.com/Phira-Pro/Phira-Pro/tree/d4b62e04d7d827d481812e49cee0b69d0fab46ad):
  marker/color protocol and Next compatibility. Fixture file hashes and its
  original rpe-next generator revision are retained beside the regression data.
