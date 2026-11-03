# Plugin-first implementation

## Objective and acceptance criteria

The existing editor features and Tools remain built in and are not migrated or
rewritten as plugins. This scope was explicitly clarified by the user after the
initial plugin-first proposal. The SDK adds extension points around the editor.
The target includes chart transactions, commands, panels and
inspectors, input tools, timeline tracks, preview overlays/effects, format and
resource providers, interceptors, replaceable services and inter-plugin APIs.
Installing a loader alone does not satisfy this objective.

The public protocol is independent of Rust's ABI, egui, the graphics context and
platform APIs. The application owns document/history invariants. An extension
never supplies callbacks needed to replay history. User extensions can be
disabled without deleting their project data. Native internals, when exposed,
are an explicitly version-coupled interface rather than part of the portable SDK.

## Delivery checklist

These are implementation gates. Checked entries retain the verification limits
below; they do not claim runtime testing on unavailable devices or operating systems.

- [x] SDK foundation: versioned manifest, dependencies/conflicts, capabilities, settings,
  lifecycle, commands, hooks, events, UI and drawing descriptions, services.
- [x] Host foundation: deterministic dependency resolution and hook ordering,
  registration ownership, unload/failure cleanup, bounded portable execution,
  discovery and loading. Native execution is explicitly unsandboxed.
- [x] Portable Wasm backend and authoring helpers, with an executable example.
- [x] Core transactions: atomic validation, revision checks, sparse independent
  Undo/Redo, preservation of unknown data and recovery/save compatibility.
- [x] Application integration: manager UI, installation/enable/disable, settings,
  commands/shortcuts, panels/inspectors and lifecycle notifications.
- [x] Input/timeline/preview extension points and service/provider replacement.
- [x] Format/resource providers, extension data persistence and export behavior.
- [x] Native backend on supported platforms with an explicit C ABI and platform
  constraints; no claim of sandboxing native code.
- [x] Exercise the public API with external tool, service, format and resource
  provider examples, without migrating existing editor features.
- [x] Developer documentation, templates, lifecycle/compatibility tests, workspace
  checks and available Android/iOS build and runtime verification.

## Delivery evidence

| Requirement | Evidence and scope |
| --- | --- |
| Portable SDK and compatibility | SDK manifest/protocol tests; thirteen independent SDK-only example packages; existing compiled modules still work with the extended host |
| Ownership, lifecycle and bounded execution | Host/runtime/install/Wasm/service/invocation tests, including revocation, dependencies, limits and read-only cascades |
| Source-preserving transactions and persistence | Core plugin transaction, project-data and archive tests; sparse Undo/Redo and recovery; application save/reload tests after module removal |
| Manager, commands, panels and public workflows | Fifteen explicitly executed compiled-Wasm egui integration tests and the native note-shift panel test |
| Input, drawing and preview | Actual mouse/touch/track/overlay tests; compiled stamp/guides/asset examples; OpenGL pixel probe for transient passes and native chart shader coexistence |
| Providers, assets and conversion | Bounded streaming/provider/package tests and external chart, resource and project examples, including large files, many entries, cancellation and export policy copies |
| Native actions and interception | Native file/action hook tests; external file-lab/action-lab controls; metadata-only status, scoped control permissions and existing native history handlers |
| Native ABI | Five Linux tests compile/load a C library; the compiled Rust native package passes its real panel test; host type-checks on Android, macOS, Windows and iOS (portable-only on iOS) |
| Workspace and provenance | 673 default tests passed, 25 ignored; strict workspace Clippy; explicit-package and external plugin format checks; seven locale catalogs; all four source/resource/icon verifiers |
| Available mobile build validation | Complete Android application check and strict app Clippy; actual debug APK builds and passes JNI/DEX/API 26, signature and 16 KiB ELF/ZIP checks with NDK r30 and SDK 35 |

Native execution has been run on Linux. Android APK validation is a package/link
check; `adb devices` has no connected device here. Windows/macOS native runtime
and physical Android/iOS interaction tests require those environments. The iOS
application requires macOS/Xcode; native plugins are unavailable there. These
limits are explicit and are not represented as successful device checks. CI is
configured to repeat the examples and local checks; it has not run remotely.

## Implementation sequence

1. Establish the protocol and document transaction boundary. Test stale work,
   rollback, imported extensions and history after plugin removal.
