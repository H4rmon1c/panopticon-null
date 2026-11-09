#!/usr/bin/env bash
# Poll configured official sources into the private ledger; do not publish findings.
set -euo pipefail

: "${PNUL_REPO_DIR:?set PNUL_REPO_DIR to the checked-out repository}"
: "${PNUL_DATA_DIR:?set PNUL_DATA_DIR to the private collector state}"

pnull_bin="${PNUL_BIN:-/usr/local/bin/pnull}"
config_path="${PNUL_CONFIG_PATH:-configs/states/co.toml}"
rules_path="${PNUL_RULES_PATH:-rules/surveillance.yml}"
ingest_sources="${PNUL_INGEST_SOURCE_IDS:-colorado-springs-legistar-events}"
procurement_sources="${PNUL_PROCUREMENT_SOURCE_IDS:-colorado-springs-contract-awards colorado-springs-solicitation-mirror}"

[[ "$PNUL_REPO_DIR" == /* && "$PNUL_DATA_DIR" == /* ]] || {
    echo "repository and private state paths must be absolute" >&2
    exit 1
}
[[ -d "$PNUL_DATA_DIR" ]] || { echo "private collector state is missing" >&2; exit 1; }
cd "$PNUL_REPO_DIR"
exec 9>"$PNUL_DATA_DIR/.collect.lock"
flock -n 9 || { echo "another collection is running" >&2; exit 1; }

status=0
read -r -a ingest_ids <<< "$ingest_sources"
read -r -a procurement_ids <<< "$procurement_sources"
for source in "${ingest_ids[@]}"; do
    if "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config_path" source review verify "$source"; then
        "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config_path" ingest --source-id "$source" || status=1
    else
        status=1
    fi
done
for source in "${procurement_ids[@]}"; do
    if "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config_path" source review verify "$source"; then
        "$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config_path" procurement refresh "$source" --live || status=1
    else
        status=1
    fi
done
"$pnull_bin" --data-dir "$PNUL_DATA_DIR" --rules "$rules_path" scan || status=1
exit "$status"
