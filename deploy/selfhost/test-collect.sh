#!/usr/bin/env bash
set -euo pipefail

python3 "$(dirname "$0")/test-candidate-profiles.py"

repo="$(cd "$(dirname "$0")/../.." && pwd)"
python3 "$repo/deploy/selfhost/test-calendar-profiles.py"
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT
mkdir -p "$scratch/repo/configs/states" "$scratch/repo/deploy/selfhost" "$scratch/data"
cp "$repo"/configs/states/co*.toml "$scratch/repo/configs/states/"
cp "$repo/deploy/selfhost/collection-sources.tsv" "$scratch/repo/deploy/selfhost/"
printf '%s\n' 'configs/states/co-commerce-archive.toml	commerce-ord2755-detail	archive' >> "$scratch/repo/deploy/selfhost/collection-sources.tsv"
cat > "$scratch/fake-pnull" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$PNUL_TEST_LOG"
if [[ " $* " == *" source review verify denver-calendar "* ]]; then
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
grep -q -- '--config configs/states/co-commerce-archive.toml source archive commerce-ord2755-detail' "$PNUL_TEST_LOG"
grep -q -- '--config configs/states/co.toml procurement refresh colorado-springs-contract-awards --live' "$PNUL_TEST_LOG"
if grep -q -- 'source discover-calendar denver-calendar' "$PNUL_TEST_LOG"; then
    echo "unreviewed Denver source was fetched" >&2
    exit 1
fi
grep -q -- ' scan$' "$PNUL_TEST_LOG"

# Reject a duplicate or malformed schedule before making any CLI call.
printf '%s\n' 'configs/states/co.toml	denver-calendar	discover-calendar' >> "$scratch/repo/deploy/selfhost/collection-sources.tsv"
: > "$PNUL_TEST_LOG"
if bash "$repo/deploy/selfhost/collect.sh" 2>/dev/null || [[ -s "$PNUL_TEST_LOG" ]]; then
    echo "duplicate source was not rejected before fetching" >&2
    exit 1
fi
printf 'collection schedule checks passed\n'