2. Implement the host, portable execution and ownership/lifecycle semantics.
3. Wire the real editor to the host and exercise external examples. Refine the
   API against the existing interaction and performance requirements.
4. Complete the deeper extension points, native support, packaging and examples.
   Preserve the implementation and behavior of existing editor features.
5. Audit every delivery gate against executable behavior and actual checks.

## Architectural decisions

- Bundled Rust plugins implement the same request/response protocol as external
  plugins. They can be statically linked; plugin-first does not require loading
  every bundled feature from a dynamic library.
- Portable modules execute in an interpreter. No JIT, OS imports or direct GL
  access are required by the protocol. Host integrations remain explicit.
- Object indices belong to a `(session, revision)` document version. Work from
  an earlier revision is rejected rather than applied to a different object.
- The host checks declared capabilities and registration ownership. These
  checks constrain portable host access; they do not sandbox bundled/native code.
- Hooks have documented cancellation and replacement semantics. Notifications
  cannot implicitly replace operations. Providers advertise namespaced services.
- UI and draw descriptions cross the portable boundary; egui and prpr remain
  the application's implementations. Heavy work must not run in a frame callback.

## Progress

Initial repository: `7ba2b6160c874b96016addef46e4e9b15469a6ae`.
Worktree: `feat/plugin-first`.

Implemented foundation:

- Separate SDK and host crates. Manifest validation, semantic dependency checks,
  conflicts/cycles, deterministic hooks, capability checks, owned contributions,
  settings, inter-plugin service dispatch and cleanup are covered by tests.
- Core JSON transactions stage edits before applying them, preserve sparse
  callback-free history and validate changed chart domains. Tests cover stale
  revisions/sessions, atomic rollback, exact missing/null restoration, unknown
  data, save/recovery and Undo after uninstalling a plugin.
- Wasmi 2.0.0 runs without OS imports or a JIT. Memory, module/message sizes and
  instruction budgets are bounded. Startup/notifications/hooks have a smaller
  budget than explicit actions. Failed instances require reloading.
- A standalone Rust guest exports the SDK ABI and exposes a note-shift command
  and panel description. `plugin-probe` loads it through the actual host, edits
  a chart, unloads the module, checks Undo/Redo and saves a separate output.
- The compiled guest test passed at 4,000 and 20,000 notes, preserving unknown
  fields. The larger input consumed about 3.54 billion interpreter fuel units;
  this informed the configurable action budget. It is substantial worker work,
  not a frame callback. No device-independent latency is claimed.
- `cargo test --workspace --locked` passed locally: 662 passed and 22 ignored.
  The compiled guest test was separately executed in release mode at both
  input sizes. `cargo check --workspace --locked` and strict workspace Clippy
  (`--all-targets --no-deps --locked -- -D warnings`) also passed. The vendored
  prpr dependency emits its existing deprecated `std::f32::INFINITY` warning;
  its sources were not changed. The host also
  passed `cargo check -p phistudio-plugins --target aarch64-linux-android --locked`.
  A CI workflow repeats the standalone guest and headless editor UI checks; it
  has not run remotely.

Application integration implemented:

- A serialized runtime worker owns the Host, module loading/execution and disk
  state. The frame loop polls replies and validates ownership/version again
  before applying an edit. Disable/remove/reload revoke queued and in-progress
  results immediately, including dependants. Shutdown clears published owners.
- The manager is available only from Projects home. It installs bounded
  directory/ZIP packages, enables/disables modules, reloads code,
  and retires removed packages without deleting personal
  or chart data. Corrupt preferences are reported and not silently overwritten.
- `Contribution::Menu` binds an owned window panel to File or Windows. Chart
  Linter registers File → Chart Linter. Legacy window panels and tracks have
  explicit Windows entries; legacy actions, converters and settings are exposed
  in expandable File sections labeled with the plugin name and More…. A click
  or touch tap expands the actions inside the scrollable menu; releasing the
  finger leaves them open. These are distinct from window-opening entries.
  Opening cached windows does not wait for unrelated worker commands. The loader
  has no plugin action or panel controls.
- Window content updates preserve explicit visibility for the current owner.
  Enable, Reload and new activations keep windows closed, including tracks;
  background updates cannot reopen a closed panel. Menu references must target
  a window registered by the same plugin.
- Contributed windows, sidebar panels and inspectors use egui, the common theme
  and touch wrappers. Command shortcuts share the existing key-routing path.
  Document open/change notifications are delivered without whole-chart copies.
