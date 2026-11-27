# AGENTS.md

This file is the entry point for AI coding agents working in this repository. Keep it small: each rule below is the
one-line form, and its explanation lives in the linked `.agents/` file.

## Start Here

The app is the Slint UI in `ui/` (Rust, `libseasoncat_ui.so`) inside the Kotlin shell in `android/`, built with Gradle
alone; the Flutter UI (`lib/`) is gone. Where the `.agents/` files describe `lib/`, they describe FlClash's Flutter UI,
which `ui/` ports page for page. `linecat-status.md` lists what each batch landed.

Read these files before making changes:

- [.agents/project.md](.agents/project.md): project overview, versions, and build dependencies.
- [.agents/commands.md](.agents/commands.md): build, development, code generation, and test commands.
- [.agents/rules.md](.agents/rules.md): lint, testing, generated-code, and workflow rules.

Read these only when the task touches their area:

- [.agents/architecture.md](.agents/architecture.md): core integration, providers, database, managers, build system, and
  local plugins.
- [.agents/setting-changes.md](.agents/setting-changes.md): whether a changed setting is patched live, re-applies the
  profile, or asks for an Android VPN restart.
- [.agents/surfaces.md](.agents/surfaces.md): fill levels, selection and status colors, and the few borders that
  remain, for any change to how a surface is colored.
- [.agents/ios-measurements.md](.agents/ios-measurements.md): measured iOS values the iOS-style surfaces and motion
  follow, and the deliberate deviations from them.
- [.agents/agent-config.md](.agents/agent-config.md): where agent guidance belongs, the repo skills in
  `.agents/skills/`, and worktree hygiene.
- [.agents/flutter-upgrade.md](.agents/flutter-upgrade.md): SDK-pinned workarounds to recheck before changing the
  Flutter version.
- [concord/AGENTS.md](concord/AGENTS.md): the Rust core's own rules, for any change inside `concord/`.

## Highest Priority Rules

- Implement an explicitly requested, scoped, low-risk change directly. Do not require brainstorming, design documents,
  implementation plans, multiple-option proposals, or repeated confirmation; ask only when ambiguity, destructive
  impact, extra authority, or scope expansion could change the result.
- The request sets the scope: an unrequested fix, cleanup, or refactor is a follow-up in the summary. Edit surgically,
  keep scratch checks out of the repository, and commit tests only where the task or the neighbouring code keeps them.
- A comment carries only what the code cannot say; never restate, narrate the change, or keep commented-out code. The
  `comment-density` gate fails a file whose added lines exceed 5% standalone comments. See Comments in
  [rules.md](.agents/rules.md).
- Start FlClash on the host only as a safe mode build, and end only processes you started, by recorded pid; never kill
  by name. See Running the App Locally in [rules.md](.agents/rules.md).
- The Rust UI makes no JNI calls: Android capabilities go through the Kotlin `UiBridge` socket (`ui/src/bridge.rs`).
- Android service intent arbitration belongs to `ServiceState`; the UI requests transitions, never owns them. Keep
  start/stop/restart latest-intent-safe. See Lifecycle Rules in [rules.md](.agents/rules.md).
- Commits follow Conventional Commits, carry no `Co-authored-by` trailer for a coding agent, and land follow-ups as
  regular commits. See Commit Messages in [rules.md](.agents/rules.md).
- Never write machine-local identifiers into the repository: the home directory or user name, an absolute checkout
  path, the host name, or a personal email. Use `$HOME`, a repository-relative path, or a placeholder.
- Anything that moves drops no frame while it moves and at most one as it starts (8 ms of UI work at 120 Hz). See UI
  Frame Budget in [rules.md](.agents/rules.md).
- Before reporting done, run `cargo fmt --check` and `cargo test --locked` in `ui/` (tests must not depend on CJK font
  rendering; CI has no CJK font); Android changes are built by `.github/workflows/slint-android.yaml`
  (`gradle :app:assembleRelease -Ptarget-platform=android-arm64` in `android/`). Say which check was skipped when a
  toolchain is missing.

## Repo Skills

Use the skills in `.agents/skills/` when a task matches their descriptions; `.claude/skills/` symlinks expose them to
Claude Code. They cover localization, UI work, UI performance, core/platform changes, test scope, and pre-commit
structural review.
