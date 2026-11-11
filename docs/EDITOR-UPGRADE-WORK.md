# Editor upgrade implementation and acceptance record

The complete requested scope is tracked here across working sessions. The
baseline is `40f9e73`; reference behavior is rpe-next main, tree
`811699d87c3a3cc25be546da4be8f56c7ceb5bb4`. Implementations remain project-owned;
upstream source is studied as a behavior reference, not copied or bundled.

## Requirements

- [x] Speed graph: remove the entire persistent bottom controls panel; provide
  in-graph touch navigation and contextual point editing, distance pinch zoom,
  horizontal panning, draggable white playhead preserving playback intent,
  adjustable range, reset/fit/snapping/easing/history access, larger vertical
  plotting area and a top-bar loop toggle that plays only the chosen range.
- [ ] Custom easing workspace replacing the simple Bezier controls: editable
  multi-point curves/handles, segment modes, reusable named templates and
  import/export, correct native preview/storage and coherent Undo/Redo.
- [x] Note Tracing in the Motion Generator tool menu (user clarification):
  inspired by rpe-next, sample the selected note's native
  trajectory and generate judge-line motion/optional rotation on a chosen
  destination line/layer or a new line, with preview, range/density controls,
  conflict handling and one reversible authoring operation.
- [x] Playback rates: expose and support 0.25x and 1.5x through both compact and
  full menus and actual music/clock transport.
- [x] Sidebar hitsound audition: selected line or selected groups of lines,
  without changing visual line visibility or judgment, including in playtest.
- [x] New-note defaults: all applicable Note Properties (including size, speed,
  fake, above and extensions), editing once and applying consistently to new
  authoring paths, without altering imports, existing notes or pasted objects.
- [x] New-line defaults: initial X/Y/Rotation/Alpha/Speed events, persistent
  settings and an independent Reset Defaults button, including batch creation.
- [x] Projects: accumulated EditTime persists between sessions; local rows show
  EditTime, event count, then note count, with accurate cached summaries.
- [x] Projects: remove the version/build row identified in the attachment;
  retain version information in About.
- [x] Settings: application-wide fullscreen on Android, including use of the
  display-cutout area, safe keyboard/system-inset handling and persistence.
- [ ] LAST: advanced audio guidance and stem separation, with real optional ML
  model support, lightweight/heavier choices and disable controls, source/stem
  audition and waveform/onset guidance, bounded background jobs, cancellation,
  model lifecycle/download validation and useful mobile UX.

## Acceptance requirements

Every item requires implementation and requirement-specific evidence before it
can be checked above. No subset completes the user goal. Stem separation and ML
audio guidance must be implemented last. Preserve raw chart extensions,
selection/source ownership, cancellation and one Undo operation per gesture.
Use existing native prpr evaluation/rendering and the shared egui context.

For input changes, exercise the production runtime modal-input preflight and
the actual Gui Miniquad handler; the previous bug escaped tests that only
called Workbench UI. Verify playback/scrubbing while playing, multitouch
promotion/cancel, modal transitions, narrow/dense windows and actual main-loop
Save/Undo/Redo. Android physical-device checks remain distinct from simulation.

Formatting uses explicit workspace packages. Run appropriate crate tests,
Clippy, relevant GPU/application probes, and platform/provenance checks for
changed integrations. Commit/push authorization as BigCamper68 persists from
the conversation; do not publish a release or claim device verification merely
because automated checks pass.

## Evidence and current work

Implemented in the first delivery slice:

- Distance graph: no persistent bottom panel. Empty-space dragging pans and
  expands the native sampled range in the background. Two-finger gestures scale
  distance and horizontal time; releasing one finger cannot create/edit a point.
  Dragging the white playhead seeks without clearing playback intent. Bottom
  handles choose playback bounds independently of the sampled editing range;
  the top loop toggle repeats them or stops exactly at their end. Long press or
  right click opens contextual precision/snapping/easing controls. Android Back
  and Escape dismiss that context before closing the graph. Compact controls
  leave more vertical plotting space on phones.
- Shared transport/menu rates include 0.25x and 1.5x. Sidebar music-note popup
  auditions all lines, the current line or selected groups. Native audio cues
  are masked/restored without changing note geometry, visibility or judgment;
  the source document and movie exports retain all original sounds.
- Projects no longer show the marked version/build row; About retains it.
  Fullscreen persists in editor preferences, updates the actual window, uses
  Android cutout space and separates safe system padding from cutout padding.

Validation: the initial workspace pass contained 777 passing tests and 26
explicitly ignored tests. The complete app pass after the graph changes had
375 passing tests and 22 ignored tests. The subsequent targeted graph pass has
17 passing tests, including actual runtime modal-input preflight, routed phone
pinch/pan/scrub/range cancellation, pending worker/source guards, precise
Undo/Redo, long press/Back and bounded graph/context layouts for all seven
languages down to 582x262 and portrait 360x780 logical pixels. Native audio-mask
and five-rate transport tests passed. The Android inset-policy JVM test covers
seven cases, and all Java sources compiled against SDK 35. The prpr provenance
check passed for 186 upstream files with the documented input hook.

