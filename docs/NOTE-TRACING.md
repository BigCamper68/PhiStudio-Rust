# Note Tracing

Select one note, open **Tools → Motion generator → Note Tracing**, and use
**Use selected note** when changing the source. The default interval starts up
to eight beats before judgment and ends at judgment, or at the end of a hold.
The source remains pinned while selecting a destination or inspecting events.

Choose a beat range, a new or existing judgment line, and event layer 0–3.
New destinations use the editor's line-creation defaults for alpha and speed;
tracing compensates initial motion on other layers.
**Track rotation** also generates rotation events. Select 60, 120 or 240 Hz;
higher rates capture fast motion more accurately. Position and rotation
tolerances under **Tracing precision** simplify the sampled trajectory, in RPE
coordinates and degrees respectively. Sampling density still limits accuracy
between samples.

**Preview tracing** calculates the path without modifying the chart. The small
plot shows destination-local movement; the editor preview overlays the source
path. **Apply tracing** creates ordinary X/Y/optional rotation events. Existing
overlapping events require **Replace events in range** and another preview.
Events crossing the interval are cropped with their original native easing and
extensions retained outside it. Untouched events retain their authored values
at the right boundary. A final discontinuity without an existing continuation
gets a constant event extending to the next event, or one beat beyond the range.

One Undo reverses the entire operation, including creation of a new line.
The original note and its custom fields stay intact. Editing the chart,
changing geometry settings, switching projects, or changing tracing parameters
invalidates a prepared result; preview again before applying it. Cancel or
close the tool to stop an unfinished calculation.

Tracing samples the original prpr animations, including parent transforms,
other destination layers, BPM changes, below-line notes, incline and distance
controls. A hold follows its head up to judgment and its pinned head afterward.
Time and distance-control boundaries retain both sides of a jump. Sampling is
incremental on the native graphics thread; scheduling, simplification and event
preparation run on workers. Jobs are limited to 60,000 samples and 20,000 output
events; shorten the interval, lower the sample rate or increase tolerances if a
limit is reached. Preview and tracing share prpr's interpretation of the chart,
including its existing format limitations.

Run the native geometry and real Miniquad touch probe after building:

```sh
cargo build -p phistudio-app --example ui-layout-probe --locked
target/debug/examples/ui-layout-probe artifacts/checks/note-tracing note-tracing 873x393
```

The probe compares original note poses with the generated line's native poses,
checks exact Undo/Redo, and activates mode/preview/apply through the actual touch
handler and runtime modal-input preflight. Use a software OpenGL display or a
normal desktop session. `PHISTUDIO_TEST_DISPLAY_DENSITY=2.75` with a 990×2145
window additionally exercises a 360×780 logical phone viewport; this does not
replace physical Android validation.
