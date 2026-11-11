# Custom easing implementation record

Status: in progress. The separate graph workspace is not yet connected to the
app. This record does not mark the custom easing requirement complete.

## Implemented data and playback integration

`phistudio-core::custom_easing` owns normalized multi-node cubic geometry,
locked endpoints, automatic/smooth/corner/linear/hold modes, shape-preserving
insertion, node/handle edits and validation. Curves support 2–256 nodes. Named
libraries serialize as `phistudio-animation-curves` version 1 and accept
`rpen-animation-curves` version 1 template files. Input/output size is bounded
at 8 MiB and 512 templates; merging resolves name collisions without overwriting
saved work. Persistent library UI/file-worker integration is still pending.

`LineEvent.custom_easing` stores an authoritative boxed `Binding` under
`phiStudioCustomEasing`. The binding retains the full curve, source domain and
optional affine source endpoints. A curve stays one logical event on the grid
and in saved editor data. X/Y timing links, properties, clipboard/clone offsets,
ordinary time changes and BPM edits retain that event and its curve. Cropping
keeps the original domain/source mapping, including loops with equal visible
endpoints. Endpoint edits rebase that mapping; incompatible unequal values on a
flat source domain produce an atomic validation error. Reversed domains use
native right-continuous jump semantics; reversing twice restores the original
binding. Standard easing replacement explicitly removes the custom binding.
Join checks the actual active curve geometry before merging intervals.

`Snapshot::prepare` applies a fresh curve over an event's visible endpoints.
`prepare_preserving_domains` edits existing root curves while retaining cropped
source mappings, recalculating their visible endpoint values. Prepared edits
hold focused source records, reject changed revisions/inputs and apply as one
Undo transaction. Their native preview fragments are separate from authored
records. The caller must also guard project session and request ownership.

Only renderer/export copies expand curves into ordinary linear RPE intervals.
The control hull bounds geometric approximation by 0.001 event-value units
before native floating-point/time rounding. This avoids aliasing in the original
parser's quantized Bezier cache and retains excursions between equal values.
Hold boundaries use instantaneous native value keys in the projection; logical
authoring events keep positive duration. Their endpoint times do not extend
prpr's terminal clock or change unrelated line playback.

Package export also projects secondary RPE chart resources. Opaque JSON resource
bytes and source files remain unchanged. Exported groups carry complete source
metadata and a fingerprint; import reconstructs a logical curve only when every
native fragment and its BPM context still match. Edited/invalid hints retain
their ordinary native data. Sparse imports never acquire missing/null fields.

`phistudio-prpr::editable_easing` retains legacy Bezier handles or samples the
original native easing/window tween. Playback remains the original prpr parser
and animations. Authoring displays can show actual cropped-loop values through
`EventCurve::value`. The note-distance graph parses the same projected speed
data as ordinary playback, retaining its native terminal clock. Explicit custom
speed invalidates an older PEC height override; Undo restores that override.
Unused legacy Bezier controls do not enter the native speed parser's cache.

The existing inspector identifies custom easing and allows an explicit return
to standard easing, with labels in all seven locales. The standalone graph
editor, creation controls and template manager are not yet available.

## Validation

The logical-binding checkpoint passes 846 workspace tests with zero failures
and 26 explicitly ignored cases, workspace Clippy with warnings denied for
project code, explicit-package formatting and the desktop application build.
Locale keys/placeholders agree across all seven catalogs. Original prpr, note
skin and preview-resource provenance checks pass.

Focused cases cover geometry editing, library formats, logical-event persistence,
X/Y links, cropped/reversed mappings, exact Undo/Redo, cancellation/stale results,
portable package export/reimport, secondary charts, sparse imports, PEC ownership,
standard easing replacement and safe Join behavior. Native parser checks compare
1,001 times per curve for X/Y/rotation/alpha through BPM changes. Speed tests
exercise both playback choices and RPE 140/170 within the native judgment clock;
extra cases verify clock preservation, reversed Hold boundaries and agreement
between the distance graph and normal playback. Idealized integration beyond
prpr's actual terminal clock is not asserted as native behavior.

`tools/custom-easing-data-smoke.py` exercises the ordinary application with
private managed storage: selecting one logical event, editing its endpoint,
Save, exact header Undo/Redo, explicit standard-easing replacement and restoration,
and preservation of curve metadata, unrelated fields and the external input.
It does not exercise a new graph window or claim physical-device verification.

## Remaining integration gates

- Build the separate responsive workspace and replace the simple Bezier workflow:
  point/handle selection and dragging, segment modes, precision fields,
  pan/pinch/fit, local history, preview and coherent chart application.
- Connect cancellation/coalesced workers with session/revision/request guards.
  Avoid large compilation work on the UI thread or on every frame; cache native
  results with all source dependencies. Keep original native clock semantics.
- Connect persistent named templates, thumbnails, save/replace/delete and platform
  import/export through existing workers/pickers. Keep the library separate from
  chart history and ordinary preference updates.
- Integrate applicable Storyboard/Noise Field and other curve-owner contexts.
  Retain their explicit scalar/vector/color/text types and ownership.
- Verify actual Miniquad/Gui touch through runtime modal-input preflight at phone
  density, both orientations and all locales. Verify complete workspace Save,
  Undo/Redo and native/export playback before marking the feature complete.
  Physical Android testing remains separate from simulated input checks.

The behavior reference remains the pinned rpe-next tree in
`EDITOR-UPGRADE-WORK.md`. No upstream implementation or assets are copied.
