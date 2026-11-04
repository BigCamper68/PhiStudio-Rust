# PhiStudio SDK (API 1 draft)

This crate contains the shared protocol for statically linked Rust extensions,
external Wasm plugins and native C ABI plugins. It does not link egui, prpr,
the application or platform libraries.
The plugin-first rollout and incomplete application features are tracked in
[`docs/PLUGINS.md`](../../docs/PLUGINS.md).

The development expansion adds reviewed permissions, HTTP/WebSocket, protected
credentials, audio inspection, richer UI, context commands, embedded tracks and
developer tools. See the [expansion reference](../../docs/SDK-SERVICES.md) for
service payloads, limits, compatibility and platform verification boundaries.

## Lifecycle and ownership

Implement `Plugin::call`. Activation returns a `Response` containing contributions
and optional initial UI descriptions. Subsequent calls return effects and/or a
result. Contributions are registered only during activation. Local IDs are
scoped to the manifest's plugin ID; callers address a plugin and contribution
separately. A host must namespace emitted events as `<plugin-id>.<local-topic>`.

Activation also receives a `storage` JSON value when Storage was granted. The
worker persists `Store` effects outside chart data and supplies that value on
the next activation. Settings are validated against the current manifest and
restored before activation. Invalid state files are reported rather than
silently overwritten. Disabling/removing a plugin retains personal data.

`Deactivate` is best effort and its returned effects are ignored; persist
personal state through Store effects before deactivation. The host removes registrations unconditionally,
including when a callback fails or panics. Queued effects include an activation
generation and must be rechecked before application. Disabling a dependency
also disables its active dependants. Uninstalling a module does not remove its
chart data or invalidate previously recorded Undo/Redo entries.

`Err` from a callback represents a plugin failure and disables it. Report normal
user input errors through a notification/result instead of failing the instance.
No Rust object reference or trait object crosses the external boundary. Native
callbacks borrow serialized input bytes only for the duration of a call.

## Document editing

Read-only action context contains a `(session, revision)` version. Commands and
panels default to metadata-only context. They can explicitly declare
`snapshot: true` for a whole-chart snapshot when DocumentRead is granted;
bounded queries are preferable for large documents. Panel snapshots apply to
action buttons; field-change events retain metadata-only contexts. A transaction must name the exact
version used to compute it. Stale results are rejected even after Undo/Redo or a
switch to a new project with the same revision number. One callback can return
at most one `Edit` effect; put all changes in that transaction.

Operations are the add/replace/remove/test subset of JSON Patch, with RFC 6901
pointer escaping (`~0` for `~`, `~1` for `/`). Array add inserts and `-` appends.
Parents must exist. Array indices are evaluated sequentially against the staged
document, so delete multiple selected indices in descending order.

The core stages and validates all edits before changing live state. A failed
operation does not change history, revision, redo or dirty state. Changed notes
are recounted. Unknown fields are retained unless explicitly edited; undo
restores missing/null containers, imported counts and object ordering. History
stores reversible data and a host-owned label, never a plugin callback. Explicit
replacement of a large subtree necessarily retains that subtree in history;
prefer field/element edits for sparse history.

Custom persistent project data lives under `phiStudioPlugins.<plugin-id>` in
the source JSON. The typed envelope below controls explicit export copies;
arbitrary custom events still require a converter to become playable in Phira.

## Project data and export policies

`Effect::Store` belongs to an installation. Project-specific data instead lives
under `phiStudioPlugins`, keyed by plugin ID, and travels with the chart without
requiring that plugin to be installed. Use a typed operation inside a normal
versioned Edit transaction:

```rust,ignore
PatchOperation::PluginData {
    plugin: "example.tags".into(),
    entry: Some(PluginData::new(
        1,
        PluginDataPolicy::EditorOnly,
        serde_json::json!({ "tag": "Review this section" }),
    )),
}
```

The wire operation is `op: "plugin_data"`. Its entry contains the explicit
`format: "phistudio.plugin-data/1"` marker, the plugin's nonzero integer data
schema `version`, `export` policy and arbitrary JSON `data`. The host checks
that a typed operation names its producing plugin. DocumentEdit still grants
general chart editing; this helper is not an isolation boundary for raw patches.

The core creates the namespace when needed and preserves other owners' entries.
`entry: None` removes this entry and removes its container if it becomes empty.
A non-object imported namespace requires explicit migration through normal JSON
editing; the helper does not silently overwrite it. Typed data edits can share
one transaction with notes/events, so the complete action has one Undo boundary.
Undo/Redo, dirty state, Save, recovery and history use core-owned data deltas and
continue working after plugin removal. Unload/disable never deletes project data.

With DocumentRead and Services, call `host/document` v1 method `plugin_data` with
`PluginDataQuery { version, plugin }`. Its `PluginDataValue` reply distinguishes
missing (`present: false`) from a legacy explicit null (`present: true`). Entries
are returned verbatim, including unrecognized schemas, and checked against the
256 KiB query limit before cloning. Use the existing inspect/read/keys/slice
methods and `plugin_data_path` for larger values or migrations. Queries follow
the normal current-version/caller-scope rules and remain read-only. Observe
`host.document.opened/changed` to refresh after a committed edit.

