#!/usr/bin/env bash
# Poll configured official sources into the private ledger; do not publish findings.
set -euo pipefail

: "${PNUL_REPO_DIR:?set PNUL_REPO_DIR to the checked-out repository}"
: "${PNUL_DATA_DIR:?set PNUL_DATA_DIR to the private collector state}"

pnull_bin="${PNUL_BIN:-/usr/local/bin/pnull}"
rules_path="${PNUL_RULES_PATH:-rules/surveillance.yml}"
matrix_path="${PNUL_COLLECT_MATRIX_PATH:-deploy/selfhost/collection-sources.tsv}"

[[ "$PNUL_REPO_DIR" == /* && "$PNUL_DATA_DIR" == /* ]] || {
    echo "repository and private state paths must be absolute" >&2
    exit 1
}
[[ -d "$PNUL_DATA_DIR" ]] || { echo "private collector state is missing" >&2; exit 1; }
cd "$PNUL_REPO_DIR"
[[ "$matrix_path" == deploy/selfhost/* && "$matrix_path" != *..* && -f "$matrix_path" && ! -L "$matrix_path" ]] || {
    echo "collection matrix must be a regular file under deploy/selfhost" >&2
    exit 1
}
exec 9>"$PNUL_DATA_DIR/.collect.lock"
flock -n 9 || { echo "another collection is running" >&2; exit 1; }

# Validate the complete schedule before any source fetch. Source IDs are unique
# across profiles because the private ledger uses source IDs as global keys.
declare -a configs=() ids=() adapters=()
declare -A seen=()
while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" || "$line" == \#* ]] && continue
    [[ "$line" == *$'\t'* ]] || { echo "malformed collection matrix row" >&2; exit 1; }
    config="${line%%$'\t'*}"
    rest="${line#*$'\t'}"
    [[ "$rest" == *$'\t'* ]] || { echo "malformed collection matrix row" >&2; exit 1; }
    id="${rest%%$'\t'*}"
    adapter="${rest#*$'\t'}"
    [[ "$config" =~ ^configs/states/co[-a-z0-9]*\.toml$ &&
       "$id" =~ ^[a-z0-9-]+$ &&
       ( "$adapter" == ingest || "$adapter" == procurement || "$adapter" == archive || "$adapter" == discover-calendar ) &&
       -z "${seen[$id]:-}" && -f "$config" && ! -L "$config" ]] || {
        echo "invalid or duplicate collection source: $id" >&2
        exit 1
    }
    seen[$id]=1
    configs+=("$config")
    ids+=("$id")
    adapters+=("$adapter")
done < "$matrix_path"
[[ ${#ids[@]} -gt 0 ]] || { echo "collection matrix has no sources" >&2; exit 1; }

status=0
for index in "${!ids[@]}"; do
    config="${configs[$index]}"
    source="${ids[$index]}"
    adapter="${adapters[$index]}"
    if "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config" source review verify "$source"; then
        if [[ "$adapter" == ingest ]]; then
            "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config" ingest --source-id "$source" || status=1
        elif [[ "$adapter" == procurement ]]; then
            "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config" procurement refresh "$source" --live || status=1
        elif [[ "$adapter" == archive ]]; then
            "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config" source archive "$source" || status=1
        else
            "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config" source discover-calendar "$source" || status=1
        fi
    else
        status=1
    fi
done
"$pnull_bin" --data-dir "$PNUL_DATA_DIR" --rules "$rules_path" scan || status=1
exit "$status"
