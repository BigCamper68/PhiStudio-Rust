# Agent Guidelines

These instructions apply to the entire repository. Follow the project's existing conventions and prefer the smallest change that fully solves the task.

## Codebase map

Keep this section accurate when responsibilities or directories change. The other sections describe enduring expectations, not implementation details.

- `crates/phistudio-core/` — chart and project models, editing commands, undo/redo, persistence, conversion, version history, and related domain logic. It is independent of the UI and platform APIs. `src/noise_fields/` owns native/Next field geometry, authoring transactions, shared sampling, and Phira Pro color export.
- `crates/phistudio-app/` — application startup, editor workflows, and the `egui` UI. Important areas include `workbench/`, `timeline/`, `grid_input/`, `touch/`, `gui/`, `files/`, and `workspace_ui/`. `noise_ui/` provides paired Noise Field/animation timelines, horizontal field navigation, fixed animation columns, and the inspector with full-screen geometry and animation point editing alongside `shader_ui/`. `multi_line.rs` shares line columns, navigation and source IDs across notes, Events and Storyboard. Shared visual styling lives in `theme.rs`; UI localization lives in `i18n.rs`.
- `crates/phistudio-prpr/` — integration with Phira's original `prpr` parser, renderer, preview, playback, and resources. `src/noise_fields/` renders shared field snapshots and filters Playtest input.
- `crates/phistudio-audio/` — deterministic audio analysis, waveform generation, and related caching.
- `vendor/phira/` — vendored upstream Phira / `prpr` sources. `vendor/egui-miniquad/` — a locally patched `egui` adapter for the shared graphics context.
- `android/`, `ios/`, `desktop/` — platform-specific integration and packaging. Consult the platform-specific documentation before changing builds or native bridges.
- `assets/` — bundled fonts, icons, skins, locale catalogs, and demo data. `tools/` — build, validation, and smoke-test scripts. `.github/workflows/` — CI workflows.

## Engineering principles

- Write clean, restrained, idiomatic Rust. Optimize for clarity, consistency, and maintainability rather than cleverness or minimum line count.
- Read neighboring code before editing. Follow established naming, module boundaries, error handling, and data-flow patterns; reuse existing helpers where they fit.
- Make focused changes. Avoid unrelated refactors, mass formatting, speculative abstractions, generic frameworks, duplicated helpers, and unnecessary dependencies.
- Keep functions and types cohesive. Prefer straightforward control flow and explicit intent; introduce abstractions only when they meaningfully reduce complexity or repeated logic.
- Handle recoverable failures explicitly. Avoid unchecked assumptions, hidden side effects, and new `unwrap`/`expect` calls in fallible production paths unless an invariant genuinely guarantees safety.
- Respect performance-sensitive editing and rendering paths, but measure before adding caches, concurrency, or other complexity.
- Preserve public behavior and existing file formats unless the task explicitly calls for a change. Do not silently change saved data, import/export semantics, or backward compatibility.

## Architecture and behavior

- Keep chart and project business rules in `phistudio-core`, not in UI widgets or platform bridges. Keep audio analysis separate from editing and playback responsibilities.
- Use `phistudio-prpr` as the integration layer for the original `prpr` behavior. Do not build a competing parser, animation evaluator, or rendering pipeline without a specific requirement.
- Preserve the single shared Macroquad/Miniquad graphics context used by `prpr` and the `egui` adapter. Treat engine, graphics, and vendored dependency upgrades as coordinated changes, not routine cleanup.
- Preserve the upstream sources in `vendor/phira` during routine editor work; prefer fixes in `phistudio-prpr`. Treat intentional upstream upgrades as coordinated dependency changes with verifiable provenance. Do not change `UPSTREAM.json` merely to make verification pass.
- `vendor/egui-miniquad` is a locally patched adapter. Document changes in its `PHISTUDIO-PATCH.md` and retain its license texts.
- Protect editor invariants: undo/redo boundaries, dirty/saved state, selection, chart round-trips, unknown JSON fields, and project recovery. A complete user gesture should behave as one coherent editing action where appropriate.
- Treat mobile touch cancellation, input focus, keyboard/IME behavior, display density, lifecycle changes, and platform file access as correctness requirements, not incidental UI details.
- Keep decoding, analysis, archive processing, and other heavy work off the UI thread. Reuse existing workers and reject results from outdated projects, document revisions, or requests before applying them.
- Keep version-history commits separate from Save and editor Undo. Checking out or merging a version loads recoverable unsaved work; only Save publishes it as the saved project.
- Preserve bundled license texts, attribution, original resource bytes, and source-provenance records. Use the existing verification scripts when changing related integrations or packaging.

## UI and design-system consistency

- Extend the existing `egui` interface rather than introducing another UI framework or a parallel design system.
- Use the shared visual rules in `crates/phistudio-app/src/theme.rs` and existing nearby widgets. Match established colors, typography, spacing, control sizes, borders, interactive states, and panel layouts. Avoid arbitrary one-off styling.
- Keep equivalent actions and controls visually and behaviorally consistent across the editor. Match existing conventions for tool selection, shortcuts, dialogs, feedback, validation, and disabled states.
- Consider mouse, keyboard, and touch interactions together. Preserve gesture handling, cancellation, focus, scrolling, undo/redo, and accessibility of compact layouts on supported mobile screen sizes.
- Use the existing `TouchScroll` and `TouchCombo` wrappers for scrolling panels and dropdowns so vertical gestures do not activate child controls.
- Keep UI text localizable through the existing `i18n` approach. English is the canonical source text; maintain matching translation keys and placeholders in locale catalogs when adding or changing UI strings.
- Prefer small, coherent UI improvements over large visual redesigns. Do not change established interaction patterns without a clear task-specific reason.

## Language and documentation

- **Use English for all new or modified project-owned source code and documentation:** identifiers where practical, comments, docstrings, error messages, developer-facing text, tests, scripts, and Markdown documentation.
- Non-English text is permitted only where it is the purpose of the file or data: translations in locale catalogs, localization tests/fixtures, and externally sourced or required content. Do not translate stable serialized keys or user data merely to satisfy this rule.
- **Do not edit the repository-root `/README.md` without the user's explicit prior permission**, including formatting, wording, links, or incidental cleanup. This restriction applies only to the root file. README files in subdirectories may be updated when relevant to the authorized task.
- Keep documentation concise and factual. Do not add speculative promises, outdated version numbers, or instructions that have not been verified.

## Validation and delivery

- Add or update focused tests when behavior changes or a regression risk is present. Prefer existing test patterns and fixtures; test data round-trips, undo/redo, and input behavior when relevant.
- Run checks appropriate to the files touched. Use the explicit package list for formatting; `cargo fmt --all` also formats local dependencies and can change vendored code. Relevant commands include:

  ```sh
  cargo fmt -p phistudio-core -p phistudio-app -p phistudio-prpr -p phistudio-audio -- --check
  cargo test -p <crate> --locked
  cargo check --workspace --locked
  cargo clippy --workspace --all-targets --no-deps --locked -- -D warnings
  ```

- When renderer integration, bundled resources, icons, or packaging are affected, run the corresponding provenance checks:

  ```sh
  python3 tools/verify-prpr-source.py
  python3 tools/verify-note-skin.py
  python3 tools/verify-preview-assets.py
  python3 tools/generate-icons.py --check
  ```
- For UI, renderer, or platform changes, use the relevant tests and scripts under `tools/` and perform manual or device checks when available. Platform build steps and prerequisites live in their respective documentation.
- Leave unrelated files untouched. Update this file's **codebase map** when the structure actually changes; do not churn the general guidelines for ordinary feature work.