Format-provider export dialogs offer an explicit copy policy:

| Export choice | Behavior |
| --- | --- |
| Preserve all plugin data (default) | Keeps every entry, including unknown/legacy/future data |
| Runtime data only | Omits recognized editor-only entries; keeps runtime and opaque entries; rejects recognized RequiresConversion entries |
| Remove all plugin data | Removes the complete root `phiStudioPlugins` namespace, including opaque entries |

Runtime filtering recognizes only valid v1 envelopes with understood metadata.
Unknown formats, unknown metadata fields, invalid owners and legacy shapes are
preserved. A RequiresConversion entry means a converter must bake/handle it;
runtime export cannot silently drop it. A custom format converter can instead
receive a preserved snapshot and implement its own documented conversion.

The policy transforms private input snapshots, never the working chart, history,
saved sources or resources. Chart bridges filter their single RPE document;
Project bridges apply it to the active chart and other RPE JSON charts in the
package. Resource JSON remains opaque and byte-preserved. Imports do not filter.
Guest Request::Format messages stay unchanged, including for old API 1 binaries.

Rust drivers can use `Runtime::begin_provider_with_plugin_data` and inspect
`Operation::plugin_data_export()`. Non-preserve policies are accepted only for
format encoding. Core drivers use `ChartDocument::with_plugin_data_export` or
`prepare_project_export_with_plugin_data[_checked]`. Existing native Save and
native project-export entry points continue to preserve all plugin data.
The SDK-only [`project-data`](../../examples/plugins/project-data) demonstrates
versioned tags, queries, lifecycle refresh and independent history.

## Hooks and services

Hook execution is ordered by ascending priority and then plugin ID. `Continue`
leaves the payload unchanged, `Replace` passes a new payload to the next hook,
and `Cancel` stops the chain. Hook failure aborts the intercepted operation.
Hooks cannot edit documents reentrantly. A chain has at most 512 callbacks,
512 returned effects and 16 MiB of serialized effect batches in aggregate.
Built-in hook payloads are validated against their schema. A driver defining a
custom hook must validate its replacement before performing the operation.

Services declare an independent semantic version. Return `Effect::Call` with a
`ServiceCall`: correlation ID, `ServiceRef { provider, id, version }`, method and
JSON arguments. The runtime invokes the provider and delivers a targeted
`Request::ServiceResult` to the caller, without an event subscription. The reply
contains `ServiceOutcome::Success { value }` or `Error { code, message }` and the
original action context, with no whole-chart snapshot. Expected failures such
as a missing service or incompatible version do not fail the caller instance.
Worker-resolved calls dispatch continuations before later effects in the same
batch; application-service replies may arrive in another turn. Put a loading
panel before its Call so it cannot overwrite the completed ServiceResult panel.

A consumer needs Services and a declared dependency on another plugin's
provider ID; self-calls need no dependency. Dependency versions describe the
plugin package, while the requested service version describes its API. Provider
effects retain both provider and caller ownership, so disabling either cancels
results still waiting for application. Application-service round trips share a
2048-step dispatch budget (16384 for a streamed file operation). Live cascades
also cap their queue at 2048 items/16 MiB and each unpublished completion at
512 effects/host calls and 16 MiB, including scope/context metadata. Releasing
consumed chunks permits streaming more than one message in total. Event fan-out
is bounded to 512 listeners/effects and 16 MiB. Hooks and activation cannot gain document editing by
indirectly calling a service. Events remain notifications and cannot implicitly
cancel or replace operations.

### Service interception and replacement

Register a Hook at `SERVICE_BEFORE_HOOK` (`host.service.before`) with Hooks
granted. It runs for plugin-to-plugin and public application-service calls.
Network, WebSocket, secrets, features and diagnostics bypass general hooks;
interceptors cannot redirect into those private services. Its payload
is `ServiceIntercept { consumer, call }`: the original caller ID and complete
`ServiceCall`. Hook contexts contain metadata, without chart/selection snapshots.
`Continue` preserves the call, `Replace` forwards the changed call through later
hooks, and `Cancel` delivers a normal `hook_cancelled` service error to the caller.
Hooks cannot change `consumer` or `call.request_id`. A malformed replacement fails
the hook and delivers `hook_failed`; the caller remains active.

Replacement can change the service address/version, method or JSON arguments.
Redirecting to another provider uses the last replacing hook's Services grant
and provider dependencies, or its own registered service without a dependency.
The original consumer does not need a dependency on that replacement module.
Argument/method changes within the original provider retain the consumer's access
checks. Application-service permissions still check the original consumer.
All target service versions are validated normally. Replacement providers must
honor the original service's response contract. The caller receives its original
service address and correlation ID, including after asynchronous host queries.

Every invoked hook generation follows provider effects, targeted replies and
host-service continuations. Disabling/reloading a participant rejects its old
results, including when a callback is in progress. Hook side effects execute
with read-only permissions and the shared dispatch budget. They and their
continuations skip service interceptors to prevent recursive hook entry; this
does not let them gain document editing or tool selection. A replacement
provider's ordinary calls can be intercepted, so a proxy hook should recognize
its own provider's requests when implementing a fallback.

