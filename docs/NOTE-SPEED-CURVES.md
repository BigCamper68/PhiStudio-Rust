# Graphical note speed curves

Select one note and open **Note properties → Note speed curve…**. The window
shows chart beats horizontally and the note's distance from its judge line
vertically. A dashed line marks judgment; a solid cursor follows playback.
The panel follows a new single-note selection, including notes on another line.
Multiple selection leaves the graph inactive. The editor remains usable outside
the window while no curve gesture is pending.

Speed events belong to the judge line, so editing this graph affects **all notes
on that line**, not just the note used to display distance. Note properties and
note arrays are unchanged by a curve operation.

## Editing

- **Edit point** selects and drags existing nodes. Horizontal movement follows
  the editor's beat division. Optional distance snapping has its own step.
- **Add point** clicks a new node into the curve. Clicking near the existing
  curve preserves its shape; clicking away requests the chosen distance.
- **Delete point**, the selected-point button or Delete/Backspace removes an
  editable node and joins its neighbors with a linear distance segment.
- Selected-point fields set beat, distance and the outgoing segment's easing.
  All 29 editor easings are available. **Original curve** retains the sampled
  native shape. Easing describes **distance**, rather than interpolating speed.
- The selected point also displays total line speed and the selected layer's
  speed, derived from the native height animation's tween derivatives.
- **Fit graph**, distance range, horizontal pan, zoom buttons and work-range
  fields control the view. Wheel scroll pans; Ctrl/Cmd + wheel zooms horizontally.
- Play/Pause and a moving timeline cursor allow checking motion. Opening the
  panel and editing a point pause playback. Undo/Redo are available in the panel;
  Ctrl/Cmd+Z, Shift+Ctrl/Cmd+Z, Space and Ctrl/Cmd+S keep their familiar meanings.

A drag or numeric edit stages its curve on a worker and commits once when editing
ends. Add, delete and easing changes each create one Undo action. Touch cancel,
lost focus, Escape during an edit, Back or closing the window discard pending
work. Back/Escape then closes the window. A late worker result cannot overwrite
a different note, project, chart revision, layer or request.

Range endpoints and the judgment-distance anchor stay fixed. Hold heads remain
fixed after their start. Source speed layers are summed for display; only the
chosen layer (0–3) is written. The initial range expands to imported nonlinear
speed-event boundaries where necessary. A manually chosen range must not cut
such an event; adjust its endpoints if the tool reports this condition.

## Playback and storage

Distance sampling uses upstream prpr height animations and the existing Y-control
sampler. The runtime supplies the exact global terminal clock from the currently
parsed preview, so changing the graph view cannot change its playback model.
Both project speed-compatibility modes are supported. Media, other lines' notes
and scene rendering are not loaded by the graph worker.

Distance edits are fitted to ordinary RPE `speedEvents`: linear speed intervals
in the modern mode, constant cells in the legacy mode to retain its integration
behavior. BPM changes,
other speed layers, note speed, above/below, Y offset and Y Control participate
in the inverse calculation. The generated result is parsed again by prpr and
checked against the requested nodes and curve before the document can change.
Range-edge distances, the judgment anchor and speed continuation outside the
work range are preserved. Imported unknown fields are retained in their native
speed-event objects; unrelated line properties, notes and tracks are untouched.

Logical points and easing hints live in `phiStudioSpeedCurves`, grouped by line,
layer and work region. They survive Save, package export and reopening. Standard
RPE players use the ordinary speed events and can ignore these editor hints.
Signatures invalidate stale hints after external changes. Independent work
regions keep their logical points; editing one region does not expose another
region's internal fitted fragments.

The worker processes one request at a time and coalesces intermediate drag
requests. Limits are 20,000 source speed events, 256 logical points in a work
range and 1,024 fitted segments per edit. Unlinked, non-overlapping intervals are
required within the edited range. Zero note speed, non-invertible Y Control,
invalid native playback or excessive fitting complexity produce an inline error
without a partial chart edit. Charts carrying a PEC height override are not
opened for speed-curve authoring; the tool requires native speed events.

## Reference and validation

Interaction behavior was studied in
[rpe-next's speed-curve panel](https://github.com/kclg-YSJ/rpe-next/blob/main/src/ui/speed-curve.mjs)
and its model. This implementation uses PhiStudio's own Rust/egui interface,
transaction system and existing prpr integration. No upstream code or assets
are bundled.

```sh
cargo test -p phistudio-prpr --lib --locked speed_curve
cargo test -p phistudio-app --lib --locked speed_curve_ui
cargo build -p phistudio-app --example ui-layout-probe --locked
xvfb-run -a -s '-screen 0 873x393x24' target/debug/examples/ui-layout-probe \
  artifacts/checks/speed-curves speed-curves 873x393
```

Tests cover native playback parity, all 29 easings, BPM boundaries, multiple
layers, negative speed, Y Control, Hold pinning, range-edge preservation,
stale results, private metadata, region
persistence, actual canvas input and exact Undo/Redo. The GPU probe checks the
English and Russian window, a real add-point click, the native clock and chart
round-trips on the requested viewport. Touch/device latency and system keyboard
behavior still require physical-device checks.
