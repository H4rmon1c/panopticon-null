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
if [[ -n "${PNUL_TEST_CALLS:-}" ]]; then printf '%s\n' "$*" >> "$PNUL_TEST_CALLS"; fi
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
for portrait in flor-alvidrez kevin-flynn stacie-gilmore serena-gonzales-gutierrez chris-hinds paul-kashmann shontel-lewis sarah-parady diana-romero-campbell amanda-sandoval amanda-sawyer jamie-torres darrell-watson; do
    [[ -f "$PNUL_CURRENT_LINK/portraits/denver-$portrait-original.jpg" ]]
done
for portrait in susan-noble joanna-sandoval craig-kim charles-dukes stefanie-trujillo lori-young steve-douglas jose-guardiola rocky-teter; do
    [[ -f "$PNUL_CURRENT_LINK/portraits/$portrait-original.png" ]]
done
[[ -f "$PNUL_CURRENT_LINK/decisions/co-commerce-city-ord-2755-2026-04-06.html" ]]
[[ -f "$PNUL_CURRENT_LINK/decisions/co-commerce-city-ord-2756-2026-04-06.html" ]]
grep -Fq 'This proposed amendment failed' "$PNUL_CURRENT_LINK/decisions/co-denver-25-0413-2025-05-05.html"
for portrait in jason-gray ryan-hollingshead kevin-bracken mark-davis max-brooks tim-dietz laura-cavey; do
    [[ -f "$PNUL_CURRENT_LINK/portraits/$portrait-original.jpg" ]]
done
[[ -f "$PNUL_CURRENT_LINK/portraits/portraits.js" ]]
[[ -f "$PNUL_CURRENT_LINK/portraits/lookup.js" ]]
grep -q 'recorded_voter_co_commerce_city' "$PNUL_CURRENT_LINK/portraits/portraits.js"
grep -q 'recorded_voter_co_castle_rock' "$PNUL_CURRENT_LINK/portraits/portraits.js"
[[ ! -e "$PNUL_CURRENT_LINK/fixtures" ]]
[[ ! -e "$PNUL_CURRENT_LINK/preview-snapshot.json" ]]
[[ ! -e "$PNUL_CURRENT_LINK/pnull.db" ]]
[[ ! -e "$PNUL_CURRENT_LINK/scripts/validate.mjs" ]]
[[ ! -e "$PNUL_CURRENT_LINK/scripts/filter-portraits.mjs" ]]
[[ ! -e "$PNUL_CURRENT_LINK/scripts/test-client.mjs" ]]
[[ ! -e "$PNUL_CURRENT_LINK/scripts/build-decision-pages.mjs" ]]
first_release="$(readlink -f "$PNUL_CURRENT_LINK")"
[[ -f "$PNUL_CURRENT_LINK/api/v1/alerts.json" && -f "$PNUL_CURRENT_LINK/api/v1/feed.json" && -f "$PNUL_CURRENT_LINK/api/v1/coverage.json" ]]
[[ -f "$PNUL_CURRENT_LINK/research.html" && -f "$PNUL_CURRENT_LINK/api/v1/research.json" ]]
[[ -f "$PNUL_CURRENT_LINK/research-portraits/84202caa2919b815df89984e12ad15f4de4289a35b74f048f54f0e68f0954174.jpg" ]]
[[ -f "$PNUL_CURRENT_LINK/research-portraits/3ea3f82d546b3de7b1d07cfdb69e429e643b6ade00677f189bdee6fc59b1e5a6.jpg" ]]
[[ -f "$PNUL_CURRENT_LINK/research-portraits/942839934eb8ea112842148a95aa7deb38f94793e503d7ea5bf76e09cf439da2.jpg" ]]
cmp -s "$PNUL_CURRENT_LINK/public/research-signals.json" "$PNUL_CURRENT_LINK/api/v1/research.json"
[[ ! -e "$PNUL_CURRENT_LINK/scripts/build-alert-api.mjs" ]]