`Action::Hook` also exposes named operation hooks to Rust application adapters.
`Output` retains result ownership even without effects; check
`Runtime::is_output_current` before using a result, plus the operation's document
version/schema. The editor checks ownership before applying worker completions.
Events remain one-way notifications, rather than hook decisions.

The external [`service-lab` example](../../examples/plugins/service-lab) replaces
a virtual application service with its own provider, toggles cancellation from
a real panel and checks transparent replies without any document permissions.

### Bounded document queries

Provider `host`, service `document`, version `1.0.0` needs DocumentRead and
Services, without a package dependency. `host` is reserved as a provider ID.
Arguments always name the original `(session, revision)` and an RFC 6901 path:

| Method | Arguments | Successful value |
| --- | --- | --- |
| `read` | `DocumentQuery { version, path }` | `DocumentValue { version, value }` |
| `inspect` | `DocumentQuery { version, path }` | `{ version, kind, length }` |
| `slice` | `DocumentPage { version, path, offset, limit }` | `DocumentItems` of array values |
| `keys` | `DocumentPage { version, path, offset, limit }` | `DocumentItems` of object keys |

`inspect` reports array/object entry counts or string UTF-8 byte length; scalar
length is null. Page limits are 1–4096 entries and responses are capped at
256 KiB before cloning a subtree. A page may be shorter than requested; continue
at `next_offset` until it is null. Values retain unknown fields and source
ordering. A single oversized value returns `response_too_large`; query one of
its children instead. The editor services at most two document requests per
frame before returning them to the worker.

An edited/replaced project returns `stale_document` before reading. Other normal
errors include `permission_denied`, `path_not_found`, `invalid_arguments`,
`method_not_found` and `service_unavailable`. A disabled/reloaded caller's pending
requests and replies are discarded. The standalone
[`note-statistics` example](../../examples/plugins/note-statistics) exercises
pagination, service results and a reusable histogram service without snapshots.

## Editor integration

Open **Windows → Plugins**, or use **Plugins** on the Projects home screen.
Desktop accepts a package-directory or ZIP path. Android/iOS use the existing
system document picker for a ZIP package. A ZIP contains `manifest.json` at its
root and the entry module at the path declared by the manifest. Packages are
validated on a worker before publication. Newly installed plugins start disabled;
enable them from the manager. Commands appear there and may declare keyboard
shortcuts. Panel descriptions render in windows, the sidebar or the inspector.

Packages live under `<workspace>/plugins/<id>`. Enabled state and settings live
in `plugins-state.json`, personal JSON data in `plugins-data/<id>.json`.
Removing a package moves it into `plugins/.removed` and preserves settings and
personal/project data. Reload reopens modules and restores validated settings.
Changing projects retains the host but invalidates pending document edits.

`Runtime` is the asynchronous application-facing operator API. `Host` remains
the synchronous protocol engine for workers/tests. `PackageSource` keeps a
temporary picker snapshot owned until installation finishes, even when queued.
The UI thread applies only still-current, validated results. Startup, reload,
execution, package copying and state writes do not execute inside a UI frame.

The action/file hooks and transient preview effects are described below.
Document lifecycle events are
currently `host.document.opened` and `host.document.changed`; notification
contexts contain IDs/version/cursor rather than whole-chart snapshots. Existing
Tools and editor features remain built in and are outside the migration scope.

## Native file workflow hooks

Register a Hook at `FILE_BEFORE_HOOK` (`host.file.before`) with Hooks granted.
The editor asynchronously sends `FileIntercept { action, content, version, name }`:

| Action | Content | Interception point |
| --- | --- | --- |
| `open` | `chart` | Before native chart loading or Discard-and-open's recovery clear |
| `open` | `project` | Before private-project opening or selected archive import |
| `save` | `chart` | Before an explicit native chart/working-copy Save |
| `save` | `project` | Before creating/saving a private project |
| `export` | `project` or `history` | After private archive preparation, before offering a destination |

`version` is the current editor session/revision that initiated the workflow,
including when opening/importing another document. `name` is a source/target
basename or suggested export name; it is null before creating a private project.
No OS path or chart snapshot is included. Hooks use the normal ordered chain and
short callback budget. `Continue` resumes the native workflow; `Cancel` reports
the reason without changing the editor or writing its save target. A replacement
may change an export's suggested basename; action/content/version and save/open
names are immutable. Replaced export names must pass `valid_file_name`. A malformed
replacement fails the hook and aborts that action. The archive's bytes are unchanged.

Hooks and their service continuations are read-only, even with DocumentEdit
granted. A decision is returned by the hook callback itself; asynchronous side
effects cannot revise it. The UI retains source/package ownership while waiting.
Back/Cancel revokes both a pending decision and approved work still queued for
the file manager. Project/revision changes and disabled/reloaded participants
discard pending decisions. Approval is rechecked at native dispatch. After that
handoff, the operation uses the existing native workflow; this is not rollback or
preemption of an already executing native save/copy or system picker.

Archive/project opening can produce a second `open/chart` hook when a chart is
selected. Recovery restoration/checkpoints, resource/provider operations, media
pickers and video export are outside this hook's current coverage. Interception
does not replace the built-in parser, save/history implementation or Tools.
File content transformation belongs in a format/resource provider.

