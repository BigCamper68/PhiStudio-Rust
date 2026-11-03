# Functional reference

The user requested a standalone Chart Linter corresponding to the built-in chart
diagnostics in [rpe-next](https://github.com/kclg-YSJ/rpe-next).
Reference revision: `90386f94e7490e8f87c17c100b6718358bf89489`.

The implementation is independent Rust code for PhiStudio's public plugin SDK.
No rpe-next source code, UI assets or executable JavaScript is incorporated.

Reference behavior is described in the chart diagnostics section of its README
and implemented in `src/core/chart-diagnostics.mjs`,
`src/core/deep-diagnostics.mjs`, `src/core/diagnostic-resources.mjs` and
`src/ui/diagnostics-panel.mjs` at the revision above.

Required behavior:

- Explicit basic scan on the plugin worker, preserving source data and history.
- Note/event/BPM/parent validation, duplicate and Hold overlap detection,
  initial event values, optional-field validation and value/easing checks.
- Audio bounds with real duration, BPM changes, line beat factors and offset;
  no invented placeholder duration when audio is unavailable.
- Project resource resolution, missing/ambiguous references and compatibility
  targets, including indirect shader/skin references for deep inspection.
- Explicit, cancelable deep inspection: native preview geometry/readability,
  300 ms visibility and Flick/Drag-to-Tap checks over 40–160 ms and 200 chart units.
- Severity/category grouping, filtering, paginated results, accurate total
  counts, object navigation and explicit removals through ordinary Undo history.
- Invalidate results and in-flight work after document/project/resource changes;
  ignore late results after cancellation, disable or replacement.

PhiStudio-specific data and original prpr playback remain authoritative. Foreign
extension schemas and engine differences must be reported rather than silently
treated as supported or interpreted using a competing renderer.