# Withdrawn rows are checked after the active set exists, and retain their CLI args.
export PNUL_TEST_CALLS="$scratch/publish-calls"
withdrawn_fixture=fixtures/atlas/preview-cases/arapahoe-county-24-633-2024-11-26.v2.json
PNUL_WITHDRAWN_MANIFESTS="$withdrawn_fixture" "$script_dir/publish.sh"
grep -Fq -- "--withdrawn-manifest $withdrawn_fixture" "$PNUL_TEST_CALLS"
first_release="$(readlink -f "$PNUL_CURRENT_LINK")"
if PNUL_WITHDRAWN_MANIFESTS=fixtures/atlas/colorado-springs-25-93.v1.json "$script_dir/publish.sh"; then
    echo "active and withdrawn manifest collision was accepted" >&2; exit 1
fi
[[ "$(readlink -f "$PNUL_CURRENT_LINK")" == "$first_release" ]]
if PNUL_WITHDRAWN_MANIFESTS="$withdrawn_fixture $withdrawn_fixture" "$script_dir/publish.sh"; then
    echo "duplicate withdrawal was accepted" >&2; exit 1
fi
[[ "$(readlink -f "$PNUL_CURRENT_LINK")" == "$first_release" ]]
(
    flock -n 7 || exit 1
    if "$script_dir/publish.sh"; then echo "publication ran during collection" >&2; exit 1; fi
) 7>"$PNUL_DATA_DIR/.collect.lock"
[[ "$(readlink -f "$PNUL_CURRENT_LINK")" == "$first_release" ]]

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

# A real staged withdrawal must survive validators, API history, and another publish.
node --input-type=module - "$PNUL_TEST_SNAPSHOT" "$scratch/withdrawal.json" "$scratch/silent.json" <<'JS'
import {readFileSync, writeFileSync} from 'node:fs';
const [source, withdrawn, silent] = process.argv.slice(2);
const snapshot = JSON.parse(readFileSync(source));
const caseId = 'co-castle-rock-res-2025-071-2025-06-03';
const release = snapshot.case_releases.find(item => item.case_id === caseId);
if (!release) throw new Error('withdrawal fixture is missing');
snapshot.entities = snapshot.entities.filter(item => item.slug !== caseId);
snapshot.case_releases = snapshot.case_releases.filter(item => item.case_id !== caseId);
writeFileSync(silent, JSON.stringify(snapshot));
snapshot.withdrawals = [{case_id:caseId,state:'withdrawn',bound_digest:release.release.bound_digest,review_id:'review:offline-shell-fixture',decided_at:new Date().toISOString()}];
writeFileSync(withdrawn, JSON.stringify(snapshot));
JS
if PNUL_TEST_SNAPSHOT="$scratch/silent.json" "$script_dir/publish.sh"; then
    echo "silent case disappearance unexpectedly published" >&2; exit 1
fi
[[ "$(readlink -f "$PNUL_CURRENT_LINK")" == "$first_release" ]]
PNUL_TEST_SNAPSHOT="$scratch/withdrawal.json" "$script_dir/publish.sh"
withdrawal_page="$PNUL_CURRENT_LINK/decisions/co-castle-rock-res-2025-071-2025-06-03.html"
grep -Fq 'Record withdrawn pending correction' "$withdrawal_page"
grep -Fq 'noindex' "$withdrawal_page"
PNUL_TEST_SNAPSHOT="$scratch/silent.json" "$script_dir/publish.sh"
grep -Fq 'Record withdrawn pending correction' "$withdrawal_page"
node --input-type=module - "$PNUL_CURRENT_LINK/api/v1/alerts.json" <<'JS'
import {readFileSync} from 'node:fs';
const api=JSON.parse(readFileSync(process.argv[2]));
if(api.records.find(item=>item.case_id==='co-castle-rock-res-2025-071-2025-06-03')?.state!=='withdrawn')throw new Error('withdrawal tombstone lost');
JS
first_release="$(readlink -f "$PNUL_CURRENT_LINK")"

export PNUL_TEST_FAIL=1
if "$script_dir/publish.sh"; then
    echo "failed export unexpectedly replaced the public release" >&2
    exit 1
fi
[[ "$(readlink -f "$PNUL_CURRENT_LINK")" == "$first_release" ]]
[[ -z "$(find "$PNUL_RELEASE_ROOT" -maxdepth 1 -name '.candidate.*' -print -quit)" ]]
echo "self-host publication test passed"
