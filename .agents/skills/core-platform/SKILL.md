---
name: core-platform
description: Use when changing how the app reaches the Core, the UiBridge socket, Android services and their lifecycle, VPN/TUN, the Core's Gradle build or JNI glue, or the Windows or Linux Helper.
---

# Core And Platform

## When To Use

Use this for changes touching `android/` (app, service, core and common modules), `android/core/jni`, `ui/src/bridge.rs`,
the Core calls the UI makes, `concord/` integration, or `services/helper/`.

## Read First

- `.agents/rules.md`: UI Boundaries and Lifecycle Rules are binding: the UI makes no JNI calls, optimistic Android
  commands are arbitrated by `ServiceState`, callbacks are not user intent, `goAsync()` finishes once.
- `.agents/architecture.md`: The Bridge, Android Service Lifecycle, The Core (sessions and build), and Rust Helper
  Service describe the current mechanisms.
- `concord/AGENTS.md` and `concord/.agents/`: for a change inside `concord/`, Concord's own layering, fix rules and
  performance gate. This skill covers only the integration side.

## Owners

Every behavior here has one authoritative owner, and a change lands there:

- The UI's side of the socket and its Core calls: `ui/src/bridge.rs`; the shell's side and its commands:
  `UiBridge.kt`.
- Android start/stop intent: `ServiceState` and `ServiceStateMachine`; binding and run-time bookkeeping:
  `ServiceController`.
- The service's own Core session: `CoreClient` and `Core.kt` in `:core`, over `NativeCore` and `Mobile`.
- The Core library and its JNI glue: `cargoCore` in `android/core/build.gradle.kts` and `android/core/jni`.
- The desktop Helper's HTTP contract: `services/helper/`.

An owner has more entry paths than the UI: Quick Settings, launcher shortcuts and quick actions, notification actions,
Always-on VPN and revoke callbacks all reach it. Always-on startup is explicit through `VPN_START_REQUESTED` and revoke
through `VPN_REVOKED`; service creation or destruction is never read as intent.

## Verification

The matching commands are in `.agents/commands.md`:

- Android Kotlin: the JVM unit tests of the touched modules, with JDK 17; `ServiceStateMachine` is tested through an
  in-memory `ServiceStateHost`.
- The UI's side: `cargo test --locked` in `ui/`.
- The wire contract with the Core: `concord/schema/api.json` and `cargo test` in `concord/`.
- Helper: `cargo fmt --check` and `cargo test`; the `windows-service` feature only builds on Windows.

State host gaps explicitly. Always-on VPN, the VPN permission, a system revoke, polkit and systemd on Linux, and the
Windows Service Control Manager need their real platform even when the unit tests pass.

## Pitfalls

- A `200` from Helper `/stop` means the Core is gone. When termination cannot be confirmed the Helper keeps the child
  and answers `coreStopFailed`, and `/start` reports the same code instead of spawning a replacement; a client must
  not fall back to a direct launch on that code, or it races a Core the Helper still owns.
- The quick start state is read from the `FlutterSharedPreferences` file under `flutter.sharedState`, the names
  earlier releases wrote; renaming them loses the tile's quick start on every updated install.
- `:core` builds Concord in every Gradle build that needs the library, unit tests included, so a missing `cargo` or
  NDK fails the tests as well as the APK.
