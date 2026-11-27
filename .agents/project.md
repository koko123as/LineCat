# Project Context

SeasonCat (application id `season.us.ci.cat`) is an Android proxy client derived from FlClash v0.9.5-rs.1. It has
three parts:

- `ui/`: the UI, a Rust crate on Slint (`seasoncat_ui`), built as the `cdylib` `libseasoncat_ui.so` and rendered into
  the shell's `NativeActivity`. The pages follow FlClash's, page for page.
- `android/`: the Kotlin shell, four Gradle modules (`:app`, `:service`, `:core`, `:common`) that own the VPN consent,
  the VPN and proxy services, the quick settings tile, and the local socket the UI talks through.
- `concord/`: the Core, Concord, a Rust submodule (`chen08209/concord`) that reads Clash configuration and speaks
  JSON-RPC 2.0. `:core` builds it into `libconcord.so` with the JNI glue `libcore_jni.so` from `android/core/jni`.

CI builds and releases only an arm64-v8a APK. There is no desktop client; `services/helper` and the packaging metadata
under `linux/`, `macos/` and `windows/` are kept for one.

## Versions

- The UI crate pins Rust 1.100.0 (`ui/rust-toolchain.toml`) and Slint 1.19.1 with the `backend-android-activity-06`
  backend (`ui/Cargo.toml`). Its release profile builds for size (`opt-level = "s"`, LTO, one codegen unit, stripped,
  `panic = "abort"`).
- Concord pins its own toolchain in `concord/rust-toolchain.toml`, and the JNI glue in
  `android/core/jni/rust-toolchain.toml`.
- Gradle 9.8.0 (`android/gradle/wrapper/gradle-wrapper.properties`), AGP 9.4.1 with its built-in Kotlin 2.4.20
  (`android/settings.gradle.kts`), JDK 17. `android/gradle.properties` keeps `android.newDsl=false`, because the module
  build files use the legacy DSL.
- `android/gradle/libs.versions.toml` holds minSdk 24, compileSdk and targetSdk 37, and the NDK the Core is built with
  (28.2.13676358, r28c). The UI library is built with NDK r30 instead: Slint's Skia binaries are compiled with r30's
  clang, and only r30's `libc++_static.a` defines the symbols they need (see the `UI_NDK_VERSION` note in
  `.github/workflows/slint-android.yaml`).
- The app's version is `appVersionName` and `appVersionCode` in `android/app/build.gradle.kts`.

## Build Dependencies

- `cargo` and `rustup` with the pinned toolchains, and the `aarch64-linux-android` target.
- `cargo-ndk` (CI uses 4.1.2) for `libseasoncat_ui.so`; the Android SDK platform 36 and build tools 36.0.0, because
  Slint's Android backend compiles a Java helper against `android.jar` and dexes it with `d8`; the NDK's libclang for
  `rquickjs`' bindgen on Android.
- Gradle and JDK 17 for the APK and the Kotlin unit tests.
- `python3` with Pillow and numpy only to regenerate the app icons (`make icons`).

`make doctor` (`tool/doctor.sh`) reports which of these are missing.
