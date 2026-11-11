#!/usr/bin/env bash
set -euo pipefail

test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT

cat >"$test_dir/nix" <<'NIX'
#!/usr/bin/env bash
case "$PNULL_NIX_FAKE_MODE" in
    success)
        echo 'flake check passed'
        exit 0
        ;;
    build_failure)
        echo 'error: Rust compilation failed' >&2
        exit 42
        ;;
    transient_once)
        if [ ! -e "$PNULL_NIX_FAKE_COUNT_FILE" ]; then
            touch "$PNULL_NIX_FAKE_COUNT_FILE"
            echo 'error: static.crates.io download HTTP error 503' >&2
            exit 77
        fi
        echo 'flake check passed after retry'
        exit 0
        ;;
    transient_always)
        echo 'error: static.crates.io download HTTP error 503' >&2
        exit 77
        ;;
esac
exit 99
NIX
cat >"$test_dir/sleep" <<'SLEEP'
#!/usr/bin/env bash
exit 0
SLEEP
chmod +x "$test_dir/nix" "$test_dir/sleep"

check_case() {
    local mode="$1" expected="$2" expected_text="$3" actual
    rm -f "$test_dir/count"
    if PATH="$test_dir:$PATH" PNULL_NIX_FAKE_MODE="$mode" \
        PNULL_NIX_FAKE_COUNT_FILE="$test_dir/count" PNULL_NIX_RETRY_ATTEMPTS=2 \
        bash .github/scripts/retry-nix-flake-check.sh >"$test_dir/output" 2>&1; then
        actual=0
    else
        actual=$?
    fi
    if [ "$actual" -ne "$expected" ] || ! grep -Fq "$expected_text" "$test_dir/output"; then
        cat "$test_dir/output" >&2
        echo "retry gate: $mode returned $actual, expected $expected with '$expected_text'" >&2
        exit 1
    fi
}

check_case success 0 'succeeded on attempt 1'
check_case build_failure 42 'failed (non-download error, not retried)'
check_case transient_once 0 'succeeded on attempt 2'
check_case transient_always 1 'failed after 2 attempts'
echo 'Nix retry gate behavior passed.'
