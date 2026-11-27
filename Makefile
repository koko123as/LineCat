SHELL := /bin/bash
.DEFAULT_GOAL := help

##> FILES=paths    comment-density: files to measure instead of the uncommitted changes

CARGO := env -u all_proxy -u ALL_PROXY CARGO_INCREMENTAL=0 cargo
CORE_MAX_BYTES ?= 30000000
# concord/.agents/commands.md lists the same crates: the members that declare default features.
CORE_FEATURE_CRATES := concord-common concord-engine concord-mitm

COMMENT_FILES = $(or $(FILES),$(shell git diff HEAD --name-only --diff-filter=d; git ls-files --others --exclude-standard))

.PHONY: help setup submodules doctor hooks worktrees \
	icons \
	fmt fmt-check test apk hooks-test comment-density \
	rust-check check \
	core-build core-fmt core-check core-test core-gate core-schema \
	clean

help: ## List every target and variable
	@awk 'BEGIN { FS = ":.*## " } \
		/^##@ / { printf "\n%s\n", substr($$0, 5) } \
		/^[a-z0-9-]+:.*## / { printf "  %-18s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@printf '\nVariables\n'
	@sed -n 's/^##> /  /p' $(MAKEFILE_LIST)

##@ Environment

setup: submodules ## Fetch submodules

submodules: ## Update git submodules (Concord core)
	git submodule update --init --recursive

doctor: ## Report missing toolchains: cargo, the pinned Rust, Gradle, NDK, JDK
	bash tool/doctor.sh

hooks: ## Install the pre-commit, pre-push and commit-msg git hooks
	pre-commit install --hook-type pre-commit --hook-type pre-push --hook-type commit-msg

worktrees: ## List worktrees with their owner tool and dirty state
	bash tool/worktrees.sh list

##@ Generate

icons: ## Regenerate the app icons from assets_source/images/icon/seasoncat.jpg (needs Pillow and numpy)
	python3 tool/generate_seasoncat_icons.py

##@ Check

fmt: ## Format the UI crate in place
	cd ui && $(CARGO) fmt

fmt-check: ## Fail on an unformatted UI crate
	cd ui && $(CARGO) fmt --check


test: ## Test the UI crate, as CI does (one job: the build runs out of memory in parallel)
	cd ui && $(CARGO) test --locked -j1

apk: ## Build the arm64 release APK with Gradle; needs libseasoncat_ui.so in android/app/src/main/jniLibs
	cd android && gradle :app:assembleRelease -Ptarget-platform=android-arm64

hooks-test: ## Test the git hook scripts in tool/hooks
	bash tool/hooks/test.sh

comment-density: ## Hold uncommitted changes, or FILES=paths, to the comment cap
	@files='$(strip $(COMMENT_FILES))'; \
	if [ -z "$$files" ]; then echo 'comment-density: nothing to measure'; \
	else tool/hooks/comment_density.sh $$files; fi


rust-check: ## Format check and test the Helper crate and the Core's JNI glue format
	$(CARGO) fmt --manifest-path services/helper/Cargo.toml -- --check
	$(CARGO) fmt --manifest-path android/core/jni/Cargo.toml -- --check
	$(CARGO) test --manifest-path services/helper/Cargo.toml

check: hooks-test fmt-check test ## Hooks, the UI crate's format and tests

##@ Concord

core-build: ## Build the desktop Core in debug
	cd concord && $(CARGO) build -p concord-desktop

core-fmt: ## Format the Concord workspace
	cd concord && $(CARGO) fmt

core-check: ## cargo fmt --check, clippy (also each crate without its default features), check-layers and schema --check
	cd concord && $(CARGO) fmt --check
	cd concord && $(CARGO) clippy --workspace --all-targets -- -D warnings
	cd concord && for crate in $(CORE_FEATURE_CRATES); do \
		$(CARGO) clippy -p $$crate --no-default-features --all-targets -- -D warnings || exit 1; done
	cd concord && $(CARGO) xtask check-layers
	cd concord && $(CARGO) xtask schema --check

core-test: ## cargo test --workspace, and each crate without its default features
	cd concord && $(CARGO) test --workspace
	cd concord && for crate in $(CORE_FEATURE_CRATES); do \
		$(CARGO) test -p $$crate --no-default-features || exit 1; done

core-gate: core-check core-test ## The full Concord gate: also deny, fuzz, release build and size budget
	cd concord && $(CARGO) deny check
	cd concord && $(CARGO) check --manifest-path fuzz/Cargo.toml
	cd concord && $(CARGO) build --release -p concord-desktop
	cd concord && $(CARGO) xtask check-size target/release/concord $(CORE_MAX_BYTES)

core-schema: ## Rewrite concord/schema/ after an API or config change
	cd concord && $(CARGO) xtask schema

##@ Clean

clean: ## Remove the Gradle outputs
	rm -rf android/build android/*/build

