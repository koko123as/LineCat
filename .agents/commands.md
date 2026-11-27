# Commands

`make` is the entry point on macOS and Linux: `make help` lists every target, generated from the `##` comments in the
`Makefile`. Each target only calls a command this file documents. `make doctor` names a missing toolchain, `make check`
runs the hook tests and the UI crate's format check and tests, and `make core-gate` runs the Concord gate.

## Setup

The Core is the `concord/` submodule:

```bash
git submodule update --init --recursive   # make setup
```

## The UI Crate

Everything in `ui/` runs on a desktop host with the toolchain pinned in `ui/rust-toolchain.toml`:

```bash
cd ui
cargo fmt --check                          # make fmt-check; make fmt formats in place
cargo test --locked                        # make test runs it with -j1, as one job builds out of memory in parallel
cargo run --example snapshot -- <out dir>  # renders every page with sample data to PNG files
cargo run --example gen_theme > src/theme.rs
```

The tests must not depend on CJK glyphs: CI's runner has no CJK font, so a test reads a page's text, not its pixels.
`tests/navigation.rs` checks sheets, pushed pages and the order back closes them in; `tests/entries.rs` checks that
every row on the settings pages reached from 工具 does something when tapped.

`src/theme.rs` is generated: `colors::theme_source` writes the vivid palette of each default accent from `src/hct.rs`,
and `theme_rs_is_what_gen_theme_writes` fails until `gen_theme` has been rerun after a change to either.

### libseasoncat_ui.so

`.github/workflows/slint-android.yaml` is the reference for building the library. In short, from `ui/` with
`ANDROID_NDK_HOME` pointing at NDK r30:

- `ANDROID_JAR` and `ANDROID_BUILD_TOOLS_VERSION` name platform 36's `android.jar` and build tools 36.0.0, because
  Slint's backend compiles and dexes a Java helper and `cargo-ndk`'s `ANDROID_PLATFORM` would otherwise be read as a
  platform directory.
- `LIBCLANG_PATH` points at the NDK's libclang and `BINDGEN_EXTRA_CLANG_ARGS_aarch64-linux-android` carries
  `--target=aarch64-linux-android24` and the NDK sysroot, for `rquickjs`' bindgen.

```bash
cargo ndk -t arm64-v8a --platform 24 -o ../android/app/src/main/jniLibs build --release --locked
```

`ui/.cargo/config.toml` links the library with `-z defs`, and CI then runs `ui/tools/check_so_symbols.py` (needs
pyelftools) against the NDK sysroot, so a symbol no linked library defines fails the build instead of `dlopen` on a
device.

## The APK

Gradle builds the Core and packages the APK. It needs `cargo`, JDK 17 and the NDK named by `ndkVersion` in
`android/gradle/libs.versions.toml`, and `libseasoncat_ui.so` already in `android/app/src/main/jniLibs/arm64-v8a/`:

```bash
cd android
gradle :app:assembleRelease -Ptarget-platform=android-arm64   # make apk
gradle -q printVersion                                        # versionName and versionCode
```

`target-platform` takes `android-arm`, `android-arm64` and `android-x64`, comma separated, and sets both the APK's ABI
filter and the ABIs `:core`'s `cargoCore` builds; without it all three are built. `android/gradlew` is not committed;
CI installs the Gradle version the wrapper properties name.

A release build is signed with `android/app/keystore.jks` when `android/local.properties` holds `keyAlias`,
`storePassword` and `keyPassword`; otherwise it is signed with the debug key and gets the `.dev` application id suffix,
as a debug build always does.

## Kotlin Unit Tests

The tests live in `android/tests/<module>/`, wired as each module's test source set. `:core` builds the Concord library,
so `cargo` and the NDK are needed, and one ABI keeps it short:

```bash
cd android
gradle :common:testDebugUnitTest :service:testDebugUnitTest :app:testDebugUnitTest :core:testDebugUnitTest \
  -Ptarget-platform=android-arm64
```

Always-on VPN, a system VPN revoke, the permission UI and rapid start and stop still need a device or emulator; unit
tests and compilation cannot prove those system callbacks.

## Rust Helper And JNI Glue

```bash
cargo fmt --manifest-path services/helper/Cargo.toml -- --check
cargo fmt --manifest-path android/core/jni/Cargo.toml -- --check
cargo test --manifest-path services/helper/Cargo.toml
cargo test --manifest-path services/helper/Cargo.toml --features windows-service   # Windows only
```

`make rust-check` runs the first three.

## Concord Core