Rust drivers use `Runtime::invocation(version)` with `Action::ScopedHook` for a
revocable chain. `Invocation::cancel` invalidates its output, effects and targeted
service continuations. Invocations are single-use, belong to one runtime and
retain their shared dispatch budget through host-service round trips. A failed
queue admission permits retry; a cancelled or already started invocation does not.
Check `is_output_current` or `is_invocation_current` before consuming a decision.
The external [`file-lab`](../../examples/plugins/file-lab) exercises these hooks.

## Native editor actions

`host/editor` v1 exposes `status` (`EditorStatusQuery { version }`) and `action`
(`EditorActionRequest { version, action }`). Both require Services and the exact
current session/revision, including the original callback context's version.
Status returns line, beat, playback/tool state, selection counts, clipboard
availability and Undo/Redo availability; it contains no chart or clipboard data.

An action additionally requires EditorControl (`editor_control`) in an editable
command, panel button, event or final input callback. Hooks, activation, render,
provider and intermediate input callbacks cannot gain control through services.

| Action | Additional capability | Parameters |
| --- | --- | --- |
| Undo / Redo | DocumentEdit | None |
| Copy | DocumentRead | None |
| Cut / Duplicate / DeleteSelection | DocumentEdit | None |
| Paste | DocumentEdit | `mirrored`, `original_time` |
| ClearSelection | None | None |
| SelectScope | None | Notes/Events/Both and `invert` |
| SetTool | None | Select/Box/Erase/Tap/Hold/Flick/Drag/Event |
| SetPlayback | None | `playing` |
| Seek | None | Finite absolute beat in `[0, MAX_EDITOR_SEEK_BEAT]` |

Successful actions return `{ document_changed, status: EditorStatus }`. Expected
permission, stale-version, busy, cancellation and native-handler errors produce
ordinary service errors without disabling the consumer. Native handlers retain
their normal history/clipboard/selection behavior. Playback actions from plugins
do not flush an unrelated shader draft. A final input continuation can perform
one native action or document commit; its acknowledgement becomes read-only and
survives its own completed gesture/tool cancellation, retaining activation scopes.
Replies retain the original callback context. After a revision-changing action,
wait for `host.document.changed` before initiating another action or edit.

Register `ACTION_BEFORE_HOOK` (`host.action.before`) with Hooks to intercept the
listed actions. Existing native history, clipboard, delete, scope/tool controls
and explicit playback buttons/shortcuts use the same asynchronous driver. Seek
currently enters through the control API. The payload `ActionIntercept` contains
the action, version, source plugin (null for a native control), line, beat,
playback intent and selection counts. The hook receives no source JSON or object
IDs. It may Continue, Cancel or Replace parameters of the same action kind.
Copy cannot become Cut, Undo cannot become Redo, and context/source identity
cannot change. Every replacement is validated before the next callback.

The editor retains selection/clipboard and cursor context while waiting. It
temporarily freezes playback without changing playback intent. Back, Cancel,
focus/touch cancellation, another modal workflow, changed document/selection/
layout settings or revoked participants discard the pending action before native
handoff. Approved actions invoke the existing built-in handlers. Once native
execution begins, this hook does not roll back or preempt it. Hook callbacks and
their service continuations remain read-only; they cannot invoke recursive native
actions. No subscribers means immediate handoff to the existing handler.

Automatic selection copying, internal history cleanup, Escape/suspend safety,
native seek/scrub gestures, note dragging and dedicated shader/noise operations
are outside this hook's coverage. Existing Tools remain built in. Extensions can
add their own commands, services and versioned document transactions for other
operations. The SDK-only [`action-lab`](../../examples/plugins/action-lab) exercises
blocking native/API actions, parameter replacement, playback, seeking and status.

## Format and resource providers

Register a `Format { id, title, extensions, decode, encode, bridge }` with Formats granted,
or `Resource { id, title, extensions, resource }` with Resources granted.
Extensions are distinct lowercase suffixes without a leading dot, at most 16
per provider; each suffix is at most 32 bytes. Formats default to decode enabled,
encode disabled and `bridge: "chart"`. Resource kind defaults to Data (native format detection).
The manager exposes explicit import/export/conversion controls; it does not
replace existing file actions or automatically intercept extensions.

The callback is `Request::Format` or `Request::Resource`, with metadata context
and `ProviderFiles { operation, input: { id, name, size }, output }`. There are no
chart/selection snapshots or filesystem paths in this request. The input is a
worker-created immutable copy, output is private and initially empty. Each file
is bounded to 256 MiB; at most eight operations can hold working files per runtime.
Format encoding additionally needs DocumentRead. A Chart bridge receives canonical
RPE and decodes to RPE. A Project bridge receives the native project ZIP snapshot,
including unsaved chart data and resources, and decodes to a native project ZIP.
Project encoding also requires **ProjectRead**: it can read the same project asset
directory that a native project export includes. Decoding reads only the selected
source, so it needs no access to the current project's files. The bridge is fixed
by the registration and repeated in `Request::Format`; Chart's default field is
omitted on the wire to keep existing Chart guests compatible.

Use `Effect::Call` to `host/files`, version `^1`, with Services granted. This
service runs on the runtime worker and requires a live operation scope:

