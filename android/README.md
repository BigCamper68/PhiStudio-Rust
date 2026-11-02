# Android build

PhiStudio Rust uses application ID `com.phistudio.rust`,
Android 8.0 or newer (API 26), and arm64-v8a. Rust supplies the editor and renderer;
Java supplies the Android activity, input and document access.

## Requirements and build

Install Rust stable with the Android target, JDK 17, Android SDK platform 35,
build-tools 35.0.0, and an Android NDK with tools for your host architecture.

```sh
rustup target add aarch64-linux-android
cargo install cargo-ndk --locked
export ANDROID_HOME=/path/to/android-sdk
export ANDROID_NDK_HOME=/path/to/android-ndk
cd android
./gradlew :app:assembleDebug --no-daemon
```

The APK is written to `android/app/build/outputs/apk/debug/app-debug.apk`.
Gradle builds the Rust library in release mode and packages it with the C++
runtime, required license notices, and the shared application icon. Debug APKs
use the local Android debug signing key; do not commit signing credentials.
Use clean Gradle packaging before distributing a build to avoid stale payloads.

On ARM64 Linux, the official NDK's Linux host tools may require an ARM64 overlay:

```sh
python3 tools/prepare-android-arm64.py --ndk /path/to/android-ndk \
  --output /path/to/ndk-arm64-host
```

Point `ANDROID_NDK_HOME` at the generated overlay. A host-compatible AAPT2 can be
selected with `-Pandroid.aapt2FromMavenOverride=/path/to/aapt2`.

## Package verification

```sh
python3 tools/verify-android-apk.py android/app/build/outputs/apk/debug/app-debug.apk \
  --ndk /path/to/android-ndk \
  --build-tools /path/to/android-sdk/build-tools/35.0.0 \
  --report artifacts/reports/apk-verification.json
```

The verifier checks native ABI, JNI exports, API compatibility, signing and
16 KiB ELF/ZIP alignment. It does not test device runtime. Document access,
keyboard behavior, multitouch, insets and lifecycle need physical-device checks.

Project imports use Android's document picker and private working copies.
Exports use the system destination picker. Editing, recovery and version history
are stored privately; checking out a version does not publish it as the saved
project until Save is pressed.

## Signed builds in GitHub Actions

Run **Actions → Android signed APK → Run workflow** on `main`. The workflow
builds an unsigned release APK without signing credentials, then signs it in a
separate hosted job using the `signing` environment. Download the
`PhiStudio-Android-signed` artifact for the verified APK. Release attachments are
not changed automatically.

The environment contains `ANDROID_KEYSTORE_BASE64` and
`ANDROID_KEYSTORE_PASSWORD` as secrets, plus `ANDROID_KEY_ALIAS` as a variable.
Its deployment branch policy permits `main` only. Signing checks the expected
certificate fingerprint, v2/v3 signatures, unchanged APK payload and 16 KiB ZIP
alignment. Key files exist only in a temporary directory in the signing job;
no application checkout or build scripts run there.

Keep an independent backup of the keystore and password. GitHub secrets are
usable by authorized workflows but cannot be downloaded through the normal
Secrets interface. Changes to the signing workflow must be reviewed carefully:
code with access to signing credentials can disclose them. Do not add untrusted
pull-request triggers or upload signing directories as artifacts.
