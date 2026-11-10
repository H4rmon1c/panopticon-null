#!/usr/bin/env bash
# Publish only the approved static atlas. The collector database is never copied.
set -euo pipefail

: "${PNUL_REPO_DIR:?set PNUL_REPO_DIR to the checked-out repository}"
: "${PNUL_DATA_DIR:?set PNUL_DATA_DIR to the private collector state}"
: "${PNUL_SITE_URL:?set PNUL_SITE_URL to the public HTTPS origin}"
: "${PNUL_RELEASE_ROOT:?set PNUL_RELEASE_ROOT to the public releases directory}"
: "${PNUL_CURRENT_LINK:?set PNUL_CURRENT_LINK to the public current symlink}"

pnull_bin="${PNUL_BIN:-/usr/local/bin/pnull}"
node_bin="${PNUL_NODE_BIN:-node}"
config_path="${PNUL_CONFIG_PATH:-configs/states/co.toml}"
manifest_path="${PNUL_MANIFEST_PATH:-fixtures/atlas/colorado-springs-25-93.v1.json}"

for path in "$PNUL_REPO_DIR" "$PNUL_DATA_DIR" "$PNUL_RELEASE_ROOT" "$PNUL_CURRENT_LINK"; do
    [[ "$path" == /* ]] || { echo "deployment paths must be absolute: $path" >&2; exit 1; }
done
[[ -d "$PNUL_REPO_DIR/web/panopticon-terminal" ]] || { echo "web source is missing" >&2; exit 1; }
[[ -d "$PNUL_DATA_DIR" ]] || { echo "private collector state is missing" >&2; exit 1; }
[[ -e "$PNUL_CURRENT_LINK" && ! -L "$PNUL_CURRENT_LINK" ]] && {
    echo "current path exists and is not a symlink" >&2
    exit 1
}

private_root="$(realpath -m "$PNUL_DATA_DIR")"
release_root="$(realpath -m "$PNUL_RELEASE_ROOT")"
public_root="$(realpath -m "$(dirname "$PNUL_CURRENT_LINK")")"
current_link="$public_root/$(basename "$PNUL_CURRENT_LINK")"
case "$private_root/" in "$release_root/"*) echo "collector state is inside the public release root" >&2; exit 1 ;; esac
case "$release_root/" in "$private_root/"*) echo "public release root is inside collector state" >&2; exit 1 ;; esac
case "$private_root/" in "$public_root/"*) echo "collector state is inside the public site directory" >&2; exit 1 ;; esac
case "$public_root/" in "$private_root/"*) echo "public site directory is inside collector state" >&2; exit 1 ;; esac

mkdir -p "$release_root" "$(dirname "$current_link")"
exec 9>"$release_root/.publish.lock"
flock -n 9 || { echo "another publish is running" >&2; exit 1; }

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
    --site-url "$PNUL_SITE_URL" atlas export --manifest "$manifest_path" \
    --output "$stage/public/public-snapshot.json"
"$node_bin" "$stage/scripts/validate.mjs"
"$node_bin" --test "$stage/scripts/test-client.mjs"

# These two operator scripts are needed for staging validation, not for browsers.
rm -- "$stage/scripts/validate.mjs" "$stage/scripts/test-client.mjs"
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