| Method | Arguments | Successful value |
| --- | --- | --- |
| `read` | `FileRead { operation, file, offset, limit }` | `FileChunk { offset, size, next_offset, data }` |
| `write` | `FileWrite { operation, file, offset, data }` | `{ size }` |
| `resize` | `FileResize { operation, file, size }` | `{ size }` |

`data` is standard base64. `encode_file_chunk`/`decode_file_chunk` enforce the
256 KiB decoded chunk limit. Read limits must be 1–256 KiB; EOF returns an empty
chunk and null `next_offset`. Read either operation file and opened package files;
write/resize the output or created package entries. A null/omitted write offset appends. An explicit offset overwrites
existing bytes without truncating; resize can truncate or zero-extend before a
random write. Offsets past EOF, other operations' handles, unsupported versions
and invalid sizes return normal `file_error` outcomes. File service calls pass
through the existing service interception chain.

Finish with one `Complete { operation, file: output, name }` effect, using a safe
portable basename (up to 255 bytes), or `Fail { operation, message }` for an
expected input/conversion error. Only the owning provider can complete. Provider
callbacks and their service continuations cannot edit the current document.
The scope and shared dispatch budget survive asynchronous document queries.
File operations retain all participating generations, including interceptors
that affected previous chunks. Cancel/disable/reload or a document/session
change prevents the editor from accepting a pending result. The editor also
rejects a callback that neither completes nor leaves a service continuation.

On chart import, the worker validates RPE using the normal core loader and offers
a new private project through the existing unsaved-changes flow. Export stages
beside the chosen desktop destination and rechecks revocation before publication;
mobile uses the existing system export bridge. Resource output uses the native
image/audio/font validator, or the existing GLSL workflow for Shader, and the
normal private-project resource import/Undo path. Data currently means native
detection, not arbitrary unvalidated binary resources. Existing resource limits
still apply, even when a provider's working-file limit is larger. Imported data
and its host-owned Undo remain usable after the module is disabled.

Rust drivers call `Runtime::begin_provider`, submit `Action::Provider` with an
owned `PackageSource`, and check `is_output_current` and the document version.
`Output::artifact` owns a temporary directory until dropped; `Artifact::write_to`
checks current participation while publishing. `cancel_provider` revokes
immediately and returns a cleanup request ID; drain that completion through
`poll` as with other actions, even when the UI operation is already closed.
The editor retains the artifact through validation/import or a mobile picker;
releasing the last owner removes the private operation files.

The external [`chart-envelope`](../../examples/plugins/chart-envelope),
[`project-pack`](../../examples/plugins/project-pack) and
[`pixel-grid`](../../examples/plugins/pixel-grid) examples exercise these paths.

### Project files

Project bridges also use these `host/files` version 1 methods. Virtual paths
address entries inside this operation, not OS paths:

| Method | Arguments | Successful value |
| --- | --- | --- |
| `entries` | `PackageEntries { operation, output, offset, limit }` | `PackagePage { offset, total, next_offset, entries }` |
| `open_entry` | `PackagePath { operation, path }` | `ProviderFile` for a read-only input ZIP member |
| `close_entry` | `PackageRemove { operation, file }` | null; releases an opened input member |
| `create_entry` | `PackagePath { operation, path }` | `ProviderFile` for a new writable output entry |
| `rename_entry` | `PackageRename { operation, file, path }` | `ProviderFile` with its normalized new name |
| `remove_entry` | `PackageRemove { operation, file }` | null; deletes an output entry |
| `pack` | `PackagePack { operation }` | `{ size }`; snapshots output entries into the primary output ZIP |

`entries` defaults to input, with limits of 1–256 entries and 256 KiB per page;
continue at `next_offset`. Input is the immutable native ZIP during encoding.
During decoding it is the user-selected custom file; use raw `read` for a format
that is not ZIP. Each entry description contains its relative `path` and `size`.
`open_entry` validates/decompresses one input member on the worker and returns an
opaque handle for chunk reads. Reopening a live path reuses its handle. Closing
it releases the private copy; a later open returns a new handle. Input members
cannot be written, renamed or removed.

Output entries start empty and use the existing read/write/resize methods.
Handles remain valid after rename and become invalid after remove. Logical names
are normalized using the native project path rules. Absolute/traversing paths,
duplicate normalized names, file/directory conflicts and ZIP links/special files
are rejected. Paths are capped at 4096 UTF-8 bytes and 64 components; entries at
1024, individual files at 256 MiB and input/output expanded totals at 512 MiB each.
Metadata paging does not clone the entire directory per request. Private entry
files retain paths rather than open descriptors between calls, including at the
1024-file limit. All working files are released with their operation/artifact.

`pack` produces a new snapshot of the current output entries, without changing
the input. It does not parse charts. A subsequent `Complete` still names the
primary output handle; the app validates/imports that ZIP through its existing
project converter. This retains native multi-chart selection, options, nested
resources and the unsaved-changes dialog. Resource bytes need not enter Wasm
memory all at once. Native package preparation, extraction and entry copies check
cancellation between chunks/phases. Existing native ZIP/export limits apply.
Canonical RPE parsing, working copies, recovery and resource export continue to
belong to the core. Arbitrary project reads by non-provider callbacks are not
exposed through these operation-scoped file handles.

