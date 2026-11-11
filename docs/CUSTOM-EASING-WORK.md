# Custom easing implementation record

Status: in progress. The graph workspace is not yet connected to the app.
This record does not mark the custom easing requirement complete.

## Implemented foundation

`phistudio-core::custom_easing` defines normalized multi-node cubic geometry,
locked endpoints, automatic/smooth/corner/linear/hold modes, shape-preserving
insertion, handle and group-node edits, deletion and validation. The model
supports 2–256 nodes. Curve/template names, coordinates, template counts and
library input/output sizes are bounded. Named libraries serialize as
`phistudio-animation-curves` version 1 and accept rpe-next's
`rpen-animation-curves` version 1 template files. Import merges preserve saved
entries and resolve name collisions instead of overwriting them.

The portable compilation prototype captures focused event snapshots, prepares
ordinary RPE linear intervals and retains the complete curve/source in a fingerprinted
authoring hint. Its control-hull subdivision bounds geometric approximation by
0.001 event-value units before native floating-point/time rounding. This avoids
the upstream parser's quantized Bezier cache keys aliasing different handles and
also represents excursions between equal endpoint values. A final Hold needs a
right-hand constant continuation; an untouched event at that boundary retains
priority. Cancellation, stale revisions/inputs, output limits, exact Undo/Redo
and untrusted/stale hints have focused coverage. Invalid hints never nest into
new recipes.

`phistudio-prpr::editable_easing` seeds existing Bezier handles directly and
samples the original native tween for standard easings and easing windows. It
does not introduce another easing table or playback evaluator. Native parser
tests cover 1,001 times per curve for X/Y/rotation/alpha through BPM changes,
including Hold and excursions between equal endpoint values. Speed-height tests
exercise both playback options and RPE versions 140/170 within the native
judgment clock. That clock remains determined by notes/non-speed events;
idealized integration outside it is not a valid assertion about native playback.

## Required final integration

Do not use the prototype's expanded native intervals as the primary editor UI
model. Doing so would expose hundreds of implementation fragments on the grid,
break logical X/Y timing links and lose curve editability after ordinary time or
value edits. The final editor must retain one authoritative logical event with
its custom curve, and expand only its renderer/export projection into ordinary
RPE intervals. Existing tools, properties, duplication, movement, BPM changes and
Undo must retain that curve rather than treating it only as a disposable hint.
The current prototype can supply the portable export bridge and validated
reimport hints; its transaction API is not the final workspace integration.

Remaining implementation/acceptance work:

- Add the authoritative event binding, including cropped source domains and
  endpoint rebasing. Route slicing, reversing and standard-easing replacement
  through that binding; preserve the existing native evaluator and raw extensions.
- Add renderer/export expansion and verified reimport reconstruction. Avoid
  compiling large curves on the UI thread or on every frame. Preserve native
  clock semantics rather than adding unrelated chart events to extend its clock.
- Build the separate responsive custom easing workspace and replace the simple
  Bezier field workflow. Support point/handle selection and dragging, segment
  modes, precision fields, pan/pinch/fit, local history, preview and coherent
  chart application with stale-source/cancellation guards.
- Connect a persistent named library, thumbnails, save/replace/delete and
  platform import/export through existing workers/pickers. Keep the library
  separate from chart history and avoid loading a multi-megabyte library in
  ordinary preference updates.
- Integrate existing Bezier-capable event contexts, including normal events and
  applicable Storyboard/Noise Field properties. Preserve explicit value types
  and ownership rather than coercing vector/color/text data into scalar values.
- Verify real Miniquad/Gui touch through runtime modal-input preflight at phone
  density, both orientations and all locales. Verify main-loop Save/Undo/Redo,
  project roundtrips and exported native playback before marking the feature
  complete. Physical Android testing remains separate from these probes.

The behavior reference remains the pinned rpe-next tree in
`EDITOR-UPGRADE-WORK.md`. No upstream implementation or assets are copied.