- Workbench preserves its runtime across project replacement and rejects an
  old project's pending edits. Tests cover a real egui button, shortcut cleanup,
  stale results, history after disable, settings/storage across restart and
  temporary picker ownership while an installation is queued.
- The separately compiled Rust/Wasm example was installed, enabled and invoked
  through its actual egui panel in an explicitly executed headless integration
  test. The resulting chart was verified, then Undo/Redo was checked after
  disabling its module. This is additional to the default workspace suite.

Services and native execution implemented:

- `Call` effects route to independently versioned plugin services and deliver
  targeted `ServiceResult` callbacks. Consumers declare provider dependencies.
  Expected access/version errors do not fail the consumer. Provider effects
  retain caller generations; disable/reload invalidates both queued and
  in-progress results. A shared dispatch budget survives host-service round
  trips, and services cannot introduce reentrant edits from hooks/activation.
- Commands/panels default to metadata contexts; whole snapshots are explicit.
  `host/document` exposes versioned read, inspect, array slice and object key
  queries. Responses are checked against 256 KiB/4096-item limits before
  cloning. At most two queries run in a frame. Tests cover permissions, source
  preservation, invalid paths, stale sessions/revisions and project replacement.
- `fields` and `rows` project selected literal object keys before cloning;
  omitted and explicit-null values remain distinct. `validate` applies existing
  native validators to bounded note, event, Noise Field or shader-data objects.
- The standalone `note-statistics` guest reads pages and calls its own reusable
  histogram service. Its actual egui button successfully analyzed 4000 notes
  with unrelated chart data larger than the snapshot message limit. Release
  worker checks passed at 4000/20,000 notes (3/13 document queries, about
  1.6/8.7 seconds locally). These are local measurements, not device guarantees.
  Optimizing only the interpreter/core in development builds reduced the UI
  test's scan from over a minute to about 4.6 seconds.
- The native backend provides a C header and Rust export macro with versioned
  function table, opaque state and separate JSON buffers. Discovery/installation
  checks files without executing native code. Activation loads the library;
  disable destroys state before unload. Native code has full process access,
  without Wasm's execution or memory bounds. iOS native loading is unavailable.
- Five Linux tests compile an actual C library and check lazy execution, ABI
  mismatch, output ownership/size/errors, unload and Undo after disable. The
  Rust note-shift implementation exports both backends; its packaged native
  cdylib passed `plugin-probe` and its actual egui panel integration test. The
  host cross-checks including native code pass for Android, macOS and Windows;
  the portable-only iOS host also passes. Physical-device, Windows and macOS
  runtime verification remains outstanding.

Visual extension points implemented:

- Preview contributions paint clipped layers over the existing image. Track
  contributions open independent windows synchronized with the editor's clock,
  zoom and horizontal range. Existing timeline modes and Tools are unchanged.
  Panels/inspectors can embed static canvases using the same primitives.
- Render requests carry metadata, viewport geometry and unique frame tickets.
  Worker callbacks return one correlated drawing description, with points,
  normalized or absolute-beat coordinates and live theme tokens. Geometry,
  text and layer counts are validated before painting. Render callbacks and
  their service continuations cannot edit documents.
- Requests coalesce per visible instance, with fair oldest-first admission,
  bounded pending work and no new frame admission while user actions are busy.
  Frame jobs/continuations do not disable panel controls or shortcuts. Stale
  ownership, project/revision/line/layout and expired requests are rejected;
  disabling removes tracks before an in-progress callback can finish.
- Host tests cover ownership, tokens, geometry/canvas limits and indirect
  reentrant edits. Headless egui tests cover clipping/scaling, live clock
  projection, project changes, expired frames, immediate disable, Back, queue
  fairness and a real panel button while rendering is blocked.
- The separately built `editor-guides` Wasm example passed its actual egui
  preview/track/canvas and checkbox integration check. Changing theme colors
  updates cached drawings without reloading the plugin. This is a shape/input
  integration test; graphics-window/device validation is still outstanding.

Interactive tools implemented:

- Tool contributions declare surfaces, pointer buttons and tap/drag behavior.
  Users select them in the toolbar/editor menus; plugin actions may select their own
  tools. The existing built-in tools and grid gesture handlers remain in place.
  Adapters cover note/event/storyboard/shader/noise timelines, owned track
  windows and inline/fullscreen previews. Native Noise Field preview editing and
  the fullscreen return control retain their existing routing.
