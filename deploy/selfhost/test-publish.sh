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
cp -- "$repo_dir/fixtures/atlas/sources.co.v1.json" "$scratch/source/fixtures/atlas/"
cp -a "$repo_dir/fixtures/atlas/cases" "$repo_dir/fixtures/atlas/preview-cases" \
  "$repo_dir/fixtures/atlas/portrait-sources" "$scratch/source/fixtures/atlas/"
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
spring_page="$PNUL_CURRENT_LINK/decisions/co-colorado-springs-25-581-2025-11-25.html"
[[ -f "$spring_page" ]]
grep -Fq 'property="og:title" content="Colorado Springs police technology surcharge vote | PANOPTICON.FAIL"' "$spring_page"
grep -Fq 'name="twitter:title" content="Colorado Springs police technology surcharge vote | PANOPTICON.FAIL"' "$spring_page"
grep -Fq 'name="robots" content="index,follow"' "$spring_page"
grep -Fq 'name="robots" content="noindex,follow"' "$PNUL_CURRENT_LINK/decision.html"
! grep -Fq 'Colorado Springs police technology surcharge vote' "$PNUL_CURRENT_LINK/decision.html"
[[ -f "$PNUL_CURRENT_LINK/portraits/tom-bailey.jpg" ]]
[[ ! -e "$PNUL_CURRENT_LINK/portraits/susan-noble-original.png" ]]
[[ ! -e "$PNUL_CURRENT_LINK/portraits/joanna-sandoval-original.png" ]]
[[ ! -e "$PNUL_CURRENT_LINK/portraits/craig-kim-original.png" ]]
[[ ! -e "$PNUL_CURRENT_LINK/portraits/charles-dukes-original.png" ]]
[[ ! -e "$PNUL_CURRENT_LINK/portraits/stefanie-trujillo-original.png" ]]
[[ ! -e "$PNUL_CURRENT_LINK/portraits/lori-young-original.png" ]]
[[ ! -e "$PNUL_CURRENT_LINK/portraits/steve-douglas-original.png" ]]
[[ ! -e "$PNUL_CURRENT_LINK/portraits/jose-guardiola-original.png" ]]
[[ ! -e "$PNUL_CURRENT_LINK/portraits/rocky-teter-original.png" ]]
for portrait in jason-gray ryan-hollingshead kevin-bracken mark-davis max-brooks tim-dietz laura-cavey; do
    [[ ! -e "$PNUL_CURRENT_LINK/portraits/$portrait-original.jpg" ]]
done
[[ -f "$PNUL_CURRENT_LINK/portraits/portraits.js" ]]
[[ -f "$PNUL_CURRENT_LINK/portraits/lookup.js" ]]
! grep -q 'recorded_voter_co_commerce_city' "$PNUL_CURRENT_LINK/portraits/portraits.js"
! grep -q 'recorded_voter_co_castle_rock' "$PNUL_CURRENT_LINK/portraits/portraits.js"
[[ ! -e "$PNUL_CURRENT_LINK/fixtures" ]]
[[ ! -e "$PNUL_CURRENT_LINK/preview-snapshot.json" ]]
[[ ! -e "$PNUL_CURRENT_LINK/pnull.db" ]]
[[ ! -e "$PNUL_CURRENT_LINK/scripts/validate.mjs" ]]
[[ ! -e "$PNUL_CURRENT_LINK/scripts/filter-portraits.mjs" ]]
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

cp -- "$scratch/source/web/panopticon-terminal/portraits/brandy-williams.jpg" "$scratch/brandy-williams.jpg"
printf 'altered' >> "$scratch/source/web/panopticon-terminal/portraits/brandy-williams.jpg"
if "$script_dir/publish.sh"; then
    echo "changed portrait bytes unexpectedly replaced the public release" >&2
    exit 1
fi
[[ "$(readlink -f "$PNUL_CURRENT_LINK")" == "$first_release" ]]
mv -- "$scratch/brandy-williams.jpg" "$scratch/source/web/panopticon-terminal/portraits/brandy-williams.jpg"

cp -- "$scratch/source/fixtures/atlas/portrait-sources/commerce-person-210.json" "$scratch/commerce-person-210.json"
printf 'altered' >> "$scratch/source/fixtures/atlas/portrait-sources/commerce-person-210.json"
if "$script_dir/publish.sh"; then
    echo "changed official person record unexpectedly replaced the public release" >&2
    exit 1
fi
[[ "$(readlink -f "$PNUL_CURRENT_LINK")" == "$first_release" ]]
mv -- "$scratch/commerce-person-210.json" "$scratch/source/fixtures/atlas/portrait-sources/commerce-person-210.json"

cp -- "$scratch/source/fixtures/atlas/portrait-sources/castlerock-jason-gray-dom.json" "$scratch/castlerock-jason-gray-dom.json"
printf 'altered' >> "$scratch/source/fixtures/atlas/portrait-sources/castlerock-jason-gray-dom.json"
if "$script_dir/publish.sh"; then
    echo "changed browser source capture unexpectedly replaced the public release" >&2
    exit 1
fi
[[ "$(readlink -f "$PNUL_CURRENT_LINK")" == "$first_release" ]]
mv -- "$scratch/castlerock-jason-gray-dom.json" "$scratch/source/fixtures/atlas/portrait-sources/castlerock-jason-gray-dom.json"

export PNUL_TEST_FAIL=1
if "$script_dir/publish.sh"; then
    echo "failed export unexpectedly replaced the public release" >&2
    exit 1
fi
[[ "$(readlink -f "$PNUL_CURRENT_LINK")" == "$first_release" ]]
[[ -z "$(find "$PNUL_RELEASE_ROOT" -maxdepth 1 -name '.candidate.*' -print -quit)" ]]
echo "self-host publication test passed"
