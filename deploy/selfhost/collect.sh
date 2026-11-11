#!/usr/bin/env bash
# Poll configured official sources into the private ledger; do not publish findings.
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

: "${PNUL_REPO_DIR:?set PNUL_REPO_DIR to the checked-out repository}"
: "${PNUL_DATA_DIR:?set PNUL_DATA_DIR to the private collector state}"

pnull_bin="${PNUL_BIN:-/usr/local/bin/pnull}"
rules_path="${PNUL_RULES_PATH:-rules/surveillance.yml}"
matrix_path="${PNUL_COLLECT_MATRIX_PATH:-deploy/selfhost/collection-sources.tsv}"
fallback_path="${PNUL_COLLECT_FALLBACKS_PATH:-deploy/selfhost/collection-fallbacks.tsv}"

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
[[ "$fallback_path" == deploy/selfhost/* && "$fallback_path" != *..* && -f "$fallback_path" && ! -L "$fallback_path" ]] || {
    echo "collection fallback matrix must be a regular file under deploy/selfhost" >&2
    exit 1
}
if [[ -z "${PNUL_HELD_LOCKS:-}" ]]; then
    exec python3 "$script_dir/locked-run.py" "$PNUL_DATA_DIR/.collect.lock" -- bash "$script_dir/collect.sh"
fi
python3 "$script_dir/locked-run.py" --verify "$PNUL_DATA_DIR/.collect.lock"


# Validate the complete schedule before any source fetch. Source IDs are unique
# across profiles because the private ledger uses source IDs as global keys.
declare -a configs=() ids=() adapters=()
declare -A seen=() config_for=() adapter_for=() fallback_for=() fallback_seen=()
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
       ( "$adapter" == ingest || "$adapter" == procurement || "$adapter" == archive || "$adapter" == discover-calendar || "$adapter" == discover-index ) &&
       -z "${seen[$id]:-}" && -f "$config" && ! -L "$config" ]] || {
        echo "invalid or duplicate collection source: $id" >&2
        exit 1
    }
    seen[$id]=1
    config_for[$id]="$config"
    adapter_for[$id]="$adapter"
    configs+=("$config")
    ids+=("$id")
    adapters+=("$adapter")
done < "$matrix_path"
[[ ${#ids[@]} -gt 0 ]] || { echo "collection matrix has no sources" >&2; exit 1; }

# A fallback is bound to exactly one scheduled calendar and the same config.
# Validate the whole matrix before any CLI call, then use it only on failure.
while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" || "$line" == \#* ]] && continue
    [[ "$line" == *$'\t'* ]] || { echo "malformed fallback matrix row" >&2; exit 1; }
    config="${line%%$'\t'*}"
    rest="${line#*$'\t'}"
    [[ "$rest" == *$'\t'* ]] || { echo "malformed fallback matrix row" >&2; exit 1; }
    primary="${rest%%$'\t'*}"
    fallback="${rest#*$'\t'}"
    [[ "$config" =~ ^configs/states/co[-a-z0-9]*\.toml$ &&
       "$primary" =~ ^[a-z0-9-]+$ && "$fallback" =~ ^[a-z0-9-]+$ &&
       "$primary" != "$fallback" &&
       "${adapter_for[$primary]:-}" == discover-calendar &&
       "${config_for[$primary]:-}" == "$config" &&
       -z "${fallback_for[$primary]:-}" && -z "${fallback_seen[$fallback]:-}" &&
       -z "${seen[$fallback]:-}" ]] || {
        echo "invalid or duplicate collection fallback: $fallback" >&2
        exit 1
    }
    fallback_for[$primary]="$fallback"
    fallback_seen[$fallback]=1
done < "$fallback_path"

python3 "$script_dir/validate-collection-matrix.py" "$PNUL_REPO_DIR" "$matrix_path" "$fallback_path"

run_api_fallback() {
    local primary="$1" config="$2" fallback="${fallback_for[$1]:-}"
    if [[ -z "$fallback" ]]; then
        echo "calendar $primary failed; no API discovery fallback is configured" >&2
        return 0
    fi
    echo "calendar $primary failed; attempting reviewed API fallback $fallback" >&2
    if "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config" source review verify "$fallback"; then
        "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config" ingest --source-id "$fallback" || return 1
    else
        return 1
    fi
}

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
        elif [[ "$adapter" == discover-index ]]; then
            "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config" source discover-index "$source" || status=1
        else
            if ! "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config" source discover-calendar "$source"; then
                status=1
                run_api_fallback "$source" "$config" || status=1
            fi
        fi
    else
        status=1
        if [[ "$adapter" == discover-calendar ]]; then
            run_api_fallback "$source" "$config" || status=1
        fi
    fi
done
"$pnull_bin" --data-dir "$PNUL_DATA_DIR" --rules "$rules_path" scan || status=1
exit "$status"