Actual main-loop Save/Add/drag/Undo/Redo smoke passed. A GPU probe entering
Gui's Miniquad touch handler also passed at density 2.75, framebuffer 990x2145:
point insertion, pinch, seeking while playing and exact Undo/Redo. Artifacts
live under `artifacts/checks/speed-curve-runtime` and
`artifacts/checks/speed-curve-gestures-phone`. The compact-toolbar GPU probe also passed at 873x393; the actual main-loop
smoke passed after that UI change. Clippy with `-D warnings` and the desktop
application/probe build passed. Additional-finger freeze coverage passed in
the final targeted graph test run.
Physical Android hardware and a fresh APK were not tested in this environment;
these must not be described as verified device behavior.

The Projects delivery adds cached standard/Storyboard event totals and accumulated
EditTime. Records from earlier builds acquire event counts on the library worker.
The current chart uses revision-cached counts including unsaved edits. Time uses
monotonic editor intervals, excludes Projects home and suspended gaps, writes
periodically in a worker and flushes at lifecycle boundaries. A private activity
record retains time through chart Save, record refresh and Trash/Undo. Per-session
absolute checkpoints and file locking make retries idempotent.

This delivery passed 786 workspace tests with 26 explicitly ignored tests, the
all-target Clippy check and the desktop application/probe build. Layout tests
verify time, events and notes in that order at 320/360/873 logical pixels across
all seven languages. A main-loop smoke opens and gracefully closes the editor
twice at density 2.75 and framebuffer 990x2145. Its two sessions accumulated
1619 ms and 3228 ms total, Projects home retained the previous total and chart
bytes stayed identical. Evidence is in `artifacts/checks/project-activity-runtime`
and the reusable runner is `tools/project-activity-smoke.py`.

At that checkpoint, custom easing, Note Tracing, creation defaults and advanced
stem/ML audio guidance remained. This delivery slice did not complete the active
user goal. The current reference cache also
includes rpe-next's animation-curve editor/library, note-tracking and placement
models; no upstream implementation or assets are copied into this repository.

Implemented in the Note Tracing delivery slice:

- Tools → Motion generator now contains Trajectory and Note Tracing modes.
  Tracing pins a selected source note, previews destination-local and screen
  paths, and generates ordinary X/Y/optional rotation events on a new or existing
  line/layer. It exposes beat bounds, 60/120/240 Hz, simplification tolerances,
  explicit overlap replacement, cancellation and one Undo/Redo transaction.
- Native note poses share prpr's existing inspection geometry and restore all
  affected animation cursors, control curves and resource time after each batch.
  Sampling accounts for parent transforms, other destination layers, BPM/native
  time boundaries, below notes, incline and distance controls. Distance-control
  crossings are refined incrementally through native animation queries. Hold
  heads remain pinned after judgment, matching the original renderer.
- Worker snapshots copy only overlapping destination events/recipes. Native
  sampling is incremental; worker results reject changed projects, revisions,
  assets, aspect/geometry settings and parameters. Outside fragments retain
  their native easing and unknown fields; unrelated recipes and source notes
  remain intact. See `NOTE-TRACING.md` for usage and boundary/accuracy details.

Note Tracing validation:

- Workspace tests: 802 passed, zero failed, 26 ignored. Ten focused core cases
  cover sparse layers, full rotations, boundary jumps/continuations, BPM changes,
  negative native chart seconds, conflicts, cancellation, stale edits, cropped
  raw extensions, recipe detachment and exact Undo/Redo. Six app cases cover
  source/request ownership, worker cancellation, stale application and distance
  crossing schedules. Workspace all-target Clippy and the app/probe builds pass.
- Actual GPU/Miniquad touch probes pass at 873×393 and density 2.75 at 990×2145
  (360×780 logical). Both activate mode/preview/apply through Gui and the runtime
  modal-input preflight. Cases include a below-line note with incline/controls,
  a parented multilayer destination and a scaled hold traced into layer 3; all
  seven locales render prepared results. Native poses are compared at 501 times
  per case: maximum observed authored-coordinate error is 0.198 position units
  and 0.000306 degrees. This includes between-sample approximation and native
  float precision; the chosen tolerance applies to the sampled points.
- `tools/note-tracing-smoke.py` passes against the ordinary application main loop
  at 1280×800: preview leaves JSON unchanged, Apply creates a line without
  changing the source, Save writes the chart, one header Undo restores original
  JSON and Redo restores generated JSON. This test uses a private direct chart;
  managed workspace imports edit their own copied/working chart instead.
- `tools/verify-prpr-source.py`: 186 upstream files verified; no vendor/phira
  source changes. Physical Android/APK verification has not been performed.

After Note Tracing, custom easing, creation defaults and, last, advanced stem/ML
audio guidance remained. This delivery did not complete the overall goal.