## General asset access

`host/assets` v1 is a read-only service for ordinary commands, panels, services,
hooks and render callbacks. It is independent of provider operation files.
`Services` grants access to the caller's installed package (`source: "plugin"`).
`ProjectRead` additionally grants the current project's files (`source: "project"`).
The host resolves relative slash-separated paths; absolute paths, traversal,
links, control characters and non-regular files are rejected. Paths contain at
most 4096 UTF-8 bytes, 64 components and 255 bytes per component.

| Method | Arguments | Result |
| --- | --- | --- |
| `open` | `AssetOpen { source, path }` | `AssetFile { id, source, path, size, project }` |
| `read` | `AssetRead { file, offset, limit }` | Base64 `FileChunk`, up to 256 KiB per read |
| `image` | `AssetHandle { file }` | `AssetImage { file, width, height }` |
| `close` | `AssetHandle { file }` | null; releases the snapshot and decoded image |
| `list` | `AssetList { source, directory, offset, limit }` | `AssetPage` with entries and next offset |

`open` copies a regular file to an immutable private snapshot on the plugin
worker, without passing an OS path or whole file through the protocol. Reads
remain stable after external modifications; close/reopen to refresh. There are
at most 16 handles per activation, 64 globally and 256 MiB of snapshot data in
total. Handles belong exclusively to their creating activation. They cannot be
passed to another plugin as a filesystem capability. Close handles when finished.

`list` enumerates one directory, sorted by relative path; an empty directory
string selects its root. Pages contain 1–256 entries and at most 256 KiB of
metadata. A directory may have at most 4096 children. Subdirectories are listed
as entries and can be queried separately; no recursive scan runs on the UI thread.

Project results include `ProjectAssetsVersion { session, revision }`. Resource
revisions are separate from chart revisions. Ordinary chart edits retain
snapshots; changing projects, resource revisions or asset roots revokes them.
Project queries require the callback's current document version. The application
publishes `host.project.assets.changed` with `{ "assets": ProjectAssetsVersion }`
when its resource state changes. Subscribe to `host.document.opened` as well to
initialize after activation. External filesystem changes are not watched.
Disabled/reloaded owners and required callers revoke access immediately.
Cancellation is checked during streamed copies and before publishing results.
Expected permission, path, size, stale-work and decode errors are ServiceOutcome
errors and leave the caller enabled.

`image` decodes PNG/JPEG/WebP/BMP on the worker. Each image is limited to 4096 ×
4096 dimensions and 16 MiB of RGBA pixels, with at most 64 MiB decoded globally.
It reuses an already prepared snapshot image. Draw it with
`DrawPrimitive::Image { file, min, max, tint }`; the host resolves the handle for
the drawing owner, clips geometry/UVs and uploads at most two new textures per
frame through the existing egui adapter. Missing, closed or revoked images draw
nothing. No renderer/GL object or guest filesystem import is exposed.

The SDK-only [`asset-view`](../../examples/plugins/asset-view) exercises package
assets, paged project listing and project images in a canvas, preview and track.

## Drawing and tracks

Register a Preview or Track contribution and handle `Request::Render`. Preview
contributions overlay the existing image; each visible preview instance gets a
separate callback. Tracks open in their own windows and share the editor's beat,
zoom, horizontal range, divisions and lanes. They can be closed normally/with
Back and reopened from the manager. Existing timeline modes and tools are not
replaced. An explicitly selected extension tool can capture input on its own
track or the preview through the input API below.

A `RenderFrame` identifies the local contribution, instance, unique token and
viewport. The context contains only version/line/beat/playback metadata, with no
chart snapshot or selection array. Return one `Draw { frame: token, layers }`
effect describing the complete frame. Only a matching render callback can draw;
an empty layer list clears its content. Static `Widget::Canvas { height, layers }`
uses the same drawing API inside any panel, without a per-frame callback.

Each `DrawLayer` chooses coordinates:

| Space | Coordinate meaning |
| --- | --- |
| `points` | Local screen points from the top-left of the viewport |
| `normalized` | `(0,0)` at the top-left and `(1,1)` at the bottom-right |
| `timeline` | Horizontal chart coordinate and absolute beat; track views only |

Circle radius, line width and font size always use screen points. Timeline data
is reprojected against the current clock while painting cached geometry. Draw
colors are RGBA arrays or theme tokens (`accent`, `text`, `tap`, etc.), resolved
at painting time. Text is literal content. Geometry is clipped to its viewport;
no egui objects, renderer handles or GL context cross the boundary.

The host checks finite coordinates within ±1e12, ordered rectangles, stroke
width 0–256, circle radius 0–8192 and font size 1–256. A description has at most
32 layers, 20,000 primitives and 256 KiB total text (16 KiB per text primitive).
Canvas height is 16–1024 points. These are bounds, not latency guarantees.

Rendering executes on the worker with the short callback fuel budget. It cannot
edit charts directly or indirectly through services. Commands/services can
prepare expensive cached data; later render callbacks describe it. A render
service result may update plugin state but must wait for a later Render callback
to draw. Service continuations remain background work in the UI and cannot acquire
document editing by crossing the asynchronous boundary.