- Input batches contain pointer identity, modifiers/pressure and frozen local,
  normalized and host-snapped chart coordinates. Raw touch input is authoritative
  over synthesized mouse events. Tap swipes yield to existing navigation; a
  second finger cancels capture without promoting a remaining finger to a click.
- Callbacks execute on the worker, with one outstanding batch per gesture and
  coalesced moves. Begin/Update/Cancel and their service continuations cannot
  edit. End may produce one changed transaction through the same sparse history
  boundary. Host-owned gesture scopes survive asynchronous document queries and
  reject cancelled, revoked, stale or duplicate commits.
- Headless tests cover final release positions, held modifiers, multitouch,
  cancellation during/after execution, layout/clock/focus changes, track ownership,
  preview navigation/exit and switching back to built-in placement. A full queue
  does not advance a gesture's admission state.
- The standalone SDK-only `note-stamp` Wasm example was built and exercised
  through its actual toolbar, mouse/touch routing and asynchronous document query.
  Array, null and missing note containers retain exact source data through Undo
  after disabling the module. It adds a new external tool; no existing editor
  feature was moved into it.
- After input integration, the complete workspace suite and strict Clippy passed.
  All four compiled Wasm editor checks and the native panel check were separately
  executed and passed. The host cross-check passed for Android, macOS, Windows
  and iOS; asset/upstream provenance checks also passed. The CI workflow builds
  and exercises the new external tool alongside the existing examples.

Service interception and replacement implemented:

- `host.service.before` runs ordered hooks around application/plugin service
  calls. Its typed payload allows arguments, methods and provider replacement,
  while caller identity and correlation ID remain immutable. Cancel/failure
  returns an ordinary service error rather than failing the consumer.
- Cross-provider replacement uses the replacing hook's Services grant and
  declared dependencies, or its own service. The original consumer's dependency
  list does not need to anticipate override modules. Host document permissions
  still apply to the original consumer; replies retain the original address.
- Hook generations follow provider effects, targeted replies and asynchronous
  host queries. Callback-in-progress and queued results are revoked on disable.
  Hook side effects retain read-only permissions and skip recursive interception
  across service round trips. Result payloads also retain ownership without
  requiring an effect; Workbench rejects revoked completions before using them.
- Runtime tests cover order, replacement permissions/version checks, forged
  payloads, activation before registry publication, cancellation during hook or
  provider execution, asynchronous ownership and indirect edit rejection. Hook
  chains cap callback/effect counts and aggregate bytes. Shared bounded JSON
  sizing avoids allocating encoded request/response copies just to check limits.
- The standalone `service-lab` guest was built and exercised through its real
  panel button/checkbox. It replaces a virtual application service, cancels it
  and restores it without document read/edit grants or chart/history changes.
- A workspace run exposed allocation-dependent native buffer classification:
  an oversized claimed range could be mistaken for an input overlap. Length
  validation now releases an independent oversized output without reading it,
  while actual input pointers remain protected even with oversized lengths.
  The native C fixture covers these paths and its ownership/unload checks pass.
- After this integration, the complete workspace suite, workspace check and
  strict Clippy passed. Five compiled Wasm editor checks and the native panel
  check were separately executed and passed. Host cross-checks passed on Android,
  macOS, Windows and iOS. The CI workflow includes the new service example.

File-provider integration implemented:

- SDK format directions and resource kinds dispatch real callbacks. Each operation
  owns an immutable private input snapshot and writable output, capped at 256 MiB
  each. `host/files` streams base64 chunks up to 256 KiB on the worker, with bounded
  reads, append/overwrite and resize. File bytes are not constrained to one JSON
  message. The runtime admits at most eight active file operations.
- Revocable scopes survive service interception and asynchronous document queries.
  Participants from earlier chunks remain required through artifact publication.
  Cross-runtime operations/replies, foreign handles, stale document versions,
  invalid offsets/sizes and attempts to edit charts from a converter are rejected.
  Artifacts own temporary files; disabled/cancelled participants cannot publish
  them over a desktop destination.
- Editor menus import/export formats and convert resources using the native
  chart/media validators, private-project import, unsaved-changes dialog and
  resource Undo. Chart imports work from Projects home. Back cancels queued
  preparation and in-progress work; late replies do not replace a changed project.
  Cleanup completions are drained after cancellation, and worker failures remain
  visible even when scope revocation happens before the error reaches the UI.