Implemented in the new-line defaults delivery slice:

- Settings → New line defaults exposes initial X/Y/rotation/alpha/speed with an
  independent Reset defaults button. Values persist in editor preferences;
  changing/resetting them never edits the current chart or adds history.
- Ordinary single/batch line creation and the first line of a newly created
  project use the same validated template. Existing imports, duplication and
  explicit pasted/generated geometry retain their source/required values.
  Stock templates retain their existing JSON structure and values.
- Note Tracing captures the intended new-line style, uses its alpha/speed and
  subtracts initial motion on other layers. Changing defaults invalidates only
  prepared traces targeting a new line. The native scaled-hold/layer-3 probe
  now uses nonzero X/Y/rotation and customized alpha/speed.

New-line defaults validation:

- Workspace tests: 810 passed, zero failed, 26 ignored. Six focused core cases
  cover single/batch creation, source-copy preservation, stock compatibility,
  atomic invalid-input rejection, preference migration/persistence/normalization,
  new-project initial-line persistence and tracing compensation in layers 0/3.
  App cases cover preferences across sessions, native command routing and
  tracing-cache ownership. Workspace all-target Clippy and app/probe builds pass.
- Actual Miniquad/Gui touch probes pass at 873×393 and at 990×2145 with density
  2.75. They swipe a numeric field, render all seven locales, activate Reset,
  preserve chart/history, and create/undo a batch using custom defaults. Header
  state stays open across language changes through a stable untranslated ID.
- The native Note Tracing geometry probe still passes its three cases, including
  the customized initial events in layer 0 underneath the hold trace in layer 3.
- `tools/creation-defaults-smoke.py` passes two normal application launches with
  persistent managed storage. Each launch adds a line via the sidebar, verifies
  all five saved tracks against settings, and checks exact header Undo/Redo.
  The script reads the authoritative active copied chart from last-project.json;
  external imported input bytes remain untouched. Clean shutdown retains the
  preferences for the next session. Physical Android validation remains pending.

Implemented in the new-note defaults delivery slice:

- Settings → New note defaults and Note Properties → Use selected note as
  defaults persist size, speed, fake/below, visibility, Y offset, alpha, optional
  judge area/color, hitsound and volume. Note reset and line reset are independent;
  preference edits do not add chart history. Custom sound defaults retain stable
  project ownership and fall back to type cues in another project.
- Manual placement and pattern generation use the same template and display it
  in their ghosts. Audio generation exposes an explicit common-defaults/own-style
  choice; legacy generator preferences retain their own style. Text generation
  keeps required fake-note geometry and inherits applicable style/extensions.
  Imports, duplication, pasted objects and existing notes remain unchanged.
- Native sound preparation runs on the file worker, caches three note-type cues
  or one shared custom cue, and uses ordinary project WAVs understood by Phira.
  Project/revision/asset/preference guards and cancellation protect pending edits.
  Placement/batches each have one Undo. Audio-generator previews use the original
  native SFX backend with the chosen source and volume, without gain-file writes.

New-note defaults validation:

- Workspace: 824 passed, zero failed, 26 explicitly ignored. Focused cases cover
  all four note kinds, generator policies, extension/unknown JSON preservation,
  exact Undo, preference migration/normalization/persistence, sound ownership,
  native WAV preparation/cache, cancellation, stale revisions/preferences and
  old-project reply collisions. Original PCM cues match staged preview cues;
  export/reimport reaches native prpr with judge-area/color/gain intact.
- Actual Miniquad touch probes pass at 873×393 and at density 2.75 with framebuffer
  990×2145: numeric swipe, independent Reset, seven locales, Tap/Hold gestures,
  real file worker and native prpr loading, exact one-step Undo and Redo. Direct
  framebuffer readback avoids a GLES texture-copy capture issue; screenshots were
  inspected. Evidence: `artifacts/checks/note-defaults-222` and `note-defaults-223`.
- `tools/note-defaults-smoke.py` passes two ordinary application sessions with
  private managed storage: real placement, Save, all note fields and gain asset,
  exact header Undo/Redo, retained preferences and unchanged external source.
  Evidence: `artifacts/checks/note-defaults-runtime`. Workspace Clippy and desktop
  app/probe builds pass; original prpr/skin/preview-resource provenance passes.
  The native audio-generator GPU probe also passes: eight staged cues through the
  original SFX backend, visible grid ghosts, native loading, exact Undo/Redo,
  idempotent repeated generation and reused FFT (`audio-notes-225`).
  Physical Android/APK verification remains pending.

Remaining work is the full custom easing workspace and, last, advanced stem/ML
audio guidance. The overall user goal remains active.

Custom easing foundation is now implemented and covered by eight focused core
cases and three original-prpr integration cases. This is not the finished
workspace: authoritative logical event storage, renderer/export projection,
property/tool integration and the actual touch UI remain. In particular, native
compiled fragments must not become the editor's primary event model. See
`CUSTOM-EASING-WORK.md` for current evidence and the remaining integration gates.
