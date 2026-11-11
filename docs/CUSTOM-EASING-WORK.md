# Custom easing implementation record

Status: in progress. The graph workspace and persistent template library are
connected to normal line-event inspectors. Additional owners and batch workflows
remain pending; this record does not mark the full custom easing requirement
complete.

## Implemented data and playback integration

`phistudio-core::custom_easing` owns normalized multi-node cubic geometry,
locked endpoints, automatic/smooth/corner/linear/hold modes, shape-preserving
insertion, node/handle edits and validation. Curves support 2–256 nodes. Named
libraries serialize as `phistudio-animation-curves` version 1 and accept
`rpen-animation-curves` version 1 template files. Input/output size is bounded
at 8 MiB and 512 templates; merging resolves name collisions without overwriting
saved work. Rpe-next imports normalize omitted template identifiers/names;
`.rpencurve` exports use `rpen-animation-curves` version 1 directly.
Automatic tangents retain finite geometry even for imported subnormal point
spacing; validation does not rely on an overflowing intermediate slope.

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

The normal event inspector opens **Edit custom easing…** for all five tracks.
Legacy Beziers seed editable node/handle geometry; the old four-number Bezier
workflow is replaced there. Unconverted events keep their original data.
The workspace offers shape-preserving point insertion, point/handle dragging,
multiple selection, node modes, precision fields, pan/pinch/fit, an animated
normalized preview and a local history bounded to 128 edits. Endpoints stay
locked; a completed drag creates one local Undo. Pinch promotion and system
touch cancellation restore any unfinished edit and retain navigation ownership
until all fingers leave the graph. The graph has no parent touch-scroll area.
Compact windows place the graph and precision fields on separate tabs; short
localized titles and shared Undo/Redo icons keep controls accessible at larger
interface scales.

**Use curve** validates/prepares the current draft on a worker and returns it to
the same inspector. The owner checks project session, document revision, event
identity and unchanged draft values/times before accepting a result. Opening,
canceling or using the graph does not edit the chart; **Apply event** publishes
the inspector draft as one normal chart action. Cropped/reversed source domains
remain attached when editing the root curve. The inspector also permits an
explicit return to standard easing. Workspace labels cover all seven locales.
Other owner contexts and batch workflows are still being integrated.

## Shared template library

**Templates** opens a searchable list with cached curve thumbnails and point
counts. Save current curve and Import templates remain visible; Template actions
groups rename, replacement, deletion and selected/all-library export. Replacement
and deletion require confirmation. The selected template's Use template button
stays outside the scroll area, including compact phone layouts.

Templates live in `custom-easing/templates.json` below the private workspace
root, independently of chart data, preferences and chart Undo/Redo. A bounded,
serialized worker owns parsing, thumbnail preparation and atomic writes. A failed
write/import leaves the previous file and published model unchanged. Writes also
check the previously loaded bytes so externally changed libraries require a
reload. The worker and queued writes survive project replacement; app lifecycle
checkpoints flush them before continuing. Pending confirmations are dismissed
when their source editor/project closes.

Loading a template copies its geometry into the curve draft as one local Undo
step and applies it over the event's complete visible interval. Existing cropped
source mappings remain intact when editing their root curve directly. Local
history retains this distinction even when the two curve geometries are equal.
Neither template management nor loading changes chart history; Use curve and
Apply event retain their inspector ownership rules.

Import/export use the existing platform file workflows. Mobile template exports
use the generic file bridge with the `.rpencurve` filename. Imported picker
snapshots are released after success or failure. The private library stays
available across charts and application launches.

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

The workspace checkpoint adds ten focused app cases. All 402 runnable app
library tests pass (22 explicitly ignored), as do the 15 custom easing core
integration cases and workspace Clippy. Actual Miniquad/Gui probes include
runtime modal-input preflight, insertion, node/handle dragging, native touch
cancellation, pinch promotion, local history, worker-to-inspector ownership,
native prpr loading and exact chart Undo/Redo. Both 873×393 and a 990×2145
framebuffer at density 2.75 pass in all seven locales. Equivalent 150% interface
scaling also passes using densities 1.5 and 4.125. A regression test covers both
graph and precision views at those compact logical dimensions. Screenshots were
inspected; evidence is in `artifacts/checks/custom-easing-248` through
`custom-easing-251`.
These are simulated platform inputs, not physical Android/APK verification.

`tools/custom-easing-workspace-smoke.py` also passes two ordinary application
sessions: the actual inspector entry, canceled and accepted graph drafts,
retained unsaved numeric properties, Save, exact header Undo/Redo, reimport of
the saved logical curve and unchanged external source. Evidence is in
`artifacts/checks/custom-easing-workspace-runtime`.

Template-library validation additionally covers optional import metadata,
selected/all-library portability, name collisions, stale replacements, failed
writes, external file changes, corrupt libraries, picker ownership, queued writes
through project replacement and fresh event values after loading a shared curve.
The Miniquad probe's `curve-templates` mode extends the real graph/touch checks
with name input, scrolling, selection, save, import/export and lifecycle flushing.
The ordinary two-session smoke also saves/reopens a template and verifies that
chart Undo/Redo never changes its private file.

At the template-library checkpoint, all 412 runnable app library tests pass
(22 explicitly ignored), together with 18 custom easing core cases and seven
native parser cases. Workspace all-target Clippy, the desktop app/probe build
and an `aarch64-linux-android` library check pass. The four density/viewport
probes in `artifacts/checks/curve-templates-261` through `curve-templates-264`
pass graph and template acceptance in all seven locales; compact library
screenshots were inspected. This includes touch scrolling, file workflows and
the fixed footer at 240×520 logical points. Logs are retained in
`artifacts/checks/template-library`. No APK or physical-device run is implied.

## Remaining integration gates

- Connect the workspace to existing batch easing/curve tools and applicable
  Storyboard/Noise Field and other curve-owner contexts. Retain their explicit
  scalar/vector/color/text types and ownership.
- Measure larger curves/projects before adding renderer caches; retain all
  source dependencies and original native clock semantics.
- Extend actual Miniquad/Gui touch and main-application acceptance to additional
  owners before marking the feature complete. Physical
  Android testing remains separate from simulated input checks.

The behavior reference remains the pinned rpe-next tree in
`EDITOR-UPGRADE-WORK.md`. No upstream implementation or assets are copied.
