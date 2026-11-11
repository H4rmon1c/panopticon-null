#!/usr/bin/env bash
set -euo pipefail

python3 "$(dirname "$0")/test-candidate-profiles.py"
python3 "$(dirname "$0")/test-fallback-profiles.py"

repo="$(cd "$(dirname "$0")/../.." && pwd)"
python3 "$repo/deploy/selfhost/test-calendar-profiles.py"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT
mkdir -p "$scratch/repo/configs/states" "$scratch/repo/deploy/selfhost" "$scratch/data"
cp "$repo"/configs/states/co*.toml "$scratch/repo/configs/states/"
cp "$repo/deploy/selfhost/collection-sources.tsv" "$scratch/repo/deploy/selfhost/"
cp "$repo/deploy/selfhost/collection-fallbacks.tsv" "$scratch/repo/deploy/selfhost/"
printf '%s\n' 'configs/states/co-commerce-archive.toml	commerce-ord2755-detail	archive' >> "$scratch/repo/deploy/selfhost/collection-sources.tsv"
cat > "$scratch/fake-pnull" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$PNUL_TEST_LOG"
if [[ " $* " == *" source review verify denver-calendar "* ]]; then
    exit 1
fi
if [[ "${PNUL_TEST_FAIL_DENVER_API:-0}" == 1 && " $* " == *" source review verify denver-legistar-events "* ]]; then
    exit 1
fi
if [[ "${PNUL_TEST_FAIL_MOFFAT:-0}" == 1 && " $* " == *" source review verify moffat-document-index "* ]]; then
    exit 1
fi
if [[ "${PNUL_TEST_FAIL_SIMLA_INDEX:-0}" == 1 && " $* " == *" source discover-index simla-document-index "* ]]; then
    exit 1
fi
if [[ "${PNUL_TEST_FAIL_COMMERCE:-0}" == 1 && " $* " == *" source discover-calendar commerce-city-calendar "* ]]; then
    exit 1
fi
EOF
chmod +x "$scratch/fake-pnull"
export PNUL_REPO_DIR="$scratch/repo"
export PNUL_DATA_DIR="$scratch/data"
export PNUL_BIN="$scratch/fake-pnull"
export PNUL_TEST_LOG="$scratch/calls"

# One failed source review must not fetch that source or stop other profiles.
if bash "$repo/deploy/selfhost/collect.sh"; then
    echo "expected a failed collection when a source review fails" >&2
    exit 1
fi
grep -q -- '--config configs/states/co-commerce-city.toml source discover-calendar commerce-city-calendar' "$PNUL_TEST_LOG"
grep -q -- '--config configs/states/co-arapahoe-county.toml source discover-calendar arapahoe-county-calendar' "$PNUL_TEST_LOG"
grep -q -- '--config configs/states/co-douglas-county.toml source discover-calendar douglas-county-calendar' "$PNUL_TEST_LOG"
grep -q -- '--config configs/states/co-littleton.toml source discover-calendar littleton-calendar' "$PNUL_TEST_LOG"
grep -q -- '--config configs/states/co-denver.toml ingest --source-id denver-legistar-events' "$PNUL_TEST_LOG"
if grep -q -- 'ingest --source-id commerce-city-legistar-events' "$PNUL_TEST_LOG"; then
    echo "API fallback ran despite successful Commerce City calendar" >&2
    exit 1
fi
grep -q -- '--config configs/states/co-commerce-archive.toml source archive commerce-ord2755-detail' "$PNUL_TEST_LOG"
grep -q -- '--config configs/states/co.toml procurement refresh colorado-springs-contract-awards --live' "$PNUL_TEST_LOG"
if grep -q -- 'source discover-calendar denver-calendar' "$PNUL_TEST_LOG"; then
    echo "unreviewed Denver source was fetched" >&2
    exit 1
fi
grep -q -- ' scan$' "$PNUL_TEST_LOG"

# A failed calendar fetch, as well as a failed calendar review, invokes only
# its configured, independently reviewed API fallback. The run stays degraded.
: > "$PNUL_TEST_LOG"
if PNUL_TEST_FAIL_COMMERCE=1 bash "$repo/deploy/selfhost/collect.sh"; then
    echo "calendar failure was hidden by the API fallback" >&2
    exit 1
fi
grep -q -- '--config configs/states/co-commerce-city.toml ingest --source-id commerce-city-legistar-events' "$PNUL_TEST_LOG"
if grep -q -- 'ingest --source-id colorado-springs-legistar-events' "$PNUL_TEST_LOG"; then
    echo "unneeded Colorado Springs API fallback ran" >&2
    exit 1
fi

