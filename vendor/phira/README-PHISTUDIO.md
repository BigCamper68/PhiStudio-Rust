# Original Phira renderer

`prpr`, `prpr-auto-offset`, `prpr-l10n` and the optional `prpr-avc` are copied
verbatim from TeamFlos/phira commit `0afd16621dd66557841a8cffe6813afff63335ec`.
All source files, crate manifests, shaders and locales are unchanged.

The only build wiring change is the member list in the outer `Cargo.toml`:
the Phira app and unrelated tools are not members of this nested workspace.
`UPSTREAM-Cargo.toml` retains the original. `UPSTREAM.json` records the SHA-256
and upstream Git blob of every copied file. Run
`python tools/verify-prpr-source.py` from the PhiStudio root to check them.

PhiStudio's editor integration lives outside this directory in
`crates/phistudio-prpr`; it supplies resource loading, skin and an editing clock.
The parser, animation evaluation and rendering execute the original prpr code.
