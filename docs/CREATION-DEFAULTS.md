# Creation defaults

## Notes

In **Settings → New note defaults**, set size, speed, visible time, Y offset,
alpha, below/fake, optional judge area and hit-effect color, hitsound and volume.
**Use selected note as defaults** is also available in Note Properties. It copies
style and the original sound source, without copying the note's time, position,
type or baked volume file. **Reset note defaults** is independent of line reset.

Manual Tap/Hold/Flick/Drag placement and note-pattern generation use this style.
The audio-marker generator starts with **Use new-note defaults** enabled; turn it
off to use its own side/size/speed/alpha/fake/Y-offset controls. Older saved
generator settings retain their explicit style. Text generation keeps its
required stationary fake-note geometry, size, timing and side; it inherits alpha,
optional extensions and sound. Existing/imported notes, duplication and pasted
objects retain their source values.

These preferences persist across editor sessions and do not change chart history.
Custom sounds retain their project scope; another project falls back to the
note-type cue. Lower volume uses the existing native sound decoder and bounded
WAV assets, prepared once per sound/style on the file worker. The first placement
waits for preparation; subsequent placements reuse it. Applying a placement or
generated batch creates one Undo entry. Pending work rejects changed documents,
assets or preferences. Export includes the ordinary custom WAV and original
source metadata; native Phira playback needs no new audio engine.

The `note-defaults` UI probe exercises real Miniquad touch, numeric swipes,
Reset, all seven locales, Tap/Hold gestures, the file worker and original prpr
loading, followed by exact Undo/Redo. `tools/note-defaults-smoke.py` opens the
ordinary application twice with private managed storage, places and saves a
note, checks all style properties and its gain asset, and verifies persisted
preferences and unchanged imported input.

## Lines

In **Settings → New line defaults**, set the initial Move X, Move Y, Rotate,
Alpha and Speed values for newly authored judgment lines. These preferences are
stored with the editor settings and persist across projects and application
sessions. They also apply to the first line when creating a new project.

Adding one line or a batch uses constant initial events on layer 0, from beat 0
to beat 1, with the selected values continuing afterward under native playback.
One Undo removes the whole creation operation; Redo restores its original values
even if the preferences have changed since then.

**Reset defaults** restores X = 0, Y = 0, rotation = 0, alpha = 255 and speed = 10.
Changing or resetting these preferences does not edit the current chart or add
an editor Undo entry. Existing/imported lines, duplicated lines and explicitly
pasted chart data retain their authored values.

Tools that generate their own motion retain that motion. In particular, Note
Tracing applies the new line's alpha and speed defaults and compensates the
initial X/Y/rotation events when tracing into an additional layer. Changing line
defaults invalidates a prepared trace only when its destination is a new line.

The native UI probe exercises Settings through the actual Miniquad touch handler
and runtime modal-input preflight:

```sh
cargo build -p phistudio-app --example ui-layout-probe --locked
target/debug/examples/ui-layout-probe artifacts/checks/creation-defaults creation-defaults 873x393
```

It swipes a numeric field, switches through all bundled locales, activates Reset
with touch, verifies that the chart/history remain intact, then creates and
undoes a batch using customized defaults. A 990×2145 window with
`PHISTUDIO_TEST_DISPLAY_DENSITY=2.75` exercises a 360×780 logical phone viewport.
