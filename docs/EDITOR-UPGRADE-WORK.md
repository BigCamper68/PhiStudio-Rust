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
- [ ] Note Tracing in the Motion Generator tool menu (user clarification):
  inspired by rpe-next, sample the selected note's native
  trajectory and generate judge-line motion/optional rotation on a chosen
  destination line/layer or a new line, with preview, range/density controls,
  conflict handling and one reversible authoring operation.
- [x] Playback rates: expose and support 0.25x and 1.5x through both compact and
  full menus and actual music/clock transport.
- [x] Sidebar hitsound audition: selected line or selected groups of lines,
  without changing visual line visibility or judgment, including in playtest.
- [ ] New-note defaults: all applicable Note Properties (including size, speed,
  fake, above and extensions), editing once and applying consistently to new
  authoring paths, without altering imports, existing notes or pasted objects.
- [ ] New-line defaults: initial X/Y/Rotation/Alpha/Speed events, persistent
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

Still required: the full custom easing workspace, Note Tracing inside Motion
Generator, persistent new-note/new-line defaults, then advanced stem/ML audio guidance LAST. This delivery
slice does not complete the active user goal. The current reference cache also
includes rpe-next's animation-curve editor/library, note-tracking and placement
models; no upstream implementation or assets are copied into this repository.
