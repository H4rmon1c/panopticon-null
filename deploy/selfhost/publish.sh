#!/usr/bin/env bash
# Publish only the approved static atlas. The collector database is never copied.
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

: "${PNUL_REPO_DIR:?set PNUL_REPO_DIR to the checked-out repository}"
: "${PNUL_DATA_DIR:?set PNUL_DATA_DIR to the private collector state}"
: "${PNUL_SITE_URL:?set PNUL_SITE_URL to the public HTTPS origin}"
: "${PNUL_RELEASE_ROOT:?set PNUL_RELEASE_ROOT to the public releases directory}"
: "${PNUL_CURRENT_LINK:?set PNUL_CURRENT_LINK to the public current symlink}"

pnull_bin="${PNUL_BIN:-/usr/local/bin/pnull}"
node_bin="${PNUL_NODE_BIN:-node}"
config_path="${PNUL_CONFIG_PATH:-configs/states/co.toml}"
manifest_paths="${PNUL_MANIFEST_PATHS:-${PNUL_MANIFEST_PATH:-fixtures/atlas/colorado-springs-25-93.v1.json}}"
withdrawn_paths="${PNUL_WITHDRAWN_MANIFESTS:-}"
proposal_paths="${PNUL_PROPOSAL_INPUTS:-}"
withdrawn_case_ids="${PNUL_WITHDRAWN_CASE_IDS:-}"