# An API with no current review is never fetched, even after the calendar fails.
: > "$PNUL_TEST_LOG"
if PNUL_TEST_FAIL_DENVER_API=1 bash "$repo/deploy/selfhost/collect.sh" 2>/dev/null; then
    echo "unreviewed API fallback hid the calendar failure" >&2
    exit 1
fi
grep -q -- 'source review verify denver-legistar-events' "$PNUL_TEST_LOG"
if grep -q -- 'ingest --source-id denver-legistar-events' "$PNUL_TEST_LOG"; then
    echo "unreviewed API fallback was fetched" >&2
    exit 1
fi

# Reject a duplicate or malformed schedule before making any CLI call.
printf '%s\n' 'configs/states/co.toml	denver-calendar	discover-calendar' >> "$scratch/repo/deploy/selfhost/collection-sources.tsv"
: > "$PNUL_TEST_LOG"
if bash "$repo/deploy/selfhost/collect.sh" 2>/dev/null || [[ -s "$PNUL_TEST_LOG" ]]; then
    echo "duplicate source was not rejected before fetching" >&2
    exit 1
fi
cp "$repo/deploy/selfhost/collection-sources.tsv" "$scratch/repo/deploy/selfhost/collection-sources.tsv"
printf '%s\n' 'configs/states/co-denver.toml	denver-calendar	denver-legistar-events' >> "$scratch/repo/deploy/selfhost/collection-fallbacks.tsv"
: > "$PNUL_TEST_LOG"
if bash "$repo/deploy/selfhost/collect.sh" 2>/dev/null || [[ -s "$PNUL_TEST_LOG" ]]; then
    echo "duplicate API fallback was not rejected before fetching" >&2
    exit 1
fi
# Opt-in town indexes dispatch correctly; one missing review does not fetch it
# or prevent independently reviewed towns from being checked.
cp "$repo/deploy/selfhost/collection-small-town-sources.tsv" "$scratch/repo/deploy/selfhost/"
cp "$repo/deploy/selfhost/collection-release-fallbacks.tsv" "$scratch/repo/deploy/selfhost/"
export PNUL_COLLECT_MATRIX_PATH=deploy/selfhost/collection-small-town-sources.tsv
export PNUL_COLLECT_FALLBACKS_PATH=deploy/selfhost/collection-release-fallbacks.tsv
: > "$PNUL_TEST_LOG"
bash "$repo/deploy/selfhost/collect.sh"
[[ "$(grep -c ' source discover-index ' "$PNUL_TEST_LOG")" == 5 ]]
: > "$PNUL_TEST_LOG"
if PNUL_TEST_FAIL_MOFFAT=1 PNUL_TEST_FAIL_SIMLA_INDEX=1 bash "$repo/deploy/selfhost/collect.sh"; then
    echo "town discovery failure was hidden" >&2; exit 1
fi
! grep -q ' source discover-index moffat-document-index' "$PNUL_TEST_LOG"
grep -q ' source discover-index saguache-document-index' "$PNUL_TEST_LOG"
! grep -q ' ingest ' "$PNUL_TEST_LOG"

# Validate every row AND its source config before the first CLI operation.
for broken in unknown wrong-adapter broken-toml duplicate-config-source wrong-policy-host; do
    cp "$repo"/configs/states/co*.toml "$scratch/repo/configs/states/"
    cp "$repo/deploy/selfhost/collection-small-town-sources.tsv" "$scratch/repo/deploy/selfhost/"
    python3 - "$scratch/repo" "$broken" <<'PYTEST'
from pathlib import Path
import sys
root=Path(sys.argv[1]);broken=sys.argv[2]
matrix=root/'deploy/selfhost/collection-small-town-sources.tsv'
config=root/'configs/states/co-saguache.toml'
if broken=='unknown': matrix.write_text(matrix.read_text()+'configs/states/co-saguache.toml\tunknown-source\tdiscover-index\n')
elif broken=='wrong-adapter': matrix.write_text(matrix.read_text().replace('saguache-document-index\tdiscover-index','saguache-document-index\tdiscover-calendar'))
elif broken=='broken-toml': config.write_text(config.read_text()+'\ninvalid = [')
elif broken=='duplicate-config-source': config.write_text(config.read_text()+'\n[[sources]]\nid = "saguache-document-index"\n')
elif broken=='wrong-policy-host': config.write_text(config.read_text().replace('https://www.townofsaguache.org/uploads/','https://other.example.org/uploads/'))
PYTEST
    : > "$PNUL_TEST_LOG"
    if bash "$repo/deploy/selfhost/collect.sh" >/dev/null 2>&1 || [[ -s "$PNUL_TEST_LOG" ]]; then
        echo "invalid $broken schedule reached the CLI" >&2; exit 1
    fi
done
printf 'collection schedule checks passed\n'