- Standalone SDK-only `chart-envelope` and `pixel-grid` Wasm examples exercise
  editor menu actions. The first preserves unknown chart JSON through custom
  export/import and ordinary RPE saving after disable. The second imports a valid
  BMP, preserves its bytes in project export/reimport and uses native resource
  Undo after disable. No existing editor feature was migrated.
- Live service/event cascades and unpublished completions have aggregate count
  and byte limits in addition to per-message limits. Tests cover exponential
  event fan-out and sequential service deliveries, including suppression of an
  unpublished edit when a later delivery exceeds the budget.
- Android's generic picker/export Java bridge compiles against SDK platform 35
  with Java 17. Equivalent iOS bridge changes reuse its existing document picker;
  neither is a claim of physical-device verification.
- Thirteen separately built Wasm examples pass fifteen explicitly executed headless
  editor checks, as does the native Rust note-shift panel. The workspace checks,
  strict Clippy, formatting and source/resource/icon provenance checks pass.
  The host crate cross-checks on Android, macOS, Windows and iOS; these do not
  exercise a platform window or native library loading on those systems.

The complete Android application now passes the `cargo ndk` arm64-v8a/API 26
check and strict app Clippy with the prepared NDK r30 ARM64 host view. The debug
APK builds through Gradle with Java 17 and SDK 35. The existing APK verifier
passes JNI exports and DEX declarations, API 26 native symbols, signature and
16 KiB ELF/ZIP alignment. These checks do not verify physical-device behavior. An earlier full
application Xvfb launch did not produce a usable window; the newer standalone
graphics probe below does pass on Mesa software OpenGL. Physical-device UI
verification is not claimed. The iOS application build requires the documented
macOS/Xcode environment.

Native file hooks implemented:

- `host.file.before` now gates explicit native chart loading, private-project
  opening/import, Save and project creation. It runs before Discard-and-open's
  checkpoint clear. Native project/history export is intercepted after private
  archive preparation and before offering a destination. Cancel preserves dirty
  work, Undo and recovery; export replacement changes only a safe suggested name.
- `FileIntercept` validates action/content/document identity at each replacement.
  Callback/service continuations stay read-only. Runtime-owned, single-use
  `Invocation` scopes propagate cancellation through effects, output and host
  document queries, and reject use by another runtime or document version.
- The editor retains native continuations and picker-source ownership while the
  hook runs. It checks approval again when queued native work is dispatched.
  Back, a changed document or disabled/reloaded participants invalidate pending
  work. Native export results also carry the revision of their original request.
- The SDK-only `file-lab` Wasm plugin exercises actual panel checkboxes for blocked
  and permitted Save/Open and a renamed native export. Recovery checkpoints and
  restoration, media/resource/provider workflows and video export are outside
  this hook's current coverage. Approval is a handoff, not rollback/preemption
  of native work or a system picker after it has started.

Project format bridges implemented:

- Format registration selects a Chart or Project bridge. Chart remains the
  default and omits the new field on the wire, preserving existing compiled
  Chart guests. Project encoding requires ProjectRead in addition to DocumentRead
  and receives the existing native project snapshot, including unsaved chart data,
  options and resources. Decode sees only the selected source.
- Operation-scoped package methods list/open/close input entries and create,
  read/write/resize/rename/remove output entries. Bounded metadata pages and
  256 KiB data chunks avoid whole-resource copies through IPC or guest memory.
  The host packs output entries and imports the result using the existing project
  converter and unsaved-changes/multiple-chart workflows.
- Core archive helpers reject escaping/aliased paths, duplicate names, conflicting
  file/directory prefixes, links, corrupted data and size/count overflows. The
  native import/export entry points retain their behavior; cancellable variants
  serve provider preparation/publication. The 1024-entry case is exercised without
  keeping one descriptor open per entry, and private snapshots are cleaned up.
- The SDK-only `project-pack` example carries 300 additional files, a resource
  larger than 16 MiB, music, an image, unknown JSON and unsaved chart changes through
  actual manager controls. Native export and Undo still work after disable.
  A second compiled UI check rejects malformed containers without losing edits.
  This uncompressed example keeps the 256 MiB custom-file limit; the bridge's
  expanded project data is capped at 512 MiB.

General asset access and images implemented:

- `host/assets` v1 serves ordinary commands, panels, render callbacks and service
  continuations independently of provider operations. Services permits reading
  the caller's installed package; ProjectRead additionally grants project files.
  List/open/read/image/close use relative names and activation-owned handles,
  without exposing operating-system paths or guest filesystem imports.
- Immutable private snapshots stream up to 256 KiB per read. Files and directory
  metadata are copied/enumerated on the worker. Aggregate size/count bounds,
  bounded relative paths, regular-file checks and link rejection apply before publication.
  Copies check cancellation between chunks and clean up partial files.
- PNG/JPEG/WebP/BMP decoding stays on the worker. Prepared image handles draw in
  existing canvases, preview overlays and tracks through the current egui adapter;
  the UI clips geometry/UVs and uploads at most two textures per frame.
- Snapshots and decoded images retain their owner and required service callers.
  Ordinary chart edits retain resources; native resource edits, project changes,
  asset-root changes and owner/caller revocation invalidate them. Project metadata
  publication is immediate and performs no filesystem work on the UI thread.
  `host.project.assets.changed` tells subscribers to refresh; external filesystem
  changes require closing/reopening a snapshot.
- The separately compiled `asset-view` example exercises its own packaged image,
  project listing and image preparation through real panel controls, then draws
  in all three surfaces. Its check covers chart edits, resource invalidation,
  project replacement, reload and immediate disable. Host checks also cover
  cross-owner access, delegated-consumer lifetimes, stream cancellation, bad and
  oversized images, handle limits, path escapes and late queries.
- The full workspace, strict Clippy and four portable-host target checks passed.
  These remain headless UI and type checks; device runtime checks are outstanding.

Project data and export policies implemented:

- SDK v1 envelopes carry a plugin-defined data version, arbitrary JSON and a
  Runtime/EditorOnly/RequiresConversion policy under the chart's existing
  `phiStudioPlugins` namespace. A typed patch operation expands to sparse normal
  edits and can share one transaction with notes/events. Host and runtime checks
  revalidate its producing owner. Malformed imported containers require explicit
  migration; other owners' data and unknown envelope versions remain intact.
- `host/document.plugin_data` provides bounded, version-checked reads and
  distinguishes missing entries from explicit null. Installation storage and
  project data remain separate. Disable/uninstall never removes chart data;
  core Save, recovery, history and callback-free Undo/Redo preserve it.
- Format export dialogs expose Preserve (default), Runtime and Remove choices.
  Only private snapshots are filtered. Runtime omits recognized editor-only
  entries and rejects data requiring conversion; opaque legacy/future entries
  remain intact. Remove explicitly clears the managed root namespace. Project
  snapshots apply the policy to every RPE chart JSON in the package while keeping
  non-chart resource JSON byte-for-byte. Native Save/export defaults are unchanged.
- Existing API 1 guest format messages remain unchanged. Rust drivers can select
  the policy when beginning a format-encoding operation; other operations reject
  non-preserve policies. Checked native export variants also check cancellation
  during secondary-chart JSON parsing. Filtering uses linear map retention.
- The external `project-data` guest persists tags through Save, reload and project
  changes, and its history survives disable. Compiled `chart-envelope` and
  `project-pack` checks select all policies through real controls and verify
  untouched sources, working state, resource bytes and failed-conversion behavior.

Transient preview effects implemented:

- PreviewEffect contributions return correlated, read-only descriptions of
  original prpr preset/custom GLSL passes, with float/vec2/vec4 uniform overrides.
  They operate after chart/global effects and HUD, before egui composition,
  sharing the existing graphics context and ping-pong target. Native chart
  shaders continue to work; transient effects never alter saved chart data or
  offline movie exports. The global Preview shaders setting controls both.
- Version/line/size/activation checks reject stale descriptions. Empty lists,
  expired frames, project replacement and unload revoke effects. Contributions
  and passes have deterministic priority/order and bounded admission. Shader
  source/uniform layouts identify programs; value updates reuse them. At most
  one program compiles per graphics frame, oldest pending first. Removed programs
  release materials, and cached failures affect only their own pass.
- The external preview-fx guest exercises real controls, presets, custom GLSL,
  diagnostics and cleanup. CPU checks cover ordering, stale work, resizing,
  background scheduling, owner revocation, compilation fairness and resource
  cleanup. Shader driver execution remains outside Wasm instruction fuel.
