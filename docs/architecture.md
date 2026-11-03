# Editor architecture and performance

The workspace separates editable chart data (`phistudio-core`), application
interaction (`phistudio-app`), native Phira integration (`phistudio-prpr`) and
offline audio analysis (`phistudio-audio`). Platform bridges exchange requests
and paths with the application; chart edits stay in the core command/history
system. The renderer and UI share one GL context.

`phistudio-sdk` defines the platform-neutral extension protocol, native C ABI and Wasm guest
exports. `phistudio-plugins` owns dependency resolution, lifecycle, registration
ownership, package discovery, portable execution and the checked editing bridge.
The core's patch history retains reversible data rather than plugin callbacks.
Typed plugin-data operations expand into that same history. Their versioned
project entries survive unload; explicit export-copy policies filter chart/package
snapshots without changing Save, recovery, working documents or opaque resources.
Application integration is in progress; existing Tools remain built in. See
[`PLUGINS.md`](PLUGINS.md) for the full scope and remaining gates.

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
| `plugins::State` in app | Cached registry/panels, manager visibility and versioned pending calls |
| `Runtime` in plugins | Worker-owned Host, module loading, callbacks, settings/storage and private provider files |

The state types are transient UI state, not new serialized chart or preferences
formats. Feature UI still coordinates through `Workbench`; pure chart rules
belong in core and native rendering remains in prpr.

`TimelineState::cancel_gesture` clears both the draft and router ownership.
Call it where a workflow previously canceled both; clearing only a draft can
have a different purpose. `Windows::close_auxiliary` preserves Back priority.
`FileWorkflow` owns its modal checks and staged-open cancellation. Document
replacement continues to preserve application preferences, clipboards and
shared resources while resetting project-local drafts and caches.

Plugin code and package/state I/O run on one serialized worker. The render loop
polls results and uses the existing command/history boundary to apply edits.
Transient plugin post-processing uses original prpr Effect materials after chart
effects/HUD, with live uniform cells, versioned frame admission, bounded fair
compilation and per-pass failure diagnostics. It shares the existing ping-pong
target and graphics context; removed passes release their materials.
The worker's `host/assets` service snapshots package/project files and decodes
bounded images. The UI caches their textures in the existing egui adapter and
rechecks activation ownership and project resource versions before drawing.
Disabling/removing/reloading a plugin immediately revokes its queued results,
including those of dependants, before the worker reaches the stop request.
Registry metadata is copied only when completions arrive. Every edit rechecks
activation ownership and document `(session, revision)` at application time.
The runtime survives project replacement; old-project edits do not. Callback
panels are painted by egui with the editor's theme and touch wrappers. Versioned
service calls and replies share a bounded dispatch budget across application
round trips and retain caller ownership. Document queries copy at most 256 KiB
per response; at most two run in a frame. Native libraries open only on activation
and destroy their state before unloading. Render callbacks use versioned frame
tickets and a coalesced fair queue; the UI paints clipped cached primitives.
Frame jobs and their service continuations stay separate from user-action busy
state. Absolute-beat track geometry follows the live clock. Selected extension
tools receive coalesced pointer batches with frozen host coordinates. Gesture
scopes follow callbacks and service continuations; cancellation revokes pending
edits and one completed gesture can commit one history transaction. Thin adapters
connect the existing grid router, track windows and preview. Built-in Tools keep
their implementations.

Format/resource providers use revocable operation scopes and private input/output
files. Bounded chunks flow through a worker-side `host/files` service without
putting an entire file in a protocol message. Dispatch queues and unpublished
completions have independent count/byte limits. Scope ownership survives service
interception and asynchronous document queries. The app retains artifacts through
native chart/media validation, file pickers and the existing project-import or
resource-Undo workflows. Invalid/cancelled/stale work cannot replace the current
document. Export publication checks participating generations. A format bridge
selects canonical RPE or a native project ZIP. ProjectRead authorizes the latter
for encoding; scoped package methods page metadata and stream individual assets
through private handles. Created output entries can be renamed/removed and packed
on the worker. The existing project importer validates the result. Core archive
helpers retain native path/size rules and check cancellation between chunks;
private entry files retain paths rather than holding an open descriptor per entry.

Explicit native open/save/export actions have an ordered `host.file.before` hook.
Callbacks run on the plugin worker with metadata and a revocable invocation.
The application retains the actual native continuation and checks its document
version and participating generations before handoff. Back also cancels approved
work still queued for the file manager. Hooks can veto the action or rename a
suggested export, while the native parser, writer, working-copy/history logic
and archive bytes keep their existing implementations. Discard-and-open hooks
run before clearing recovery; automatic checkpoints remain independent.

`host.service.before` intercepts application and plugin service calls. Validated
replacements can route to another provider using the replacing hook's grant and
dependencies. Replies retain the original service/correlation address. Hook
participants follow all derived work; disabling one revokes its pending results.
Hook side effects share the dispatch budget but retain read-only permissions
and skip recursive interception across host round trips. Result payloads also
retain activation/gesture ownership when no effect is returned.

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
