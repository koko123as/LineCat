#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
checker="$script_dir/comment_density.sh"
temp_dir="$(mktemp -d)"
trap 'rm -rf "$temp_dir"' EXIT

repo="$temp_dir/repo"
mkdir -p "$repo"
git -C "$repo" init --quiet
git -C "$repo" config user.email 'test@example.com'
git -C "$repo" config user.name 'Test'
printf 'fn main() {}\n' >"$repo/tracked.rs"
git -C "$repo" add tracked.rs
git -C "$repo" commit --quiet -m 'seed'

failures=0
status=0

dense() {
  local comments="$1" code="$2" i
  echo 'fn main() {'
  for i in $(seq 1 "$comments"); do echo "    // step $i explained"; done
  for i in $(seq 1 "$code"); do echo "    step$i();"; done
  echo '}'
}

run_stdin() {
  local target="${1:-}"
  local payload='{"tool_name":"apply_patch","tool_input":{}}'
  if [ -n "$target" ]; then
    payload="$(printf '{"tool_name":"Edit","tool_input":{"file_path":"%s"}}' "$target")"
  fi
  set +e
  printf '%s' "$payload" | (cd "$repo" && bash "$checker") >"$temp_dir/out" 2>"$temp_dir/err"
  status=$?
  set -e
}

run_argv() {
  set +e
  bash "$checker" "$@" >"$temp_dir/out" 2>"$temp_dir/err"
  status=$?
  set -e
}

expect_over() {
  local name="$1" path="$2"
  shift 2
  mkdir -p "$(dirname "$path")"
  printf '%s\n' "$@" >"$path"
  run_stdin "$path"
  if [ "$status" -ne 2 ]; then
    echo "FAIL: $name should exit 2, got $status" >&2
    failures=$((failures + 1))
  elif ! grep -q 'added lines are standalone comments' "$temp_dir/err"; then
    echo "FAIL: $name should report the density" >&2
    failures=$((failures + 1))
  fi
}

expect_under() {
  local name="$1" path="$2"
  shift 2
  mkdir -p "$(dirname "$path")"
  printf '%s\n' "$@" >"$path"
  run_stdin "$path"
  if [ "$status" -ne 0 ] || [ -s "$temp_dir/err" ]; then
    echo "FAIL: $name should pass, got $status" >&2
    cat "$temp_dir/err" >&2
    failures=$((failures + 1))
  fi
}

expect_over 'a diff that is a quarter comments' "$repo/dense.rs" "$(dense 8 24)"
expect_over 'block comment bodies count as comments' "$repo/block.kt" \
  '/**' \
  ' * first constraint' \
  ' * second constraint' \
  ' * third constraint' \
  ' * fourth constraint' \
  ' * fifth constraint' \
  ' * sixth constraint' \
  ' * seventh constraint' \
  ' */' \
  "$(dense 0 24)"
expect_over 'nested crate source is checked' \
  "$repo/ui/src/dense.rs" "$(dense 8 24)"
expect_under 'a diff with a few real comments' "$repo/sparse.rs" "$(dense 2 40)"
expect_under 'a small diff below the line floor' "$repo/tiny.rs" \
  'fn main() {' '    // why this retries' '}'
expect_under 'code with no comments at all' "$repo/clean.rs" "$(dense 0 40)"

expect_under 'a license header does not count' "$repo/license.rs" \
  "$(
    for i in $(seq 1 8); do echo '// Copyright 2026 The FlClash Authors.'; done
    for i in $(seq 1 24); do echo "const V$i: i32 = $i;"; done
  )"
expect_under 'go pointer dereferences are not comments' "$repo/deref.go" \
  "$(
    echo 'package main'
    echo 'func f(p *int) {'
    for i in $(seq 1 30); do echo '	*p = 1'; done
    echo '}'
  )"
