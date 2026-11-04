# SDK expansion implementation

Scope: the user approved the complete extension proposal after release 1.31.0.
Existing tools stay built in. Release 1.32.0 is now authorized for publication, with APK and IPA only.

## Acceptance checklist

- [x] Persisted capability consent, revocation, native-code trust, and renewed
      consent when a package requests additional capabilities or network origins.
- [x] Host-owned asynchronous HTTP, bounded streamed responses, progress/cancel,
      HTTPS validation, explicit local-network access, redirect/DNS protection,
      and generation-scoped cleanup. Private calls bypass general interceptors.
- [x] Owner-scoped protected credential storage with platform backends and
      opaque credential use by network requests; no plaintext fallback.
- [x] Rich UI tables, trees, tabs, and plugin dialogs using the existing theme,
      touch routing, ownership and lifecycle checks.
- [x] Versioned audio service exposing bounded PCM/waveform and existing audio
      analysis without changing playback or duplicating the decoder.
- [x] Note/line context commands and embedded synchronized timeline tracks.
- [x] Developer CLI for templates, validation, packaging, and inspection;
      bounded runtime diagnostics for errors and execution cost.
- [x] Host feature discovery and explicit minimum-editor compatibility.
- [x] Public SDK-only examples and regression tests exercising permission
      denial, revocation, stale work, touch UI and the actual host integrations.
- [x] Documentation, licenses, localization, formatting, focused tests, strict
      Clippy and available mobile build checks.

HTTP and WebSocket transports share permission and lifecycle boundaries.
Collaborative chart editing itself needs a separate synchronization protocol.

## Working notes

- Branch: `feat/sdk-expansion`, starting at release commit `71118c4`.
- New host services must not block the serialized plugin execution worker or UI.
- Existing `host.service.before` exposes arguments: network/secret operations
  must be excluded from ordinary interception, including interception into a
  private service from an initially public request.
- Android now declares INTERNET. Platform networking permission does not replace
  per-plugin consent.
- Legacy installed external packages must not gain new permissions silently.
- SDK manifests use API 1 with deny_unknown_fields. New fields need serde defaults
  and omitted defaults to preserve existing wire messages.

## Validation evidence

- Focused runtime tests cover consent, dependencies, revocation, HTTP streaming,
  redirects/header stripping, untrusted TLS, WebSocket echo, cancellation,
  callback isolation and opaque credentials. Protected record quotas survive
  simulated restarts and are isolated by owner.
- App tests exercise real touch routing for rich widgets, consent and source
  menus, explicit dialogs, bounded decoded audio, stale source rejection,
  analysis lease preservation and synchronized embedded track frames.
- `cargo test --workspace --locked`: 700 passed, zero failures, 26 explicitly
  ignored tests. The separately executed application plugin integration suite
  (`--include-ignored`) passed all 75 tests, including compiled legacy packages
  and Chart Linter. Both separately compiled guest tests also passed.
- Workspace formatting and strict Clippy (`--all-targets --no-deps -D warnings`)
  passed. SDK Lab passed strict Clippy for its actual wasm32 target and a locked
  release build. Existing vendored prpr emits its unchanged deprecation warning.
- The developer CLI generated a standalone template, which built as Rust Wasm;
  validate, pack and inspect succeeded. SDK Lab's ZIP passed inspection and
  contains its source and license.
- Final SDK Lab GPU probes passed at 873×393 and 1194×834: feature-populated rich
  UI and embedded timeline rendered through real egui/prpr; opening was explicit,
  chart data stayed unchanged, and Disable cleaned up the UI. No external
  network request was started by those visual probes.
- The final Android debug APK built and passed the APK verifier, including the
  credential JNI bridge, native symbols, 16 KiB alignment, signature and packaging.
  Android startup configures tempfile's private `.tmp` directory before workers
  start; it does not rely on the app-inaccessible `/data/local/tmp` default.
- Source/asset/icon provenance checks and matching keys in all seven locale
  catalogs passed. Root README and vendored renderer sources are unchanged.
- Physical device credential/keyboard/network behavior has not been tested.
  The iOS release workflow compiles on macOS/Xcode; physical Keychain behavior
  remains a device verification limitation.
