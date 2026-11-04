# SDK expansion reference

This describes the SDK expansion in release 1.32.0. The wire API remains 1;
new fields have defaults, and old Wasm packages retain their ABI. Existing editor
Tools remain built in.

## Consent and compatibility

Projects → Plugins owns installation, enable/disable, reload, deletion and
permission review. Plugin functionality belongs in editor menus and properties.
Enabling or reloading does not open windows, dialogs or timeline columns.

External packages start with no grants. Enable opens a review of capabilities,
exact network origins, local-network access and (separately) native-code trust.
Users can deny individual items. Plugins must handle denied optional access;
contributions requiring denied permissions fail activation. Applying changed
permissions disables the current activation and its dependants. Revocation is
immediate for queued host work. Expanding requested permissions/origins or
changing to a native backend requires renewed review. Reinstall clears consent;
personal settings, project data and protected credentials are retained.

`minimum_editor` is an optional semver requirement checked before loading.
`host/features`, version `^1`, method `get`, arguments `null` or `{}`, needs no
Services permission. The reply is `HostFeatures`: editor/API/platform, effective
grants, available service versions, widget names, extensions and limits. It is
metadata, not a request for extra permissions. Discover services before using
optional functionality. Contributions are still registered only in Activate;
packages registering new contribution variants require a compatible host.

## HTTP

Declare `network` and `services` capabilities and canonical exact origins:

```json
"network": {
  "origins": ["https://api.example.org", "wss://api.example.org"],
  "local_network": false
}
```

Origins have no path or trailing slash. Scheme and explicit non-default port
are part of the grant. Wildcards, URL credentials and fragments are rejected.
HTTP/WS additionally requires local-network consent and resolves only to
loopback/private addresses. HTTPS/WSS uses certificate/hostname verification.
DNS results are checked and pinned to the connection; all returned addresses
must pass policy. Link-local/metadata, unspecified, multicast, reserved and
transition ranges are rejected, even with local-network consent. Proxy environment
variables are ignored. Network permissions allow sending data a plugin can read
to its approved servers; they do not enforce a privacy policy at those servers.

Call `host/network`, version `^1`:

| Method | Arguments | Result |
| --- | --- | --- |
| `request` | `NetworkRequest` | Deferred `NetworkResponse` after body completion |
| `status` | `NetworkId` | `NetworkProgress` (running/ready/cancelled/failed, bytes/total) |
| `read` | `NetworkRead` | Base64 `NetworkChunk`, next offset and EOF |
| `cancel` | `NetworkId` | Cancel acknowledgement; request gets a targeted error |
| `close` | `NetworkId` | Cancels and releases the retained download |

The caller chooses a unique request ID **and** a download ID. IDs/handles belong
to one plugin activation. `NetworkRequest::get` supplies defaults. Methods are
GET/HEAD/POST/PUT/PATCH/DELETE/OPTIONS; uploads use optional `body_base64`.
Status/cancel can run while the initial request is waiting. Small responses
include `body_base64`; use `read` for large responses. Bodies stream into private
temporary files, not guest memory. Close retained downloads when done.

Limits: 16 retained downloads/sockets combined, four per owner; 16 active I/O
tasks; 60 new HTTP requests/socket opens per minute per activation; uploads 1 MiB;
responses default 8 MiB, maximum 64 MiB; aggregate retained download bytes 128 MiB;
inline bodies 256 KiB; read chunks 1–256 KiB; timeout 100 ms–120 s; at most five
redirects. Request headers are capped at 32/32 KiB. The host owns connection,
framing, cookie and WebSocket handshake headers. Decompression is disabled.

Only bodyless GET/HEAD follows redirects. Each hop repeats origin/address checks;
changing origins strips **all** supplied headers, including injected credentials.
Raw URLs, queries and transport error details are omitted from host I/O errors.
Network, WebSocket, secret, feature and diagnostic services bypass general service
hooks. Interceptors cannot redirect an ordinary call into those private services.

## WebSocket

Call `host/websocket`, version `^1`, with Network and Services grants:

| Method | Arguments | Result |
| --- | --- | --- |
| `open` | `WebSocketOpen` (ID, ws/wss URL, headers, optional credential/subprotocols) | Deferred `{id, protocol}` |
| `send` | `WebSocketSend` (text or base64 binary message) | Deferred byte-count acknowledgement |
| `receive` | `WebSocketReceive` (ID, timeout_ms 0–30000) | Deferred message or null, plus status |
| `status` | `NetworkId` | `WebSocketStatus` |
| `close` | `NetworkId` | Releases the socket |

There is one outstanding receive per socket; use another receive after each
reply. Incoming messages are bounded to 256 KiB, with a 16-message queue; the
outgoing queue has eight entries. Eight sockets globally/two per owner share
the HTTP concurrency limits. Each direction is capped at 64 MiB per connection;
600 messages/control frames per minute, 15 s handshake, 5 s writes and one-hour
lifetime. Overflow disconnects instead of growing memory. There are no automatic
redirects, reconnects, raw TCP, arbitrary certificates or collaborative chart
synchronization. Disable/reload/revocation closes transports and drops stale
callbacks. A service continuation retains its original editing/read-only scope.

## Credentials

`host/secrets`, version `^1`, needs Secrets and Services. `put` takes `SecretWrite`
(`key`, `value`, exact `origins`); `info` and `remove` take `SecretKey`. `info`
returns existence and allowed origins, **never** plaintext. Values are 1–16384
bytes, with a maximum 32 KiB encoded record and 64 records per owner. A protected
host index reserves slots before writes and survives restart. The key belongs to the calling
plugin; another plugin cannot name its owner.