expect_under 'a generated file is skipped' "$repo/src/generated/model.rs" "$(dense 20 4)"
expect_under 'a build output is skipped' "$repo/app/build/out.kt" "$(dense 20 4)"
expect_under 'a vendored upstream file is skipped' "$repo/Clash.Meta/hub.go" "$(dense 20 4)"
expect_under 'a markdown file is skipped' "$repo/notes.md" "$(dense 20 4)"

expect_over 'a dense change to a tracked file' "$repo/tracked.rs" "$(dense 8 24)"
git -C "$repo" checkout --quiet -- tracked.rs

mkdir -p "$repo/src/nested"
printf '%s\n' "$(dense 8 24)" >"$repo/src/nested/hooked.rs"
git -C "$repo" add src/nested/hooked.rs
git -C "$repo" commit --quiet -m 'seed a dense tracked file'
printf 'const ADDITION: i32 = 1;\n' >>"$repo/src/nested/hooked.rs"
set +e
(cd "$repo" && GIT_DIR="$repo/.git" bash "$checker" src/nested/hooked.rs) \
  >"$temp_dir/out" 2>"$temp_dir/err"
status=$?
set -e
if [ "$status" -ne 0 ] || [ -s "$temp_dir/err" ]; then
  echo "FAIL: the GIT_DIR a hook exports should not widen the diff to the whole file, got $status" >&2
  cat "$temp_dir/err" >&2
  failures=$((failures + 1))
fi
git -C "$repo" checkout --quiet -- src/nested/hooked.rs

printf '%s\n' "$(dense 8 24)" >"$repo/argv.rs"
run_argv "$repo/argv.rs"
if [ "$status" -ne 1 ]; then
  echo "FAIL: argv mode should exit 1 when over the cap, got $status" >&2
  failures=$((failures + 1))
fi

run_argv "$repo/sparse.rs"
if [ "$status" -ne 0 ]; then
  echo "FAIL: argv mode should exit 0 under the cap, got $status" >&2
  cat "$temp_dir/err" >&2
  failures=$((failures + 1))
fi

set +e
COMMENT_DENSITY_MAX=50 bash "$checker" "$repo/argv.rs" >"$temp_dir/out" 2>"$temp_dir/err"
status=$?
set -e
if [ "$status" -ne 0 ]; then
  echo 'FAIL: COMMENT_DENSITY_MAX should raise the cap' >&2
  failures=$((failures + 1))
fi

set +e
COMMENT_DENSITY_MAX=1 bash "$checker" "$repo/sparse.rs" >"$temp_dir/out" 2>"$temp_dir/err"
status=$?
set -e
if [ "$status" -ne 1 ]; then
  echo 'FAIL: COMMENT_DENSITY_MAX should lower the cap' >&2
  failures=$((failures + 1))
fi

# Codex apply_patch payloads carry no file path, so the whole working tree is scanned.
run_stdin
if [ "$status" -ne 2 ]; then
  echo "FAIL: a payload without a file path should scan the working tree, got $status" >&2
  failures=$((failures + 1))
fi

rm -f \
  "$repo/dense.rs" \
  "$repo/block.kt" \
  "$repo/argv.rs" \
  "$repo/ui/src/dense.rs"
run_stdin
if [ "$status" -ne 0 ] || [ -s "$temp_dir/err" ]; then
  echo "FAIL: a tree under the cap should pass silently, got $status" >&2
  cat "$temp_dir/err" >&2
  failures=$((failures + 1))
fi

space_path="$repo/path with space/dense.rs"
mkdir -p "$(dirname "$space_path")"
printf '%s\n' "$(dense 8 24)" >"$space_path"
run_stdin
if [ "$status" -ne 2 ]; then
  echo "FAIL: fallback scanning should handle paths with spaces, got $status" >&2
  failures=$((failures + 1))
fi
rm -f "$space_path"

if ((failures > 0)); then
  echo "$failures check(s) failed" >&2
  exit 1
fi

echo 'comment_density.sh behaves as documented.'
