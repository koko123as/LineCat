---
name: test-scope
description: Use when about to run `cargo test` in `ui/`, the Concord gate, the Gradle unit tests or an APK build; when deciding which checks a change needs, in what order and on which machine; or when reporting which checks ran and which were skipped.
---

# Test Scope

Size every check to the change. The commands live in `.agents/commands.md` and `make help`; this skill decides which
of them a change runs and where.

## What a Change Runs

| Change under | Checks | Where |
|---|---|---|
| `ui/src/`, `ui/ui/`, `ui/tests/`, `ui/examples/` | `cargo fmt --check` and `cargo test --locked` in `ui/`; `cargo run --example snapshot` for a layout change | host |
| `ui/src/hct.rs`, `ui/src/colors.rs` | the above, after `cargo run --example gen_theme > src/theme.rs` | host |
| `ui/Cargo.toml`, `ui/Cargo.lock`, `ui/build.rs`, `ui/.cargo/` | the above, and the `slint-android.yaml` run that builds and checks `libseasoncat_ui.so` | host; CI |
| `android/` | the Kotlin unit tests; `gradle :app:assembleRelease -Ptarget-platform=android-arm64` for a build change | host; a device or emulator for always-on VPN, revoke, the permission UI, rapid start and stop |
| `android/core/jni` | `make rust-check`, the Kotlin unit tests (`:core` builds the glue) | host |
| `services/helper` | `make rust-check` | host; Windows for `--features windows-service` |
| Concord sources in `concord/` | Concord's own test scope (`concord/AGENTS.md`); `make core-check core-test` | host; another platform for its `cfg` tests |
| the `concord` pointer | `make core-gate`, then the APK build | host; CI |
| `assets/data/` | the APK build; its check step greps for both geo files | CI |
| `tool/hooks/` | `make hooks-test` | host |
| `tool/generate_seasoncat_icons.py` | `make icons`, then look at the changed images | host |
| `.github/scripts/`, `.github/ISSUE_TEMPLATE/` | `node --test .github/scripts/` | host |
| `.github/workflows/` | none locally; only a run proves it | CI |

- On the private `origin` every `build.yaml` job runs only when started by hand, and `slint-android.yaml` runs only on
  `slint-ui`, `v*-rs.*` tags and by hand, so start the run a change needs.
- Gradle, which also builds Concord for Android, a cold `cargo` build and a Concord release build each take long and
  load the whole machine.
- Report a check you could not run as skipped, with the reason: a missing toolchain (`make doctor` names it), another
  platform, or no device. Never report it as passed.

## Reading a Run

- `cargo test` prints one `test result:` line per test target; report the passed count of each target the change
  touched, not only the last line.
- A `slint-android.yaml` run is done when the `apk` job passes; its summary carries the APK's size, the libraries it
  packs, its version and its signer. The artifact is `seasoncat-slint-arm64-v8a`.
