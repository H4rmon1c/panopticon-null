#!/usr/bin/env bash
# Follow only digest-bound official document candidates into private state.
set -euo pipefail

: "${PNUL_REPO_DIR:?set PNUL_REPO_DIR to the checked-out repository}"
: "${PNUL_DATA_DIR:?set PNUL_DATA_DIR to the private collector state}"
pnull_bin="${PNUL_BIN:-/usr/local/bin/pnull}"
matrix_path="${PNUL_CANDIDATE_MATRIX_PATH:-deploy/selfhost/candidate-queues.tsv}"

[[ "$PNUL_REPO_DIR" == /* && "$PNUL_DATA_DIR" == /* && -d "$PNUL_DATA_DIR" ]] || {
    echo "repository and private state must be absolute existing paths" >&2
    exit 1
}
cd "$PNUL_REPO_DIR"
[[ "$matrix_path" == deploy/selfhost/* && "$matrix_path" != *..* &&
   -f "$matrix_path" && ! -L "$matrix_path" ]] || {
    echo "candidate queue matrix must be a regular file under deploy/selfhost" >&2
    exit 1
}

# The calendar collector takes this same lock while writing candidate files.
exec 9>"$PNUL_DATA_DIR/.collect.lock"
flock -n 9 || { echo "another collection is running" >&2; exit 1; }

declare -a configs=() origins=() maxima=() refreshes=()
declare -A seen=()
while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" || "$line" == \#* ]] && continue
    rest="$line"
    [[ "$rest" == *$'\t'* ]] || { echo "malformed candidate queue row" >&2; exit 1; }
    config="${rest%%$'\t'*}"; rest="${rest#*$'\t'}"
    [[ "$rest" == *$'\t'* ]] || { echo "malformed candidate queue row" >&2; exit 1; }
    origin="${rest%%$'\t'*}"; rest="${rest#*$'\t'}"
    [[ "$rest" == *$'\t'* ]] || { echo "malformed candidate queue row" >&2; exit 1; }
    maximum="${rest%%$'\t'*}"; refresh="${rest#*$'\t'}"
    [[ "$refresh" != *$'\t'* && "$config" =~ ^configs/states/co[-a-z0-9]*\.toml$ &&
       "$origin" =~ ^[a-z0-9-]+$ && -z "${seen[$origin]:-}" &&
       "$maximum" =~ ^[1-8]$ && "$refresh" =~ ^[1-9][0-9]*$ &&
       ${#refresh} -le 8 && "$refresh" -le 31536000 && -f "$config" && ! -L "$config" ]] || {
        echo "invalid or duplicate candidate queue row" >&2
        exit 1
    }
    seen[$origin]=1
    configs+=("$config")
    origins+=("$origin")
    maxima+=("$maximum")
    refreshes+=("$refresh")
done < "$matrix_path"

if [[ ${#origins[@]} -eq 0 ]]; then
    echo "no reviewed candidate queues enabled"
    exit 0
fi
status=0
for index in "${!origins[@]}"; do
    "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "${configs[$index]}" source archive-queue \
        "${origins[$index]}" --max-fetches "${maxima[$index]}" \
        --refresh-after-seconds "${refreshes[$index]}" || status=1
done
exit "$status"
