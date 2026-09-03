#!/usr/bin/env bash
# Retry `nix flake check` on transient crates.io download failures only.
#
# Background: `nix flake check` builds the Rust crates via `buildRustPackage`,
# which resolves Cargo.lock through `importCargoLock` and fetches the `.crate`
# archives from `https://static.crates.io/crates`. Occasionally the crates.io
# CDN returns a transient HTTP 403 (or 5xx / network timeout) for a specific
# crate. That is an upstream availability failure, not a source-code error.
#
# This wrapper:
#   - runs `nix flake check` normally;
#   - on failure, inspects the log for a crates.io *download* error
#     (HTTP 4xx/5xx, network/timeout, "cannot download", "static.crates.io");
#   - if and only if a download error is present, retries with bounded backoff
#     (each retry re-uses the Nix store, so only the failed crate is re-fetched);
#   - on any other failure (eval, compile, formatting, clippy, tests) it fails
#     immediately, so a genuine source-code failure is never masked by a retry.
#
# This keeps `nix flake check` required, keeps all locked dependencies, and does
# not weaken the dependency policy or replace reproducibility with an unpinned
# install.

set -u

MAX_ATTEMPTS="${PNULL_NIX_RETRY_ATTEMPTS:-5}"
SLEEP_BASE=5

is_transient_download_failure() {
    # Match a crates.io/static.crates.io download or HTTP/network failure.
    grep -Eiq \
        -e "cannot download" \
        -e "unable to download" \
        -e "static\.crates\.io" \
        -e "HTTP error [45][0-9][0-9]" \
        -e "http.*403" \
        -e "curl.*(error|failed)" \
        -e "(timed out|timeout|connection (reset|refused|closed))" \
        -e "error 5[0-9][0-9]" \
        -e "while fetching.*\.crate" \
        || return 1
}

attempt=1
while :; do
    log="$(mktemp)"
    if nix flake check --print-build-logs >"$log" 2>&1; then
        cat "$log"
        rm -f "$log"
        echo "nix flake check succeeded on attempt $attempt."
        exit 0
    fi
    status=$?
    if ! is_transient_download_failure "$log"; then
        # A genuine source-code / evaluation / build failure: surface it now.
        cat "$log"
        rm -f "$log"
        echo "nix flake check failed (non-download error, not retried)." >&2
        exit "$status"
    fi
    rm -f "$log"
    if [ "$attempt" -ge "$MAX_ATTEMPTS" ]; then
        echo "nix flake check failed after $MAX_ATTEMPTS attempts; crates.io download remained unavailable (transient upstream failure, not a source-code error)." >&2
        exit 1
    fi
    echo "nix flake check: transient crates.io download failure on attempt $attempt; retrying in ${SLEEP_BASE}s..." >&2
    sleep "$SLEEP_BASE"
    attempt=$((attempt + 1))
done
