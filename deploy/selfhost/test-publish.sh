#!/usr/bin/env bash
set -euo pipefail

for unit in panopticon-collect.service panopticon-publish.service; do
  grep -Fxq 'StateDirectoryMode=0700' "$(dirname "$0")/$unit" || {
    echo "private state mode missing from $unit" >&2
    exit 1
  }
done

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="$(cd -- "$script_dir/../.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT

mkdir -p "$scratch/source/web" "$scratch/source/fixtures/atlas" "$scratch/private" "$scratch/site"
cp -a "$repo_dir/web/panopticon-terminal" "$scratch/source/web/"
cp -- "$repo_dir/fixtures/atlas/colorado-springs-25-93.v1.json" "$scratch/source/fixtures/atlas/"
printf '%s\n' 'PRIVATE PREVIEW' > "$scratch/source/web/panopticon-terminal/preview-snapshot.json"
printf '%s\n' 'PRIVATE DATABASE' > "$scratch/private/pnull.db"

cat > "$scratch/fake-pnull" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "${PNUL_TEST_FAIL:-0}" != 1 ]] || exit 23
while (($#)); do
    if [[ "$1" == --output ]]; then
        shift
        cp -- "$PNUL_TEST_SNAPSHOT" "$1"
        exit 0
    fi
    shift
done
exit 1
EOF
chmod +x "$scratch/fake-pnull"

export PNUL_REPO_DIR="$scratch/source"
export PNUL_DATA_DIR="$scratch/private"
export PNUL_SITE_URL=https://panopticon.fail
export PNUL_RELEASE_ROOT="$scratch/site/releases"
export PNUL_CURRENT_LINK="$scratch/site/current"
export PNUL_BIN="$scratch/fake-pnull"
export PNUL_TEST_SNAPSHOT="$repo_dir/web/panopticon-terminal/public/public-snapshot.json"

"$script_dir/publish.sh"
[[ -L "$PNUL_CURRENT_LINK" ]]
[[ -f "$PNUL_CURRENT_LINK/public/public-snapshot.json" ]]
[[ -f "$PNUL_CURRENT_LINK/portraits/tom-bailey.jpg" ]]
[[ -f "$PNUL_CURRENT_LINK/portraits/portraits.js" ]]
[[ ! -e "$PNUL_CURRENT_LINK/preview-snapshot.json" ]]
[[ ! -e "$PNUL_CURRENT_LINK/pnull.db" ]]
[[ ! -e "$PNUL_CURRENT_LINK/scripts/validate.mjs" ]]
[[ ! -e "$PNUL_CURRENT_LINK/scripts/test-client.mjs" ]]
[[ ! -e "$PNUL_CURRENT_LINK/scripts/build-decision-pages.mjs" ]]
first_release="$(readlink -f "$PNUL_CURRENT_LINK")"

printf '%s\n' 'PRIVATE TOKEN' > "$scratch/source/web/panopticon-terminal/scripts/.env"
printf '%s\n' 'PRIVATE DATABASE' > "$scratch/source/web/panopticon-terminal/public/pnull.db"
printf '%s\n' 'PRIVATE NOTE' > "$scratch/source/web/panopticon-terminal/portraits/private-note.txt"
"$script_dir/publish.sh"
[[ ! -e "$PNUL_CURRENT_LINK/scripts/.env" ]]
[[ ! -e "$PNUL_CURRENT_LINK/public/pnull.db" ]]
[[ ! -e "$PNUL_CURRENT_LINK/portraits/private-note.txt" ]]
[[ ! -e "$PNUL_CURRENT_LINK/geography/source-archives" ]]
first_release="$(readlink -f "$PNUL_CURRENT_LINK")"

mv -- "$scratch/source/web/panopticon-terminal/portraits/tom-bailey.jpg" "$scratch/tom-bailey.jpg"
ln -s -- /etc/passwd "$scratch/source/web/panopticon-terminal/portraits/tom-bailey.jpg"
if "$script_dir/publish.sh"; then
    echo "symlinked public asset unexpectedly replaced the public release" >&2
    exit 1
fi
[[ "$(readlink -f "$PNUL_CURRENT_LINK")" == "$first_release" ]]
rm -- "$scratch/source/web/panopticon-terminal/portraits/tom-bailey.jpg"
mv -- "$scratch/tom-bailey.jpg" "$scratch/source/web/panopticon-terminal/portraits/tom-bailey.jpg"

export PNUL_TEST_FAIL=1
if "$script_dir/publish.sh"; then
    echo "failed export unexpectedly replaced the public release" >&2
    exit 1
fi
[[ "$(readlink -f "$PNUL_CURRENT_LINK")" == "$first_release" ]]
[[ -z "$(find "$PNUL_RELEASE_ROOT" -maxdepth 1 -name '.candidate.*' -print -quit)" ]]
echo "self-host publication test passed"
