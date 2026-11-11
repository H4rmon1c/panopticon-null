#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="$(cd -- "$script_dir/../.." && pwd)"
grep -Fxq 'StateDirectoryMode=0700' "$script_dir/panopticon-candidates.service"
[[ -x "$script_dir/collect-candidates.sh" ]]
scratch="$(mktemp -d)"
trap 'rm -rf -- "$scratch"' EXIT
mkdir -p "$scratch/repo/deploy/selfhost" "$scratch/repo/configs/states" "$scratch/private"
cp -- "$repo_dir/configs/states/co-commerce-city.toml" "$scratch/repo/configs/states/"
cp -- "$repo_dir/configs/states/co-moffat.toml" "$scratch/repo/configs/states/"
cat > "$scratch/fake-pnull" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$PNUL_TEST_LOG"
[[ "${PNUL_TEST_FAIL:-0}" != 1 ]]
EOF
chmod +x "$scratch/fake-pnull"
export PNUL_REPO_DIR="$scratch/repo"
export PNUL_DATA_DIR="$scratch/private"
export PNUL_BIN="$scratch/fake-pnull"
export PNUL_TEST_LOG="$scratch/calls.log"

matrix="$scratch/repo/deploy/selfhost/candidate-queues.tsv"
printf '# no reviewed queues yet\n' > "$matrix"
"$script_dir/collect-candidates.sh"
[[ ! -e "$PNUL_TEST_LOG" ]]

printf 'configs/states/co-commerce-city.toml\tcommerce-city-calendar\t2\t604800\n' > "$matrix"
"$script_dir/collect-candidates.sh"
grep -Fq -- '--config configs/states/co-commerce-city.toml source archive-queue commerce-city-calendar --max-fetches 2 --refresh-after-seconds 604800' "$PNUL_TEST_LOG"
[[ "$(wc -l < "$PNUL_TEST_LOG")" -eq 1 ]]

# Parse every row before the first CLI invocation.
printf 'configs/states/co-moffat.toml\tmoffat-document-index\t1\t999999999999999999999999\n' >> "$matrix"
if "$script_dir/collect-candidates.sh"; then
    echo "unbounded queue row was accepted" >&2
    exit 1
fi
[[ "$(wc -l < "$PNUL_TEST_LOG")" -eq 1 ]]

printf 'configs/states/co-commerce-city.toml\tcommerce-city-calendar\t2\t604800\n' > "$matrix"
printf 'configs/states/co-moffat.toml\tmoffat-document-index\t1\t604800\n' >> "$matrix"
export PNUL_TEST_FAIL=1
if "$script_dir/collect-candidates.sh"; then
    echo "failed candidate fetch was hidden" >&2
    exit 1
fi
[[ "$(wc -l < "$PNUL_TEST_LOG")" -eq 3 ]]
unset PNUL_TEST_FAIL

printf 'configs/states/co-commerce-city.toml\tcommerce-city-calendar\t2\t604800\n' >> "$matrix"
if "$script_dir/collect-candidates.sh"; then
    echo "duplicate queue source was accepted" >&2
    exit 1
fi
[[ "$(wc -l < "$PNUL_TEST_LOG")" -eq 3 ]]
echo "bounded candidate queue schedule checks passed"
