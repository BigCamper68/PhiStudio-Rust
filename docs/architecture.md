# Editor architecture and performance

The workspace separates editable chart data (`phistudio-core`), application
interaction (`phistudio-app`), native Phira integration (`phistudio-prpr`) and
offline audio analysis (`phistudio-audio`). Platform bridges exchange requests
and paths with the application; chart edits stay in the core command/history
system. The renderer and UI share one GL context.

## State ownership

`Workbench` coordinates the editor and feature modules. New state belongs to its
owning subsystem rather than another unrelated field on `Workbench`.

| Owner | Responsibility |
| --- | --- |
| `Editor` in core | Source JSON, sparse commands, Undo/Redo and saved-state identity |
| `TimelineState` | Grid geometry, tools, input router and in-progress gestures |
| `TransportState` | Chart cursor, playback intent, seek dragging and volume |
| `Windows` | Auxiliary window visibility, sidebar and property-scroll intent |
| `FileWorkflow` | Picker requests, staged opens, recovery and project dialogs |
| `DocumentView` | Read-only note indices, line summaries and seek extent |
| Noise/Shader feature state | Format-specific selection and inspector drafts |
| `PickingState` in prpr | ID-buffer target, source mapping and visibility buffers |

The state types are transient UI state, not new serialized chart or preferences
formats. Feature UI still coordinates through `Workbench`; pure chart rules
belong in core and native rendering remains in prpr.

`TimelineState::cancel_gesture` clears both the draft and router ownership.
Call it where a workflow previously canceled both; clearing only a draft can
have a different purpose. `Windows::close_auxiliary` preserves Back priority.
`FileWorkflow` owns its modal checks and staged-open cancellation. Document
replacement continues to preserve application preferences, clipboards and
shared resources while resetting project-local drafts and caches.

## Read models and hot paths

`DocumentView` uses `(project session, editor revision)` as one invalidation key
for all its cached values. A cursor change is not a document edit. A new project
can reuse revision zero, so a revision-only cache key is insufficient. Keep the
existing public read methods on `Workbench` as delegates to this component.

`ChartDocument::iter_notes` lazily decodes typed notes from borrowed JSON in raw
source order. `notes` retains its collected API. Unknown JSON extensions remain
in the source tree; reads neither copy nor normalize them. Event reads borrow
when possible and use copy-on-write for missing import defaults and Speed
normalization. Do not remove those special import rules to make a reader faster.

Shader grids use a borrowed `Rows` view. Source rows and the selected inspector
draft are borrowed. A group drag allocates only transformed selected rows.
Creation adds one ghost row; it never extends or duplicates the underlying source
array. Hit testing also reads borrowed rows. Clipboard/history transactions
intentionally own their snapshots.

Picking retains visibility scratch buffers between passes and clears their
contents before decoding a new ID buffer. Replacing the parsed chart releases
the previous chart's visibility buffers and invalidates its candidates. The native renderer, masks, GL context
and note source-ID ordering are unchanged. Audio normalization selects its 95th
percentile rather than sorting an entire track; total float ordering and the
resulting noise floor are preserved, including duplicates and float edge cases.

## Validation and measurements

Run the workspace tests and Clippy with `--locked`; use the explicit package list
from `AGENTS.md` for formatting. Differential reader tests compare against the
former owned reader, including import defaults and errors. Lifecycle tests cover
cache invalidation, Undo/Redo, project replacement and window priority. Existing
gesture, recovery, localization, rendering and export tests remain required.

Opt-in debug-build measurements, with no timing assertions:

```sh
cargo test -p phistudio-core --test read_performance -- --ignored --nocapture
cargo test -p phistudio-app --lib shader_grid_performance -- --ignored --nocapture
cargo test -p phistudio-audio --lib percentile_performance -- --ignored --nocapture
```

The fixtures use 4,000 notes with unknown extensions, a Shader grid with 150
effects and 100 keys per effect, and a 150,000-frame percentile input. These
measure specific CPU paths, not whole-application FPS or physical-device latency.
Use `ui-layout-probe` for real editor/native-renderer galleries and compare the
same screen sizes, scales and scenes before and after a refactor. Native Android
builds remain necessary because desktop checks do not compile mobile-only paths.
