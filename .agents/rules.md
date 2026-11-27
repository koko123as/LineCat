# Rules

These are repository coding and testing conventions. Codex command permission rules belong in `.codex/rules/*.rules`;
see `.agents/agent-config.md` before adding those.

## Comments

Comments are part of the code and some of them are load-bearing. What this repository limits is the useless and the
frequent. Every comment that restates the code devalues the ones carrying real information, until readers skim past all
of them — a file with three comments that matter is more readable than one with thirty.

The two halves are enforced differently, because only one of them can be counted. Whether a comment is useless is a
judgment, and it lives in the rules below and in review. Whether comments are too frequent is arithmetic, and
`comment-density` measures it.

### What a Comment Must Earn

- Write one when it carries something the code cannot: a non-obvious constraint, an upstream or platform behavior being
  worked around, a reason the next reader would otherwise get wrong, a coupling to a value defined somewhere else.
- Never restate what the code already says. If the comment and the line below it convey the same thing, the comment is
  noise.
- Never narrate. Not what this diff changed, not what the code used to be, not the order you did things in. Version
  history belongs in git and change rationale belongs in the commit message.
- Never annotate line by line or statement by statement. A block that seems to need a comment per step needs better
  names or a smaller decomposition instead — reach for structure before prose.
- Prefer encoding the intent in structure and naming. A named type or function that makes the invariant hard to break
  beats a paragraph asking the next reader not to break it.
- Delete commented-out code, stale version notes, and comments that only restate the code, whenever you edit the file
  that contains them. This does not need approval.
- License and copyright headers and code-generation markers are not comments and are exempt from every rule here,
  including the density count.

### Density

`tool/hooks/comment_density.sh` fails a file whose added lines are more than 5% standalone comment lines, ignoring
diffs under 20 added lines so small edits are never caught. It measures Kotlin, Rust, Go, Swift, Java, C and C++,
Gradle and YAML files, and skips `build/`, `generated/` and vendored `Clash.Meta/` paths. It is a ceiling on
frequency, not a target to fill.

Three gates run the same script, and they do not have equal force. The `PostToolUse` hooks in `.claude/settings.json`
and `.codex/config.toml` run after the tool and hand offending lines back as feedback; neither can undo the completed
write. The `comment-density` pre-commit hook fails the commit, and it is the only hard gate that covers every tool.
Behavior is pinned by `tool/hooks/comment_density_test.sh`, which CI runs through `tool/hooks/test.sh`.

When a change genuinely warrants more, raise the ceiling for that run rather than working around it:
`COMMENT_DENSITY_MAX=20 git commit`, or `SKIP=comment-density git commit` to step past it entirely.

### Where Knowledge Belongs

A comment is one of three destinations, and often the weakest. Pick by where the constraint would be violated, not by
how important it feels.

- **Assertable behavior goes in a test.** A test is the only form that cannot drift, because it fails when the behavior
  it describes is broken. Prefer it over both a comment and a document whenever the fact can be checked in code.
- **Repository-wide defaults, ownership, and invariants go in `.agents/*.md` or a `.agents/skills/*/SKILL.md`.** They
  are violated from many files, so they must reach every future agent at session start. A comment in one file cannot do
  that.
- **A fact that is true only at one call site, and is not visible from that call site, is the one thing a comment does
  better than a test or a document.** Its value is being in the reader's line of sight at the moment of the edit, such
  as the NDK note on `UI_NDK_VERSION` in `slint-android.yaml`, which whoever bumps Slint must see on the same screen.

Both failure directions are real. Moving a local constraint into `.agents/` hides it from the person editing the line;
leaving a repo-wide policy as a comment reaches only the reader of that one file.

## UI Boundaries

- The UI crate makes no JNI or Android calls. Everything Android, the VPN consent, the services, the Core session, files,
  the camera and the system CA stores, goes through the Kotlin shell's `UiBridge` socket (`ui/src/bridge.rs`,
  `android/app/src/main/kotlin/com/follow/clash/UiBridge.kt`). A new capability is a new shell command there.
- The UI reaches the Core only through the JSON-RPC session the shell opens for its connection (channel 0 of the
  bridge). It never loads `libconcord.so` itself.
- Do not expose direct filesystem deletion through the Core, the bridge or the Helper; use a scope-specific cleanup
  command instead.

## Lifecycle Rules

- Android start/stop requests from the UI are optimistic commands. Latest-wins arbitration stays in native
  `ServiceState`; do not add a UI-side completion that creates a second lifecycle owner.
- Android `RunState` is derived in `ServiceStateMachine.publish`; do not assign it from a transition step, and do not
  add a second "is it running" field beside `ServiceController.runtime`. Every divergence between the tile and the
  service so far came from a copy that one path forgot to update.
- Android service callbacks are not automatically user intent. Route explicit Quick Settings, Always-on VPN, and revoke
  actions through `ServiceState` and keep `ServiceController` as the sole binding/run-time owner.
- Every `BroadcastReceiver.goAsync()` path must finish its `PendingResult` exactly once. A watchdog may release the
  broadcast lease, but must not cancel, reverse, or otherwise redefine the service operation.
