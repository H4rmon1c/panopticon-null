#!/usr/bin/env bash
# No fetches or configuration writes. Run as panopticon with the environment loaded.
set -euo pipefail
: "${PNUL_REPO_DIR:?load /etc/panopticon/panopticon.env}"
: "${PNUL_DATA_DIR:?load /etc/panopticon/panopticon.env}"
: "${PNUL_SITE_URL:?load /etc/panopticon/panopticon.env}"
[[ "$(id -un)" == panopticon ]] || { echo 'run preflight as the panopticon service account' >&2; exit 1; }
[[ "$PNUL_SITE_URL" == https://panopticon.fail ]] || { echo 'review canonical hostname and generated metadata before changing it' >&2; exit 1; }
for binary in python3 node caddy bwrap pdftotext flock realpath; do command -v "$binary" >/dev/null; done
pnull_binary="${PNUL_BIN:-/usr/local/bin/pnull}"
[[ -x "$pnull_binary" && ! -L "$pnull_binary" && "$(stat -c %U "$pnull_binary")" == root && -z "$(find "$pnull_binary" -perm /022 -print)" ]]
[[ -f /etc/panopticon/panopticon.env && ! -L /etc/panopticon/panopticon.env && "$(stat -c %U /etc/panopticon/panopticon.env)" == root && -z "$(find /etc/panopticon/panopticon.env -perm /022 -print)" ]]
node -e 'if(Number(process.versions.node.split(".")[0])<22)process.exit(1)'
[[ -z "$(find "$PNUL_REPO_DIR" -xdev \( ! -user root -o -perm /022 \) -print -quit)" ]] || { echo 'mutable repository' >&2; exit 1; }
[[ -f "$PNUL_DATA_DIR/pnull.db" && ! -L "$PNUL_DATA_DIR/pnull.db" ]]
[[ "$(stat -c %a "$PNUL_DATA_DIR")" == 700 && "$(stat -c %U "$PNUL_DATA_DIR")" == panopticon ]]
cd "$PNUL_REPO_DIR"
caddy validate --config web/panopticon-terminal/deploy/Caddyfile.example --adapter caddyfile
status=0
count=0
while IFS=$'\t' read -r config source adapter; do
    [[ -z "$config" || "$config" == \#* ]] && continue
    count=$((count+1))
    "${PNUL_BIN:-/usr/local/bin/pnull}" --data-dir "$PNUL_DATA_DIR" --config "$config" source review verify "$source" || status=1
done < "${PNUL_COLLECT_MATRIX_PATH:-deploy/selfhost/collection-release-sources.tsv}"
[[ "$count" -gt 0 ]] || exit 1
[[ "$status" == 0 ]] || { echo 'source review is missing, expired, or mismatched; see CLI errors above' >&2; exit 1; }
printf '%s\n' 'Read-only preflight passed. Actual service sandbox extraction, backup restore, HTTPS, and timer activation still require host tests.'