HTTP/WebSocket requests can pass a `NetworkCredential` reference: key, header
(default `authorization`), prefix (default `Bearer `). Secrets permission and an
origin bound to that record are required. The host injects the value after
policy checks. No general hooks observe it. Password UI fields mask rendering
and redact Debug output; they require Secrets. They are not OS secure-entry
widgets or a guarantee that a plugin cannot mishandle entered text.

Backends: AndroidKeyStore AES-GCM with owner/key authenticated data and encrypted
private preferences; iOS Keychain (WhenUnlockedThisDeviceOnly); macOS Keychain,
Windows Credential Manager and Linux Secret Service through keyring. Locked or
unavailable stores return errors; there is no plaintext fallback. OS/hardware
protection and record-size limits vary by device. OS writes can fail below the
host size limit. Native code and a compromised host process can
access process memory. JSON copies are not comprehensively zeroized. Revoking
permission cannot retract bytes already sent or undo an OS write already handed
off. Protected records persist until the owning plugin removes them or app data
is cleared; permission review does not make native code a sandbox.

## Audio

`host/audio`, version `^1`, requires Audio and Services. DocumentRead/ProjectRead
are not required. Only already decoded current-project music is exposed, through
the original prpr/sasa clip. There is no competing decoder or audio output API.

`info`, `status`, `analyze`, `cancel` accept `AudioQuery` (document version,
optional expected assets version). `info` discovers `AudioSourceVersion` and
sample rate, stereo frame count/duration, progress and readiness. `analyze`
acquires an owner-scoped analysis lease and returns immediately. Poll status;
waveform can be ready before spectral candidates. `cancel` releases that owner's
lease, leaving work required by built-in guides/other plugins alone. Disabling
the owner or changing project/assets releases its lease. User guide preferences
and playback are unchanged. Existing analysis limits (including 30-minute audio)
still apply.

| Method | Payload | Bound |
| --- | --- | --- |
| `pcm` | `AudioPcmQuery`, stereo interleaved f32 little-endian/base64 reply | 4096 frames/page |
| `waveform` | `AudioWaveformQuery`, min/max/RMS buckets over a finite seconds range | 2048 buckets |
| `candidates` | `AudioCandidatesQuery`, paged native candidates with four salience bands | 1024 items/page |

Reads require the exact source token plus document version. Source replacement,
resource edits, project changes, invalid ranges and unready analysis are rejected.
Candidate filtering can run inside the guest; the existing native worker still
owns decoding/analysis/cache computation. PCM copying and waveform queries on
the UI thread are bounded; full scans do not run there.

## UI and editor extensions

`Table` has stable row IDs, columns/cells, selection and bounded height; selection
events return a row ID. `Tree` has stable IDs, nested nodes and expansion defaults;
selection returns a node ID. `Tabs` contains named pages and emits the selected
page ID. `Password` uses `SecretText` and a masked, length-limited input. Validation
includes hidden tab children, unique IDs, tree depth, cell counts and selections.
Panels are limited to 2048 widgets/nodes/cells and 32 levels. Tables allow 32
columns/1024 rows within that combined count. Tabs allow 1–32 pages.

`Effect::Dialog {panel, open, version}` addresses an owned Window panel. Only an
explicit command/UI/completed input gesture can open it. Activation, render,
hooks, events and close acknowledgements cannot open dialogs. One dialog is
active at a time; native workflows take precedence. Back/X, document change,
disable/reload/revocation dismiss it. `dialog.close` is a read-only UiEvent.

`ContextMenu {id,title,target,command}` adds a command to the native note,
selection or line properties' Plugin actions… menu, reachable by touch. The
command must belong to the same plugin. `ContextArguments` freezes document
version, target, line and up to 4096 source note IDs when opening the menu;
changed documents and revoked owners disable stale entries. This does not
replace native grid gestures.

`TimelineTrack {id,title,width}` adds a synchronized column inside the main
timeline. Width is 80–400 points, with horizontal scrolling for additional
columns. Open it explicitly from Windows; closing it cancels captured input.
It reuses Track render/input descriptions, the native beat/zoom/baseline and
generation-scoped caches. Legacy `Track` remains a floating window. Guest
rendering describes bounded primitives; no arbitrary egui or graphics pointers
cross the portable boundary.

## Developer tools

```sh
cargo run -p phistudio-plugins --bin phistudio-plugin -- new /tmp/my-mod example.my-mod
cargo run -p phistudio-plugins --bin phistudio-plugin -- validate /path/to/package
cargo run -p phistudio-plugins --bin phistudio-plugin -- inspect /path/to/mod.zip
cargo run -p phistudio-plugins --bin phistudio-plugin -- pack /path/to/package /tmp/mod.zip
```

Templates point to the local SDK checkout. Build the generated Rust guest for
wasm32-unknown-unknown and copy its module to `plugin.wasm` before packaging.
Validation/packing uses the installer's bounded, confined snapshot; ZIP traversal,
symlinks and unsupported imports fail. Wasm startup/ABI is checked under execution
limits; validation does not activate plugins. Native inspection validates the
binary without loading it. Existing destinations are not overwritten.

`host/diagnostics`, `^1`, `get`, `null`/`{}` returns the caller's counters and last
64 callback measurements: elapsed microseconds, failure flag, optional Wasm fuel
and linear memory. Runtime Registry snapshots also expose these summaries. It
does not retain arguments, headers, UI values, body contents or raw plugin error
messages. Elapsed time includes serialization/validation; it is not a hard native
CPU limit or a full profiler. Existing native execution privileges remain intact.

See [SDK Lab](../examples/plugins/sdk-lab/README.md), the
[main SDK reference](../crates/phistudio-sdk/README.md), and the
[implementation checklist](SDK-EXPANSION.md) for validation evidence and platform
limitations.
