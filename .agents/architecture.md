# Architecture

## Layers

```text
ui/ (libseasoncat_ui.so, Rust + Slint)
   │  local socket <applicationId>.ui: channel 0 Core JSON-RPC, channel 1 shell commands and events
android/app (Kotlin: SlintActivity, UiBridge, ServiceState)
   │  CoreClient / NativeCore over JNI (Mobile)
android/core (libcore_jni.so) ── C ABI ── concord (libconcord.so)
android/service (VpnService / ProxyService, NetworkObserver, notification)
```

The UI makes no JNI or Android calls of its own. Everything Android goes through the shell, and the Core is reached
only through a JSON-RPC session the shell opens for the UI's connection.

## The UI Crate

`ui/` is the crate `seasoncat_ui`, built as a `cdylib` for Android and as a `lib` for its tests and examples.

- `build.rs` compiles `ui/app.slint`, the whole UI in one Slint file, with Slint's `material` style.
  `slint::include_modules!()` brings in `AppWindow`.
- `src/lib.rs` holds `android_main`, the entry `android-activity` calls in the shell's `NativeActivity`, and `App`,
  which owns the window, the bridge and the state, and wires every callback.
- Each page or feature is split in two: `<name>.rs` is the logic with no window (parsing, models, what a page shows),
  public so tests and the snapshot example reach it, and `<name>_app.rs` is that feature on `App`'s side, the
  callbacks and what they publish to the window. Unit tests sit in the logic modules; `tests/` drives the window.
- `src/bridge.rs` is the client side of the shell's socket (see The Bridge below). Core calls and stream
  subscriptions (`watch` with credit) go over channel 0; shell commands (`vpn.start`, `file.open`, `qr.capture`, ...)
  over channel 1.
- `src/store.rs` keeps the settings and profiles as one JSON file under the app's internal data directory; profile
  files live in `ui/profiles/<id>.yaml` inside the Core's home, since `config.apply` only reads files there.
- `src/theme.rs` is generated from `src/hct.rs` through `colors::theme_source` (`cargo run --example gen_theme`).
  Each accent's palette carries four surface levels, `page` (page body, sheets), `card` (cards, grouped rows),
  `block` (tiles and segments in a card) and `inset` (small marks in a block). A surface takes the level one deeper
  than the one it sits on and stands out by its fill, not a border; colour is kept for state. Light mode alternates a
  gray page and a near-white card, dark mode only lightens, and pure black (`pure_black` in `src/colors.rs`) drops
  every level by the page's tone.
- `src/script/` runs profile scripts on QuickJS through `rquickjs`: it evaluates `main(config)` with a 10-second
  interrupt deadline, since evaluation blocks applying the profile. Android builds enable `rquickjs`' `bindgen`
  feature, which needs the NDK's libclang (see the build step in `slint-android.yaml`).
- `src/update.rs` is the in-app updater: it reads the newest rs release from the GitHub release list, takes a tag whose
  `SeasonCat-<version>-SHA256SUMS` is attached, and installs `SeasonCat-<version>-android-arm64.apk` through the shell
  after checking its SHA-256. These names are what the release job in `slint-android.yaml` publishes; releases up to
  v0.9.5-rs.3, named `FlClash-`, are never offered.
- `examples/snapshot.rs` renders every page with sample data through Slint's software renderer, so a layout can be
  checked on a desktop.

## The Bridge

`UiBridge` (`android/app/src/main/kotlin/com/follow/clash/UiBridge.kt`) listens on the abstract-namespace socket
`<applicationId>.ui` and accepts only peers running as the app's own uid. A frame is a `u32` big-endian payload length,
one channel byte and the payload.

- Channel 0 carries the Core's JSON-RPC 2.0 frames verbatim. Each connection gets its own Core session
  (`NativeCore.open`), closed when the connection ends.
- Channel 1 carries small JSON commands (`{"id":1,"op":"vpn.start",...}`), their answers (`{"id":1,"ok":true,...}`)
  and pushed events (`{"ev":"state",...}`, `network`, `resume`, links). `hello` answers with the Core's home, the SDK
  level, the version name, the run state, insets, system colour seeds, the font scale, whether the device is a TV,
  any links waiting for the UI and, from API 30, the app process's newest `ApplicationExitInfo` (`lastExit`).
- The shell does only what the Rust side must not: the VPN consent and permission prompts, the services, files through
  the system pickers, the camera, the system CA stores, package lists and icons, installing an update.

`SlintActivity` is the launcher `NativeActivity` the UI renders into (meta-data `android.app.lib_name`).

## Android Service Lifecycle

The UI's requests are optimistic and the native layer is authoritative:

- `vpn.start` asks for the VPN consent when the run needs it and for the notification permission, hands the UI's
  `SharedState` to `ServiceState.syncSharedState`, saves it for quick start, then calls `ServiceState.requestStart()`.
  `vpn.sync` only updates the shared state (the notification's title after a profile switch); `vpn.stop` calls
  `requestStop()`. The run state reaches the UI as `state` events.
- `ServiceState` delegates to `ServiceStateMachine`, which owns the latest request through `RunIntentArbiter`.
  Identity checks discard obsolete work; `startPreparationLock` serializes start preparation and `transitionLock` the
  actual service transitions. `ServiceStateHost` is the part that cannot run on a plain JVM, so the machine is unit
  tested with an in-memory host.
- `RunState` is derived, never assigned by a step: `publish` computes it from the transition in flight, the latest
  intent and `ServiceController.runtime`. A service up under a stop intent reads `STOPPING`, and `STARTED` needs both a
  running intent and a runtime. The tile and the UI read this one value, so they cannot disagree with the service.
- `ServiceController` owns exactly one `ManagedServiceBinding`, selects `VpnService` or `ProxyService` from
  `VpnOptions`, and starts and stops it off the main thread. The binding and `runtime` are set and cleared together.
  The Core listens exactly while `runtime` is set: a start ends in `Core.startListener()` and every path that clears
  `runtime` calls `Core.stopListener()`.
- A start that does not bring the service up gives its intent back and takes down whatever it left running, unless a
  newer request has claimed the service, so a failed start always ends in `STOPPED`. Both the UI's apply and the native
  quick setup open the Core's listeners before any service exists, so `reconcileStopped` stops an empty service too.
  A repeat start on a settled run of the same kind mints no request.

Quick Settings, shortcuts, quick actions, revoke and Always-on VPN converge on the same owner:

- With no UI gateway handed to the machine, `handleStartAction()` claims the intent, restores `SharedState` from the
  quick start preferences, runs the Core's `quickSetup` with it, checks the VPN permission and starts. Claiming first
  is what makes a second action during the setup toggle that start off instead of racing a second setup.
- `handleStopAction()` stops the service natively, whatever the UI believes.
- Android may create an Always-on `VpnService` through `onStartCommand()`; the service sends the permission-protected
  `VPN_START_REQUESTED` broadcast, and `ServiceBroadcastReceiver` routes it to `handleStartAction()`.
- `VpnService.onRevoke()` stops TUN first, then sends `VPN_REVOKED`; the receiver requests a stop only while the run
  is a VPN run.
- `ServiceBroadcastReceiver` uses `goAsync()` with a one-shot completion: normal completion or a nine-second watchdog
  finishes the `PendingResult` exactly once, and the watchdog never redefines the lifecycle intent.

## The Core

`concord/` is the Rust core, its own repository (`chen08209/concord`) checked out as a submodule. It reads Clash YAML
and speaks JSON-RPC 2.0, described by `concord/schema/api.json`. Its internals are documented in the Concord repository,
from `concord/AGENTS.md`; this section keeps only how the app builds, hosts and talks to it.

### Sessions

`libconcord.so` holds one Core per process, and every client reaches it through a session of its JSON-RPC server
(`concord-ffi`: `concord_open_callback`, `concord_send`, `concord_close`, `concord_adopt_fd`). A session lives until its
owner closes it or its receiver reports the host gone, and ending one cancels every subscription it started. Two kinds
of client hold sessions, independently:

- each UI connection, through `UiBridge`;
- the service's own `CoreClient` in `:core` over `NativeCore`, opened on first use. The VPN service, `NetworkObserver`,
  the notification (a `traffic.watch`) and the memory callbacks reach the Core through `Core.kt`'s typed calls on it,
  so none of them needs the UI.

The JNI surface is the five entries of `Mobile` (`openSession`, `sendFrame`, `closeSession`, `setPlatformHost`,
`adoptFd`). Loading `Mobile` runs `System.loadLibrary("core_jni")`, which loads `libconcord.so` with it; the glue's
`JNI_OnLoad` starts the Core with `concord_start` and attaches every Core thread to the JVM. `PlatformHost` (`protect`,
`resolveUid`, `resolvePackage`) is registered beside `tun.start` and cleared after `stopTun` or a failed start.

### Build

`android/core/build.gradle.kts` registers `cargoCore`, which per ABI runs `cargo build --release --locked -p
concord-android` in the Concord workspace (`concord_dir` in `build_config.yaml`, default `concord`), builds
`android/core/jni` against it in the same target directory, and hands `libconcord.so` and `libcore_jni.so` to every
variant through `variant.sources.jniLibs.addGeneratedSourceDirectory`. `CONCORD_PRODUCT_NAME` comes from `product_name`
in `build_config.yaml`, the name Concord gives the MITM CA and the TUN device.

- `cargo` is found on `PATH` or in `~/.cargo/bin`, `/opt/homebrew/bin` and `/usr/local/bin`, which Android Studio's GUI
  `PATH` leaves out.
- The ABIs come from the `target-platform` property and default to all three. The NDK is AGP's `ndkVersion`, so the
  Core and the shell share one NDK, and the clang API level is the app's `minSdk`.
- `android/core/consumer-rules.pro` keeps `com.follow.clash.core.mobile.**`, `FrameReceiver` and `PlatformHost`, the
  classes JNI looks up by name.

The UI library is built separately with `cargo ndk` and NDK r30 (see `.agents/commands.md`) and dropped into
`android/app/src/main/jniLibs/arm64-v8a/` before Gradle runs. `:app` also copies `assets/data/GEOIP.metadb` and
`GEOSITE.dat` into the APK's `assets/data`, which `UiBridge` extracts into the Core's home on `hello`.

## Rust Helper Service

`services/helper/` is the privileged helper a desktop client uses to start the Core elevated so TUN works, on Windows
and Linux. No desktop client exists yet; what follows is the Helper's own contract, which such a client has to follow.
`build.rs` embeds the expected Core's SHA256 from the `CORE_SHA256` environment variable at build time.

On Windows the Helper owns its Service Control Manager lifecycle through two elevated commands:

- `FlClashHelperService.exe install --owner-pid <pid>` reads the account of that process (the app, which passes its
  own PID because the elevated installer may be another administrator), stops and removes any stale registration,
  creates the auto-start service for the current executable path with `--owner-sid <SID>` as its launch argument,
  starts it, and waits for the running state. A service started without an owner exits instead of serving anyone.
- `FlClashHelperService.exe uninstall` stops the service, waits for shutdown, removes its registration, and is also used
  by the Windows package uninstaller.

The app launches only the Helper's `install` command; it does not compose `sc.exe`, `taskkill`, or `cmd.exe` command
lines. The rs Helper registers under its own names so it installs beside FlClash's: the Windows service
`FlClashRsHelperService` (and its `Runtime` key), the systemd unit `flclash-rs-helper.service`,
`RuntimeDirectory=flclash-rs` and `/run/flclash-rs/helper.sock`. The executable keeps the name `FlClashHelperService`,
and the names below are FlClash's.

On Linux systemd takes the Service Control Manager's place, and nothing is registered at package install.

- `FlClashHelperService install`, run elevated through `pkexec` or `sudo`, writes
  `/etc/systemd/system/flclash-helper.service` for the current executable path and enables and restarts it. It reads
  `PKEXEC_UID`/`SUDO_UID` to learn who asked, and refuses to install without one. It also refuses a Helper whose binary
  or directory is not root-owned and non-writable, and refuses to replace a unit already installed for a different UID.
- Packages therefore have to ship the tree as 0755/0644; the rpm spec in `linux/packaging/rpm/make_config.yaml` sets
  `debug_package` and `__os_install_post` to nil, because rpmbuild's find-debuginfo and brp-strip rewrite the Core and
  a Core whose SHA256 no longer matches the Helper's embedded value is refused at `/start`.
- `FlClashHelperService uninstall` disables the unit, removes it and reloads systemd.
- The unit carries `Group=` (the owner's primary GID), `RuntimeDirectory=flclash`, the owner's UID/GID in
  `FLCLASH_HELPER_OWNER_UID`/`_GID`, a double-quoted `ExecStart=` with `%` escaped, and `Restart=on-failure` under a
  start limit. The Helper serves `/run/flclash/helper.sock` at mode `0660`, drops any connection whose `SO_PEERCRED`
  UID is not the owner's, retries an `accept` failure, and handles SIGTERM so `systemctl stop` still tears its Core
  down.
- `/start` requires the Core address to be a socket owned by the owner UID, and spawns the Core with the owner's real
  UID and an effective UID of 0, which the Core uses to hand the files it created back to the user.

`/start` opens the fixed Core executable beside the Helper without write/delete sharing, validates it against the
embedded SHA256, and keeps that handle open through process creation. `/ping` only compares the requested `coreSha256`
with the embedded value and checks the Core path exists. Protocol version 7 (9 on Windows) uses 32-character
lowercase-hex session ownership:

- `GET /ping?coreSha256=...` returns the Helper executable path with `x-flclash-helper-protocol` when the SHA matches.
- `POST /start` rejects unknown JSON fields, validates `{address, sessionId}` and, on Linux, the socket owner, releases
  any previously managed Core, verifies the Core and returns `{sessionId, pid}`. The Helper passes `sessionId` to the
  Core as one line on its standard input; it is the Core's IPC token and appears in no argument list, environment, or
  log.
- `POST /stop` validates `{sessionId}` and only stops the matching managed Core. A session mismatch is HTTP 409.
- `GET /logs` exposes the bounded recent Helper/Core stderr buffer with `no-store` caching.

`/start` and `/stop` take at most 4096 bytes and need a `Content-Length` (HTTP 413 and 411 otherwise). All endpoints
reject `Origin` headers and non-local `Host` values; the accepted authorities are `127.0.0.1`, `localhost` and the
Linux Unix-socket placeholder `FlClashHelperService`. The Helper grants no CORS access.

Endpoints bind only to `127.0.0.1` on Windows and to `/run/flclash/helper.sock` on Linux. The Windows port is picked by
the system and published as the `Port` DWORD of a volatile `Runtime` key under the service's registry key, which only
administrators can write; a client falls back to 47890 for an older Helper. Who may call is decided per connection: the
Linux socket drops a peer whose `SO_PEERCRED` UID is not the owner's, and the Windows listener (`service/peer.rs`)
traces the connection through the TCP table to the process holding its client end and drops it unless that process
runs as the owner SID. When the Helper service shuts down, it stops the Core process it owns; under systemd the unit's
control group does the same.
