# Application update checks

Settings starts with **Application updates**, showing the installed version, a
**Check for updates now** button and a **Check for updates on startup** switch.
The switch is on by default and is saved with editor preferences, separately
from project data. A startup check runs once per application launch after
preferences have loaded. Disabling it does not disable manual checks.

The checker reads the public
[latest stable release API](https://docs.github.com/en/rest/releases/releases#get-the-latest-release)
for `BigCamper68/PhiStudio-Rust`, without authentication or project data. Publish
a non-draft, non-prerelease release with a SemVer tag such as `v1.32.0` or
`1.32.0`. Drafts, prereleases, invalid tags and malformed responses do not
produce update offers. Comparison uses numeric version precedence and ignores
build metadata; a development build newer than the release is never downgraded.

The installed version comes from the Cargo package, or the Android build's
version name when `PHISTUDIO_ANDROID_BUILD` is set. Android version codes are
shown separately by existing build UI and do not determine update precedence.

When a newer release exists, a dismissible banner appears in the editor or
Projects screen. **Open release** opens that release in the system browser;
**Later** hides the banner for the current session. Updates are installed
manually. The banner waits while Playtest or fullscreen preview is active and
does not pause playback or change chart/history state. Switching projects
preserves in-flight checks, results and notification dismissal.

Requests run on a worker with a 5-second connection timeout, a 12-second total
request timeout and a 256 KiB response limit. Network failures, GitHub limits
and unavailable releases are reported in Settings; failed checks never claim
the installed version is current. Repeated startup attempts and concurrent
checks are suppressed. Turning off the switch discards a pending automatic
result. Native iOS links are passed to UIKit on the main thread.

Tests cover real HTTP exchanges against a local fixture server, response
limits, semantic versions, error states, request ownership, persistence and
compact localized UI. Mobile network/browser behavior needs device checks.

For GPU snapshots with a working OpenGL context:

```sh
cargo build -p phistudio-app --example ui-layout-probe --locked
xvfb-run -a -s '-screen 0 320x720x24' target/debug/examples/ui-layout-probe \
  artifacts/checks/updates updates 320x720
```

This probe uses fixture results and covers English/Russian settings and banners
for available, current, offline and checking states without public API requests.
