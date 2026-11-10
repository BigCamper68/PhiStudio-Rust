# iOS build

PhiStudio Rust 1.31.0 / build 65 targets physical arm64 iPhone and iPad devices
running iOS/iPadOS 16.0 or newer. The application opens in landscape.

## Build

Use macOS, Xcode 16.4 or newer, its command-line tools, and Rust stable:

```sh
python3 tools/build-ios.py
```

The manual `iOS device IPA` GitHub Actions workflow performs this build on a
macOS runner. Output is written to `artifacts/releases/ios/current/` and includes
the IPA and local verification data. Public release attachments are managed
separately and need not include those verification files.

The generated IPA is unsigned. Installation requires signing the complete app
with an appropriate certificate and provisioning profile. Signing credentials
are not included in the repository or build workflow.

The UIKit bridge provides document import/export, text input, clipboard access, web links through UIKit,
safe-area handling and audio suspension. Projects are stored under
`Documents/PhiStudio` and exposed through iOS File Sharing. The renderer and
application icon artwork are shared with the other platform builds.

Archive, Mach-O and checksum verification do not replace testing on a physical
iPhone or iPad, especially for keyboard behavior, multitouch and lifecycle.
