# Local prpr patch

`patches/noise-touch-filter.patch` adds `Judge::update_with_touch_filter`, a single
callback after native touches have been merged, transformed and timestamped,
before any note is judged. Existing `update` supplies a no-op callback. Autoplay,
note matching, grades, holds, flick detection and score calculation retain their
upstream implementation.

The integration layer needs this hook to implement Phigros Noise Field finger
blocking for Tap, Hold, Drag and Flick together. The upstream input queue is
private; filtering only its public snapshot cannot suppress native judgement.
All block geometry and finger state live in `phistudio-prpr`, outside the vendor.

`UPSTREAM.json` retains the original revision and hashes. The source verifier
reverses this exact patch in a temporary directory and checks the restored file
against its pinned upstream SHA-256 and Git blob, then verifies all other files
directly. Updating the patch does not waive upstream verification.