for path in "$PNUL_REPO_DIR" "$PNUL_DATA_DIR" "$PNUL_RELEASE_ROOT" "$PNUL_CURRENT_LINK"; do
    [[ "$path" == /* ]] || { echo "deployment paths must be absolute: $path" >&2; exit 1; }
done
[[ -d "$PNUL_REPO_DIR/web/panopticon-terminal" ]] || { echo "web source is missing" >&2; exit 1; }
[[ -d "$PNUL_DATA_DIR" ]] || { echo "private collector state is missing" >&2; exit 1; }

read -r -a manifest_list <<< "$manifest_paths"
[[ ${#manifest_list[@]} -gt 0 ]] || { echo "no atlas manifests configured" >&2; exit 1; }
manifest_args=()
declare -A seen_manifests=()
for manifest in "${manifest_list[@]}"; do
    [[ "$manifest" =~ ^fixtures/atlas/[a-zA-Z0-9._/-]+\.json$ &&
       "$manifest" != *..* && -z "${seen_manifests[$manifest]:-}" &&
       -f "$PNUL_REPO_DIR/$manifest" && ! -L "$PNUL_REPO_DIR/$manifest" ]] || {
        echo "invalid or duplicate atlas manifest: $manifest" >&2
        exit 1
    }
    seen_manifests[$manifest]=1
    manifest_args+=(--manifest "$manifest")
done
withdrawn_args=()
if [[ -n "$withdrawn_paths" ]]; then
    read -r -a withdrawn_list <<< "$withdrawn_paths"
    for manifest in "${withdrawn_list[@]}"; do
        [[ "$manifest" =~ ^fixtures/atlas/[a-zA-Z0-9._/-]+\.json$ &&
           "$manifest" != *..* && -z "${seen_manifests[$manifest]:-}" &&
           -f "$PNUL_REPO_DIR/$manifest" && ! -L "$PNUL_REPO_DIR/$manifest" ]] || {
            echo "invalid or active withdrawn manifest: $manifest" >&2
            exit 1
        }
        seen_manifests[$manifest]=1
        withdrawn_args+=(--withdrawn-manifest "$manifest")
    done
fi
[[ -e "$PNUL_CURRENT_LINK" && ! -L "$PNUL_CURRENT_LINK" ]] && {
    echo "current path exists and is not a symlink" >&2
    exit 1
}

private_root="$(realpath -m "$PNUL_DATA_DIR")"
proposal_args=()
if [[ -n "$proposal_paths" ]]; then
    read -r -a proposal_list <<< "$proposal_paths"
    [[ ${#proposal_list[@]} -le 64 ]] || { echo "too many proposal inputs" >&2; exit 1; }
    declare -A seen_proposals=()
    for proposal in "${proposal_list[@]}"; do
        [[ "$proposal" == "$private_root/"* && "$proposal" == *.json &&
           "$proposal" != *..* && -z "${seen_proposals[$proposal]:-}" &&
           -f "$proposal" && ! -L "$proposal" ]] || {
            echo "invalid or duplicate private proposal input: $proposal" >&2
            exit 1
        }
        canonical_proposal="$(realpath -e -- "$proposal")"
        [[ "$canonical_proposal" == "$private_root/"* ]] || {
            echo "proposal input escapes private state: $proposal" >&2
            exit 1
        }
        seen_proposals[$proposal]=1
        proposal_args+=(--proposal-input "$proposal")
    done
fi
release_root="$(realpath -m "$PNUL_RELEASE_ROOT")"
public_root="$(realpath -m "$(dirname "$PNUL_CURRENT_LINK")")"
current_link="$public_root/$(basename "$PNUL_CURRENT_LINK")"
case "$private_root/" in "$release_root/"*) echo "collector state is inside the public release root" >&2; exit 1 ;; esac
case "$release_root/" in "$private_root/"*) echo "public release root is inside collector state" >&2; exit 1 ;; esac
case "$private_root/" in "$public_root/"*) echo "collector state is inside the public site directory" >&2; exit 1 ;; esac
case "$public_root/" in "$private_root/"*) echo "public site directory is inside collector state" >&2; exit 1 ;; esac

mkdir -p "$release_root" "$(dirname "$current_link")"
if [[ -z "${PNUL_HELD_LOCKS:-}" ]]; then
    exec python3 "$script_dir/locked-run.py" "$release_root/.publish.lock" "$PNUL_DATA_DIR/.collect.lock" -- bash "$script_dir/publish.sh"
fi
python3 "$script_dir/locked-run.py" --verify "$release_root/.publish.lock" "$PNUL_DATA_DIR/.collect.lock"

prior_api_args=()
if [[ -L "$current_link" ]]; then
    prior_release="$(realpath -e -- "$current_link")"
    [[ "$prior_release" == "$release_root/"* ]] || { echo "current release escapes release root" >&2; exit 1; }
    if [[ -e "$prior_release/api/v1/alerts.json" ]]; then
        [[ -f "$prior_release/api/v1/alerts.json" && ! -L "$prior_release/api/v1/alerts.json" ]] || { echo "prior alert API is unsafe" >&2; exit 1; }
        prior_api_args+=("$prior_release/api/v1/alerts.json")
    fi
fi
prior_withdrawal_args=()
if [[ -n "$withdrawn_case_ids" ]]; then
    [[ -L "$current_link" ]] || { echo "prior withdrawals need a current release" >&2; exit 1; }
    prior_release="$(realpath -e -- "$current_link")"
    [[ "$prior_release" == "$release_root/"* ]] || { echo "current release escapes release root" >&2; exit 1; }
    prior_snapshot="$prior_release/public/public-snapshot.json"
    [[ -f "$prior_snapshot" && ! -L "$prior_snapshot" ]] || { echo "prior public snapshot missing" >&2; exit 1; }
    read -r -a withdrawn_id_list <<< "$withdrawn_case_ids"
    declare -A seen_withdrawn_ids=()
    for case_id in "${withdrawn_id_list[@]}"; do
        [[ "$case_id" =~ ^co-[a-z0-9-]+$ && -z "${seen_withdrawn_ids[$case_id]:-}" ]] || {
            echo "invalid or duplicate withdrawn case ID: $case_id" >&2
            exit 1
        }
        seen_withdrawn_ids[$case_id]=1
        prior_withdrawal_args+=(--withdrawn-case-id "$case_id")
    done
    prior_withdrawal_args+=(--previous-snapshot "$prior_snapshot")
fi
stage="$(mktemp -d "$release_root/.candidate.XXXXXXXX")"
cleanup() {
    if [[ -n "$stage" && -d "$stage" ]]; then
        rm -rf -- "$stage"
    fi
}
trap cleanup EXIT

web="$(realpath "$PNUL_REPO_DIR/web/panopticon-terminal")"
asset_manifest="$(dirname -- "$(realpath -e -- "${BASH_SOURCE[0]}")")/public-assets.txt"
[[ -f "$asset_manifest" ]] || { echo "public asset manifest is missing" >&2; exit 1; }
while IFS= read -r asset || [[ -n "$asset" ]]; do
    [[ -z "$asset" || "$asset" == \#* ]] && continue
    if [[ ! "$asset" =~ ^[a-zA-Z0-9][a-zA-Z0-9._/-]*$ ]] ||
       [[ "$asset" == *".."* ]] || [[ "$asset" == *"//"* ]]; then
        echo "invalid public asset path: $asset" >&2
        exit 1
    fi
    source_path="$(realpath -e "$web/$asset")"
    if [[ "$source_path" != "$web/"* || ! -f "$source_path" || -L "$web/$asset" ]]; then
        echo "public asset escapes the web source or is not a regular file: $asset" >&2
        exit 1
    fi
    mkdir -p -- "$stage/$(dirname -- "$asset")"
    cp -- "$source_path" "$stage/$asset"
done < "$asset_manifest"
mkdir -p -- "$stage/public"
research_dir="$PNUL_DATA_DIR/research"
if [[ -e "$research_dir/research.sqlite3" || -L "$research_dir/research.sqlite3" ]]; then
    [[ -d "$research_dir" && ! -L "$research_dir" && -f "$research_dir/research.sqlite3" && ! -L "$research_dir/research.sqlite3" ]] || {
        echo "research state must be a real private directory and database" >&2; exit 1;
    }
    python3 "$PNUL_REPO_DIR/scripts/colorado-research.py" --data-dir "$research_dir" \
        export --output "$stage/public/research-signals.json" --assets-dir "$stage/research-portraits"
    python3 "$PNUL_REPO_DIR/scripts/garden-city-motion-sidecar.py" --data-dir "$research_dir" \
        --output "$stage/public/garden-city-motion-v1.json" --research-snapshot "$stage/public/research-signals.json"
    python3 "$PNUL_REPO_DIR/scripts/keenesburg-motion-sidecar.py" --data-dir "$research_dir" \
        --output "$stage/public/keenesburg-motion-v1.json" --research-snapshot "$stage/public/research-signals.json"
    python3 "$PNUL_REPO_DIR/scripts/cortez-motion-sidecar.py" --data-dir "$research_dir" \
        --output "$stage/public/cortez-motion-v1.json" --research-snapshot "$stage/public/research-signals.json"
    python3 "$PNUL_REPO_DIR/scripts/yuma-motion-sidecar.py" --data-dir "$research_dir" \
        --output "$stage/public/yuma-motion-v1.json" --research-snapshot "$stage/public/research-signals.json"
else
    printf '{"schema_version":"pnull-yuma-motion-v1","state":"not_available"}\n' > "$stage/public/yuma-motion-v1.json"
    printf '{"schema_version":"pnull-keenesburg-motion-v1","state":"not_available"}\n' > "$stage/public/keenesburg-motion-v1.json"
    printf '{"schema_version":"pnull-cortez-motion-v1","state":"not_available"}\n' > "$stage/public/cortez-motion-v1.json"
    printf '{"schema_version":"pnull-garden-city-motion-v1","state":"not_available"}\n' > "$stage/public/garden-city-motion-v1.json"
fi
# The vote card remains available independently, but the historical image is
# withheld unless its own source PDF, capture receipt and embedded bytes verify.
if ! grep -Fq '"state":"available"' "$stage/public/cortez-motion-v1.json" ||
   ! python3 "$PNUL_REPO_DIR/scripts/verify-cortez-lucero-portrait.py" --archive "$research_dir"; then
    rm -f -- "$stage/assets/portraits/cortez-orly-lucero-2017.jpg"
fi
# Each press portrait is withheld independently if its exact captioned source
# and image receipts are absent; the vote remains available.
press_names=('Amy Huckins' 'Rachel Medina' 'Robert Dobry')
press_files=('cortez-amy-huckins-2021.jpg' 'cortez-rachel-medina-2022.jpg' 'cortez-robert-dobry-2020.jpg')
for i in "${!press_names[@]}"; do
    if ! grep -Fq '"state":"available"' "$stage/public/cortez-motion-v1.json" ||
       ! python3 "$PNUL_REPO_DIR/scripts/verify-cortez-press-portraits.py" --archive "$research_dir" --person "${press_names[$i]}" >/dev/null 2>&1; then
        rm -f -- "$stage/assets/portraits/${press_files[$i]}"
    fi
done
if [[ -n "$(find "$stage" -type l -print -quit)" ]]; then
    echo "site assets contain a symlink" >&2
    exit 1
fi
if [[ -e "$stage/preview-snapshot.json" ]]; then
    echo "private preview was copied into the public release" >&2
    exit 1
fi

cd "$PNUL_REPO_DIR"
"$pnull_bin" --data-dir "$PNUL_DATA_DIR" --config "$config_path" \
    --site-url "$PNUL_SITE_URL" atlas export "${manifest_args[@]}" "${withdrawn_args[@]}" "${prior_withdrawal_args[@]}" "${proposal_args[@]}" \
    --output "$stage/public/public-snapshot.json"
PNUL_PORTRAIT_PROOF_ROOT="$PNUL_REPO_DIR" "$node_bin" "$stage/scripts/validate.mjs"
"$node_bin" "$stage/scripts/filter-portraits.mjs" "$stage"
"$node_bin" "$stage/scripts/build-decision-pages.mjs" "$stage" "$PNUL_SITE_URL"
PNUL_PORTRAIT_PROOF_ROOT="$PNUL_REPO_DIR" PNUL_VALIDATE_RELEASE=1 "$node_bin" "$stage/scripts/validate.mjs"
PNUL_VALIDATE_RELEASE=1 "$node_bin" --test "$stage/scripts/test-client.mjs"
"$node_bin" "$stage/scripts/build-alert-api.mjs" "$stage" "$PNUL_SITE_URL" "${prior_api_args[@]}"
"$node_bin" "$PNUL_REPO_DIR/web/panopticon-terminal/scripts/build-coverage-api.mjs" "$stage"
cp -- "$stage/public/research-signals.json" "$stage/api/v1/research.json"

# Operator scripts are needed for staging validation, not for browsers.
rm -- "$stage/scripts/build-alert-api.mjs" "$stage/scripts/build-decision-pages.mjs" "$stage/scripts/filter-portraits.mjs" "$stage/scripts/validate.mjs" "$stage/scripts/validate-embedded-portrait.mjs" "$stage/scripts/test-client.mjs"
find "$stage" -type d -exec chmod 755 {} +
find "$stage" -type f -exec chmod 644 {} +

release="$release_root/release-$(date -u +%Y%m%dT%H%M%SZ)-$(basename "$stage" | cut -d. -f3)"
mv -- "$stage" "$release"
stage=""
next_link="$current_link.next.$$"
trap 'rm -f -- "$next_link"; cleanup' EXIT
ln -s -- "$release" "$next_link"
mv -Tf -- "$next_link" "$current_link"
echo "published $release"