- Android's `system` nameserver resolves only through the DNS `NetworkObserver` reports; the Core's `system` client
  (`SystemServers` in `concord/crates/dns/src/system.rs`) falls back only to `default-nameserver`, so with no list
  every other lookup through it fails at once. The observer lives for the process and never sends an empty list on a
  service stop.
- The delayed DNS re-check `NetworkObserver.onLosing` posts is deliberately left un-deduplicated: the runnable re-reads
  `networkInfos`, and `updateDns` calls into the Core only when the resolved list or the preferred network changed.
- Callbacks of the service's `CoreClient` run on the Core's threads and must not block.

## UI Frame Budget

A frame is 16 ms at 60 Hz and 8 ms at 120 Hz. The bar for a push, a pop, a sheet, a menu or a tab switch is no dropped
frame while it moves and at most one as it starts. Most dropped frames are not spent computing but on doing work
nobody sees: building every row of a long list, rebuilding a whole page for one edited item, or loading a page's data
in the middle of its transition.

- Keep a list that grows with the user's data virtualized, and update one row when one item changes rather than
  replacing the whole model.
- Load a page's document before it moves or after it settles, behind a spinner, never halfway through its push.
- Keep blocking work, file and network access, Core calls and script evaluation off the UI thread; hand the result
  back to the event loop.
- Stop polls and timers for pages and sheets that are not shown.
- Decide on a phone at the largest data the app expects: missed vsyncs from `atrace` on a release build, the base and
  the change installed in turn. A change that leaves missed vsyncs within noise is reverted, however sound its
  reasoning. No gate checks this section.

## Testing Rules

- `cargo test --locked` in `ui/` covers the UI's logic and its wiring: the pure modules (`<page>.rs`) carry unit
  tests, and `ui/tests/` drives the window. Tests read text and properties, never rendered CJK glyphs.
- Kotlin unit tests live in `android/tests/<module>/`. No test loads the Core.
- The Concord workspace is checked by the CI `rust-core` job and by `make core-gate`.
- MITM sources, tests and fixtures name only reserved hosts (`a.test`, `example.com`) and neutral samples written for the
  test, never a published third-party module or a real app's ad or tracking endpoint.

## Running the App

- The app runs on an Android device or emulator. On a desktop host, check a layout with
  `cargo run --example snapshot -- <out dir>` in `ui/`; nothing else of the app runs there.
- Never end a process the agent did not start, and never kill by name (`pkill`, `killall`, `adb shell am force-stop`
  on a device the user is using): the user's own instance matches too. Record what you launched and end only that.
- A debug build, and a release build without the release key, installs as `season.us.ci.cat.dev` beside the released
  app; a release build signed with the release key replaces the installed app.

## Commit Messages

Subjects follow Conventional Commits and are enforced by the `commit-msg` hook in `.pre-commit-config.yaml`, which runs
`tool/hooks/commit_msg.sh`:

```text
<type>[(scope)][!]: <description>
```

- Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`.
- Scope is optional and lower case; use a comma to list several, as in `fix(core,android)`.
- `!` before the colon marks a breaking change and needs a `BREAKING CHANGE:` footer.
- Descriptions start in lower case, omit the trailing period, and keep the whole subject within 100 characters.
  Identifiers and acronyms keep their own casing, as in `fix(ui): AppBar text is truncated`.
- `Merge`/`Revert` subjects and `fixup!`/`squash!` commits are exempt.
- No `Co-authored-by` trailer crediting a coding agent, whatever that tool's own convention says. The history
  records who owns the change, not which tool typed it; human co-authors are still fine. The hook rejects the
  known agent identities.
- The hook still checks the `Changelog:`, `Changelog-Type:` and `BREAKING CHANGE:` trailers, but nothing generates a
  changelog from them any more: release notes are written by hand in `release-notes/<tag>.md`. `CHANGELOG.md` is
  FlClash's history up to v0.9.4 and is not regenerated.

Write what the change does, not that something changed: `perf(ui): stop rebuilding the proxy list per delay result`,
not `Optimize more details`.

Install the hooks once with `make hooks`.

### Fixup Commits

A follow-up lands as a regular commit. Create a `fixup!` commit only when the user asks for one, aimed at the commit
that introduced what it amends. A commit a release tag contains takes no fixup. Asking for a fixup means creating the
`fixup!` commit only: do not run `git rebase --autosquash`, amend, or otherwise fold it into its target unless the user
asks for the squash.

## Migrations

An upgrade starts from the last release, not from the previous development build. Before writing a legacy reader, a
data move, or a test for one, check that the state it reads shipped: `git tag --contains <commit>` on the commit that
introduced it, or `git show <tag>:<path>` for the file as the release had it. A state that only ever existed between
two releases gets no migration. Names a release wrote to a device keep working: the quick start state, for one, stays
in the `FlutterSharedPreferences` file under `flutter.sharedState` (`android/app/src/main/kotlin/com/follow/clash/Ext.kt`).

## Generated Code

Do not edit `ui/src/theme.rs` by hand; regenerate it with `cargo run --example gen_theme > src/theme.rs` in `ui/`.
The app icons are generated by `make icons` from `assets_source/images/icon/seasoncat.jpg`.