- The preview-effect-probe ran successfully under Xvfb/Mesa software OpenGL 4.6.
  Actual pixels verified live float/vec2/vec4 values, chained passes, failed shader
  isolation, exact removal, coexistence with a native chart shader and the global
  shader toggle. The baseline pixel was [10,15,27,255], tint [0,255,255,255],
  chained grayscale [179,179,179,255], and removal restored the baseline. This is
  graphics-context execution; it does not verify physical mobile GPU drivers.

Chart Linter and reusable inspection services:

- `plugins/chart-linter/` is a separately packaged SDK plugin. It checks chart
  structure/timing, resources, compatibility and explicit deep readability.
  Reports support filters, pagination, source navigation, cancelation and
  confirmed removals through existing sparse editor history. No built-in Tools
  move into the plugin.
- `host/project.info` returns versioned project metadata, actual audio duration,
  resource epoch and active plugin versions, with Services/DocumentRead/ProjectRead.
  It exposes no operating-system paths. `host/editor.reveal` selects an existing
  source location after ordinary service and native Seek hooks, with additional
  DocumentRead/EditorControl authorization and version checks.
- `host/preview.inspect` samples one source note at 1–64 time offsets using the
  existing parsed prpr objects. Services/DocumentRead/Preview are required.
  Sampling preserves animations, judging, pixels, audio and playback; parent
  depth and animation/sample work are bounded. The plugin adapts batch size.
  Non-finite geometry is represented explicitly, and stale callers are revoked.
- The opt-in `long_running` capability lets explicit commands/UI continuations
  retain a shared 262,144-dispatch background budget while leaving Cancel and
  other panel controls usable. Ordinary dispatch, live queue, message, memory
  and fuel limits remain in place. Event/render/hook callbacks do not receive
  this enlarged budget.
- Foreign JavaScript validators are not executable by this Rust/Wasm plugin.
  It reports unsupported schemas and native-engine differences. Bounded or
  unavailable checks are marked incomplete. See the plugin's README for limits,
  packaging and repeatable CPU/editor/GPU verification commands.
- Validation passed: 20 standalone rule/lifecycle tests; three installed-ZIP
  editor tests for scanning, real project resources, cancellation, navigation
  and Undo; and the original-renderer GPU probe, including parent transforms,
  below-line coordinates, zero width and the compiled Wasm deep scan. The APK
  was rebuilt and passed package verification; physical-device execution remains
  unverified.

Native editor action hooks and control service implemented:

- SDK `EditorAction`, `ActionIntercept` and bounded `EditorStatus` payloads add
  `host.action.before` and `host/editor` v1. Replacement retains action kind,
  caller and editor context. Native Undo/Redo, clipboard, deletion, selection
  scope, tool selection and explicit playback controls use existing handlers;
  extensions can additionally seek through the service. No Tools were migrated.
- Service actions require Services + EditorControl in editable continuations;
  document-changing actions also require DocumentEdit, and Copy DocumentRead.
  Status requires Services only. Exact document versions and caller/participant
  lifetimes are checked at admission and again at native handoff. Expected errors
  remain values and leave the calling plugin enabled.
- Pending hooks retain the original selection and shared clipboard identity,
  freeze actual playback without changing playback intent, and are canceled by
  Back, focus/touch cancellation, modal/context changes or unload. Source input
  gestures retain their single action/commit allowance through the asynchronous
  decision. Acknowledgements survive their own tool cancellation as read-only
  continuations. Native handlers keep their existing history boundaries.
- Nine new application tests cover real Delete key routing, history, identity
  forgery, permissions, stale versions, read-only service cascades, gestures,
  document replacement, selection changes, Back, disable and focus loss. The
  external `action-lab` Wasm module separately passes its actual egui control test
  for blocked Delete/Undo, API playback, replacement seeks, status and history
  after disable. Native seek/drag and automatic safety cleanup are not intercepted.

The implementation gates are covered by the delivery evidence above; remaining
platform runtime verification has the stated environment requirements. Format
providers now convert custom files to/from RPE or native project ZIP snapshots;
resource providers produce a resource supported by the native pipeline. They do not replace built-in
file actions automatically. Existing generators and other Tools are outside the
migration scope and have not been moved. Source JSON snapshots remain bounded by
the message limit; large-document readers can use the query service. The SDK
remains an API 1 draft for external use and review before compatibility is frozen.

SDK authoring and ABI details: [`phistudio-sdk/README.md`](../crates/phistudio-sdk/README.md).