There is at most one outstanding frame per visible contribution instance, with
at most eight pending background/user jobs before new frames are admitted. New
geometry replaces the desired viewport while work is pending. The oldest ready
view is serviced first, at most four submissions per UI poll, up to 30 Hz during
playback and 10 Hz while paused. Frames yield admission to user actions. Render
jobs and their service continuations do not disable panel controls or shortcuts.

Late results are rejected after owner/caller revocation, project/revision/line
changes or viewport layout changes. Closing a track stops requests once its view
expires; disabling removes it immediately even if a callback is still running.
Cached frames expire 500 ms after their request was submitted. Animations display asynchronous recent frames,
while absolute-beat geometry follows the live clock. The standalone
[`editor-guides` example](../../examples/plugins/editor-guides) exercises preview,
track, canvas, theme colors and its real settings checkbox without editing data.

## Transient preview effects

With Preview granted, register `PreviewEffect { id, priority }`. Its
`Request::Render` target kind is `preview_effect`, with instance `preview-effects`.
The viewport size is the physical shared preview texture size, without a
timeline. Return `PreviewEffects { frame: token, passes }`; an empty list clears
the contribution. Drawing and effect targets are distinct, correlated and
read-only. Existing overlay/track guests never receive this target kind.

Each `PreviewPass { id, shader, uniforms }` has a contribution-local ID, a
`PreviewShader::Preset { name }` or `Fragment { source }`, and a map of typed
`ShaderUniform::Float/Vec2/Vec4` values. Presets are the original prpr names in
`phistudio_core::shaders::PRESETS`. Custom sources are GLSL ES 1.00 fragment
programs using the original prpr vertex shader and `uv` varying. The renderer
provides `screenTexture`, `time` (chart seconds), `screenSize`, `UVScale`, Model
and Projection. Built-ins cannot be overridden by a plugin.

Custom parameters use plain supported GLSL uniform declarations, discovered by
the existing shader inspector; optional `%default%` comments have prpr semantics.
Their declared float/vec2/vec4 types must match the description. Vec4 values use
floating components, including normalized RGBA colors. Values must be finite.
Uniform names are ASCII GLSL identifiers of at most 128 bytes, excluding reserved
names and the `gl_` prefix. There are at most 32 overrides and 64 KiB of source
per pass, eight passes per contribution, and sixteen active passes globally.

Contributions are ordered by ascending priority, plugin ID and contribution ID;
passes retain their listed order. The first sixteen current passes are admitted.
They post-process the existing prpr ping-pong target after chart/global effects
and HUD, before egui overlays. All editor views share that processed texture.
Native chart effects remain active, and the Preview shaders setting controls both.
These transient passes do not modify chart data or affect offline movie exports.

Callbacks follow the ordinary bounded asynchronous render scheduler. Stale
version/line/size results and expired frames are discarded. Disable, reload,
project replacement and caller revocation remove effects at the next render
handoff; already submitted GPU work cannot be preempted. Source changes invalidate
their program, while uniform value changes reuse it. The graphics thread compiles
at most one pending program per frame, oldest first. Pending passes are omitted
until ready. Removed passes release their native materials.

A failed program affects only that pass. Diagnostics appear under its plugin in
the manager. Failures are cached until source/uniform layout/activation changes;
changing a value alone does not retry compilation. GPU compilation/execution has
the graphics driver's constraints, outside Wasm instruction fuel. Use trusted
shader sources; the portable CPU sandbox does not sandbox arbitrary GPU programs.

The SDK-only [`preview-fx`](../../examples/plugins/preview-fx) demonstrates real
panel controls, presets, custom GLSL and live uniforms. The native
`phistudio-prpr` example `preview-effect-probe` checks actual output pixels,
uniform updates, chained passes, shader failures, removal, native-chart shader
coexistence and the global shader setting on a real graphics context.

## Interactive tools

Declare Input and register a `Tool { id, title, surfaces, buttons, gesture }`.
Tools appear alongside existing tools in the toolbar and in the plugin manager.
A command or UI action can select its own tool with `SelectTool { id: Some(id) }`;
`None` deselects only the caller's selected tool. Selecting a built-in tool,
disabling/reloading the extension or its dependency releases the extension tool.
Existing editor Tools are unchanged.

Available surfaces are Notes, Events, Storyboard, ShaderEffects,
ShaderParameters, NoiseFields, NoiseAnimation, Preview and Track. Track input is
limited to contributions owned by the selected tool's plugin. The preview's
return control and native Noise Field editors keep their existing input. A
preview behind the grid is a drawing surface; its grid receives timeline input.
Buttons default to Primary and surfaces to Notes. Tap cancels beyond eight
screen points and yields to existing navigation where available. Drag owns the
gesture through release; it does not also invoke built-in placement or dragging.

Handle `Request::Input { tool, input, context }`. `InputBatch` identifies one
host-owned gesture, surface/instance, pointer kind/device/ID/button, frozen
viewport and start position. Ordered events contain Begin, Update, End or
Cancel, local/normalized coordinates, modifier keys, optional pressure and
`moved`. Timeline points also carry raw beat and exact snapped RPE time; note
points carry the host's snapped horizontal chart coordinate. Preview points
have no timeline coordinates. Release position is authoritative even if no move
event preceded it. Touches are delivered once, separately from the platform's
synthesized mouse stream. A second finger cancels capture; the remaining finger
cannot become a new placement.

