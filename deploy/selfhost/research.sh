#!/usr/bin/env bash
# Bounded preliminary source discovery, followed by publication of labeled leads.
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
: "${PNUL_REPO_DIR:?set repository path}"
: "${PNUL_DATA_DIR:?set private state path}"
[[ "$PNUL_REPO_DIR" == /* && "$PNUL_DATA_DIR" == /* && -d "$PNUL_DATA_DIR" && ! -L "$PNUL_DATA_DIR" ]] || exit 1
if [[ -z "${PNUL_HELD_LOCKS:-}" ]]; then
    exec python3 "$script_dir/locked-run.py" "$PNUL_DATA_DIR/.collect.lock" -- bash "$script_dir/research.sh"
fi
python3 "$script_dir/locked-run.py" --verify "$PNUL_DATA_DIR/.collect.lock"
cd "$PNUL_REPO_DIR"
data="$PNUL_DATA_DIR/research"
[[ ! -L "$data" ]] || { echo 'research directory cannot be a symlink' >&2; exit 1; }
binary="${PNUL_BIN:-/usr/local/bin/pnull}"
[[ "$binary" == /* && -x "$binary" && ! -L "$binary" ]] || { echo 'pnull binary missing or unsafe' >&2; exit 1; }
status=0
python3 scripts/colorado-research.py --data-dir "$data" discover --limit 320 --workers 8 || status=1
python3 scripts/colorado-research.py --data-dir "$data" civic-history --limit 20 --per-place 4 --workers 8 || status=1
python3 scripts/colorado-research.py --data-dir "$data" scan --max-clients 100 --pages 10 || status=1
python3 scripts/colorado-research.py --data-dir "$data" scan --max-clients 100 --pages 10 || status=1
python3 scripts/colorado-research.py --data-dir "$data" actions --limit 100 || status=1
python3 scripts/colorado-research.py --data-dir "$data" agendasuite --limit 60 --pages 3 || status=1
python3 scripts/colorado-research.py --data-dir "$data" screen-documents --pnull-bin "$binary" --limit 300 --per-place 30 --workers 4 || status=1
python3 scripts/colorado-research.py --data-dir "$data" screen-documents --pnull-bin "$binary" --refresh-existing --limit 50 --per-place 2 --workers 4 || status=1
python3 scripts/colorado-research.py --data-dir "$data" docx-saved --revalidate-docx --limit 50 --workers 2 || status=1
python3 scripts/colorado-research.py --data-dir "$data" docx-saved --limit 50 --workers 2 || status=1
python3 scripts/colorado-research.py --data-dir "$data" ocr-saved --pnull-bin "$binary" --revalidate-ocr --limit 30 --workers 2 || status=1
python3 scripts/colorado-research.py --data-dir "$data" ocr-saved --pnull-bin "$binary" --retry-previous-limit --limit 30 --workers 2 || status=1
python3 scripts/colorado-research.py --data-dir "$data" screen-documents --pnull-bin "$binary" --retry-errors --ocr --limit 30 --per-place 3 --workers 2 || status=1
python3 scripts/colorado-research.py --data-dir "$data" portraits --limit 32 || status=1
exit "$status"
