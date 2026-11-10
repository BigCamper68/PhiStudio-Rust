# Release notes and About

The bottom of **Settings** contains **About the project**: the bundled PhiStudio
logo, application name, installed version/build, project link, update controls
and a **Release notes** button. The author card underneath uses the supplied
BigCamper68 avatar and links to the author's GitHub profile.

Installed-release notes are independent of the startup update-check switch.

On the first managed application launch after installation or a version/build
change, a release-notes window opens as soon as file/recovery dialogs allow it.
It reads the installed version's release from GitHub's `/releases/tags/vVERSION`
endpoint, falling back to the unprefixed tag. It never substitutes the latest
release for the installed version. Exact-version prerelease notes are supported;
the existing update checker still offers stable releases only.

The window shows release headings and bullet lists using native editor text.
Other Markdown remains readable text, with the original release available through
**Open release**. The release body is preserved in a bounded private cache,
`release-notes.json`, and a matching cached body works without a network connection.
The **Refresh release notes** button retries the GitHub request.

Closing successfully displayed notes records the version/build in the existing
editor preferences. The same installation does not automatically open them again;
**Release notes** in About always allows reopening them. Closing a loading/error
window does not acknowledge unread content, so a future launch can retry. Empty
release bodies show an explicit message and can be acknowledged normally.

The window participates in native modal and Back/Escape handling, pauses editor
playback and leaves chart data and history untouched. Pending recovery/file
workflows take precedence. Fullscreen preview and Playtest defer presentation.
Unmanaged direct-chart CLI sessions retain their existing ephemeral preferences
and do not show the automatic window; About remains available manually.

No GitHub credentials or project data are sent. Existing request timeouts, response
limits and validated release links also apply to installed-release requests.
Avatars and logos are bundled, so About does not need remote image requests. The
original user-supplied JPEG and its SHA-256 are recorded in `assets/about/SOURCE.json`.