Input runs on the serialized worker with metadata context, without chart or
selection snapshots. Begin/Update/Cancel and their service continuations cannot
edit; keep draft state inside the plugin and use Render for feedback. Update
positions coalesce while one callback is pending. The final End is retained and
can return one transaction directly or through a bounded document query/service
continuation. One gesture commits at most one changed transaction, recorded in
the normal callback-free Undo/Redo history.

Cancel, Back/Escape, focus/lifecycle cancellation, transform/layout changes,
project/revision/line changes or owner revocation invalidate the gesture before
an already-produced effect can commit. Scope follows service calls and replies.
Cancel notification is best effort and its effects are ignored. Failed queue
admission does not advance gesture state. End uses the explicit-action Wasm
budget; earlier phases use the short callback budget. Native callbacks retain
the native execution constraints described below.

The SDK-only [`note-stamp` example](../../examples/plugins/note-stamp) exercises
the real toolbar, mouse/touch input, `host/document` inspection, exact source
preservation and independent Undo/Redo after disabling the compiled Wasm module.

## Wasm32 ABI

Use `export_plugin!(|| MyPlugin::default())` and build a `cdylib` for
`wasm32-unknown-unknown`. The example in
[`examples/plugins/note-shift`](../../examples/plugins/note-shift) builds separately
from the editor and can be run through `plugin-probe`.

| Export | Signature |
| --- | --- |
| `memory` | Wasm linear memory |
| `phistudio_api_version` | `() -> u32` |
| `phistudio_alloc` | `(length: u32) -> u32` |
| `phistudio_call` | `(pointer: u32, length: u32) -> u64` |
| `phistudio_free` | `(pointer: u32, length: u32) -> ()` |

Input is UTF-8 JSON encoding a `Request`. The caller owns the input allocation
until the call completes. The result packs the output length in the high 32 bits
and pointer in the low 32 bits. Output is UTF-8 JSON encoding a `Response` in a
separate allocation. The host copies it and returns both buffers exactly once.
Empty/malformed responses, overlapping allocations and out-of-memory accesses
fail the instance. Reload after failure rather than reusing corrupted state.

The interpreter imports no OS or host functions. Default limits are one 128 MiB
linear memory, a 16 MiB module and 16 MiB messages. Startup/lifecycle/event/hook
callbacks have a smaller instruction budget than explicit commands, UI actions
and service calls. Budgets are configurable and are instruction limits, not
wall-clock timeouts. Compilation and substantial plugin execution must run on
workers; a limit does not make arbitrary guest work suitable for a UI frame.
The interpreter and its core are optimized in editor development builds too;
an unoptimized interpreter substantially changes callback latency.

## Native C ABI

Native packages use `backend: { "kind": "native", "entry": "plugin.so" }`
(the platform's DLL/dylib name on Windows/macOS). Each package targets the same
OS, CPU architecture and pointer width as its editor. The implemented loader
targets Linux, Windows, macOS and Android. Linux has executable C and Rust
integration tests; Android, Windows and macOS have host cross-checks, with
runtime verification outstanding. iOS dynamic native loading is unavailable;
use the portable backend there. These are verification/support limits, not
claims about store distribution policies.

Native code has full process privileges, no sandbox and no fuel/memory/time
limit. A native memory fault, abort or unresponsive callback can affect the
editor process. C ABI compatibility does not expose Rust's internal ABI or GL
context. Native requests, registration ownership, capabilities and transactions
still pass through the same protocol checks, which cannot constrain arbitrary
native instructions or OS access.

Installation/discovery validates the file without opening it as a library.
Explicit activation (including a previously enabled plugin on startup) opens
the library and executes its constructors. Disable destroys its state before
unloading its code. Native implementations must stop/join their own background
work and release callbacks before destroy returns. Requests remain serialized
on the plugin worker. Native package dependencies must be resolved by the OS
loader; the SDK does not rewrite platform library search paths.

The shared header is [`include/phistudio_plugin.h`](include/phistudio_plugin.h).
Export `phistudio_plugin_v1() -> const PhiStudioNativeApiV1*`. Its static table
begins with ABI version 1, SDK API version and table size, followed by C function
pointers for create, destroy, call and free. No Rust object layout crosses it.
Create returns opaque state; call borrows UTF-8 Request JSON and returns a
separate plugin-owned `NativeBuffer { data, len, status }`. Status 0 is Response
JSON; status 1 is an error string. The host copies a valid output then returns
it exactly once to that library's free function. Messages retain the 16 MiB
wire limit. Native addresses cannot be validated like Wasm linear memory;
libraries must obey the C contract.

Rust authors can use `export_native_plugin!(|| MyPlugin::default())` in a
`cdylib`. The SDK catches Rust unwinds inside its FFI entry points when built
with unwinding enabled; `panic = "abort"` still aborts the process. C++ authors
must catch exceptions inside their callbacks. The
[`note-shift` example](../../examples/plugins/note-shift) exports both backends
from the same implementation and provides native packaging instructions.