Work inside the Core from `concord/`; `make core-check`, `make core-test` and `make core-gate` wrap these:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p xtask -- check-layers
cargo run -p xtask -- schema                      # rewrite concord/schema/, then commit it in Concord
cargo run -p xtask -- schema --check
cargo deny check
cargo build --release -p concord-desktop
cargo run -p xtask -- check-size target/release/concord 30000000
```

The workspace has one target directory, `concord/target`, and it grows past 10 GB. Never point cargo at another target
directory, never keep one per crate or per agent, and do not `cargo clean` a tree someone else is building. Run `cargo`
with `CARGO_INCREMENTAL=0` and without a stale `all_proxy`, as the `Makefile` does. `concord/AGENTS.md` and
`concord/.agents/` hold the Core's own commands.

## Generated Files

- `ui/src/theme.rs`: `cargo run --example gen_theme > src/theme.rs` in `ui/`.
- App icons: `make icons` runs `tool/generate_seasoncat_icons.py` (Pillow and numpy), which renders every launcher,
  TV, Play Store, notification, macOS and Windows icon and `assets/images/icon.png` from
  `assets_source/images/icon/seasoncat.jpg`.
- The desktop tray icons in `assets/images/tray/` have no generator in the repository; they are kept as committed.

## Git Hooks

```bash
make hooks          # pre-commit install for the pre-commit, pre-push and commit-msg hooks
make hooks-test     # bash tool/hooks/test.sh
make comment-density FILES="path ..."
```

`.pre-commit-config.yaml` runs `cargo fmt --check` in `ui/` when a `ui/` Rust file changes, the comment-density gate
(`tool/hooks/comment_density.sh`) on source and YAML files, and `tool/hooks/commit_msg.sh` on the commit message.
`tool/hooks/test.sh` runs every `*_test.sh` beside the hooks, so a new hook script needs only its test file.

`.github/scripts/issue-gate.js` is tested with `node --test .github/scripts/` against the forms in
`.github/ISSUE_TEMPLATE/`; no workflow runs that test.

## CI

- `build.yaml` runs on pushes to every branch but `slint-ui`, on `v*` tags and by hand. Its jobs are `hooks`
  (`tool/hooks/test.sh`), `android` (the Kotlin unit tests above), `rust` (Helper and JNI glue format, Helper tests),
  `rust-core` (the Concord gate) and `windows-helper-test`. In a private copy they run only when started by hand.
- `slint-android.yaml` runs on pushes to `slint-ui`, on `v*-rs.*` tags and by hand. Its `apk` job runs the UI crate's
  format check (a warning only) and tests, builds and checks `libseasoncat_ui.so`, builds the release APK, checks that it
  carries the UI library, the Core and the geo data, checks its version and signer, and uploads the artifact
  `seasoncat-slint-arm64-v8a` (`SeasonCat-slint-arm64-v8a.apk` and its `.sha256`).
- `issue-gate.yaml` labels an incomplete issue, and a daily sweep closes issues left incomplete and locks old closed
  ones.

### CI Caches

A run restores only cache entries saved on its own branch or tag, or on the default branch (`main`). `BRANCH_CACHE` in
`build.yaml` turns saving off in tag runs, whose entries no later run reads. `Swatinem/rust-cache` keys each entry on
its workspaces; setup-gradle writes only on `main` and `dev` in `build.yaml` and never in `slint-android.yaml`.

## Release

1. Raise `appVersionName` and `appVersionCode` in `android/app/build.gradle.kts`; the code only ever goes up, or the
   APK does not install over the previous one.
2. Write `release-notes/<tag>.md`; the release job fails without it. Name the files there `SeasonCat-<version>-…`
   (the v0.9.5-rs.2 and rs.3 notes name `FlClash-` files, as those releases had).
3. Push, wait for `slint-android.yaml` to pass, then push the tag `v<appVersionName>` (an `-rs.N` version, so the tag
   matches `v*-rs.*`).

On the tag the `apk` job needs the four signing secrets and a versionName equal to the tag, and the `release` job
publishes `SeasonCat-<version>-android-arm64.apk` and `SeasonCat-<version>-SHA256SUMS`, the names the in-app updater
(`ui/src/update.rs`) looks for, with the notes file as the release body. It also publishes the same APK as
`FlClash-<version>-android-arm64.apk` with `FlClash-<version>-SHA256SUMS` for v0.9.5-rs.3 and older, whose updater
knows only that name; the workflow says when that copy can go. Push the tag by name: `--follow-tags` skips a
lightweight tag.

## Worktree Tooling

```bash
bash tool/worktrees.sh list       # every worktree with owner tool and dirty/clean state
bash tool/worktrees.sh prune      # remove clean worktrees; add --force to drop dirty ones too
```
