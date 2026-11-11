#!/usr/bin/env bash
# Encrypt a coherent private SQLite/evidence snapshot and the published API history.
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
: "${PNUL_DATA_DIR:?set private state path}"
: "${PNUL_CURRENT_LINK:?set current release symlink}"
: "${PNUL_RELEASE_ROOT:?set release root}"
: "${RESTIC_REPOSITORY:?set an off-server restic repository}"
: "${RESTIC_PASSWORD_FILE:?set a root-managed restic password file}"
[[ "$PNUL_DATA_DIR" == /* && -d "$PNUL_DATA_DIR" && ! -L "$PNUL_DATA_DIR" ]] || exit 1
[[ "$RESTIC_PASSWORD_FILE" == /* && -f "$RESTIC_PASSWORD_FILE" && ! -L "$RESTIC_PASSWORD_FILE" ]] || exit 1
case "$RESTIC_REPOSITORY" in sftp:*|s3:*|b2:*|rest:*|azure:*|gs:*) ;; *) echo 'backup destination must be off-server storage' >&2; exit 1 ;; esac
if [[ -z "${PNUL_HELD_LOCKS:-}" ]]; then
    exec python3 "$script_dir/locked-run.py" "$PNUL_DATA_DIR/.collect.lock" -- bash "$script_dir/backup.sh"
fi
python3 "$script_dir/locked-run.py" --verify "$PNUL_DATA_DIR/.collect.lock"

[[ -L "$PNUL_CURRENT_LINK" ]] || { echo 'published release required for backup' >&2; exit 1; }
release="$(realpath -e "$PNUL_CURRENT_LINK")"
release_root="$(realpath -e "$PNUL_RELEASE_ROOT")"
[[ "$release" == "$release_root/"* ]] || exit 1
private_root="$(realpath -e "$PNUL_DATA_DIR")"
for tree in evidence records; do
    [[ -d "$private_root/$tree" && ! -L "$private_root/$tree" ]] || exit 1
done
extra_paths=()
for tree in discovery-candidates discovery-archives; do
    if [[ -e "$private_root/$tree" || -L "$private_root/$tree" ]]; then
        [[ -d "$private_root/$tree" && ! -L "$private_root/$tree" ]] || exit 1
        extra_paths+=("$private_root/$tree")
    fi
done
if [[ -n "${PNUL_PROPOSAL_INPUTS:-}" ]]; then
    read -r -a proposals <<< "$PNUL_PROPOSAL_INPUTS"
    for proposal in "${proposals[@]}"; do
        [[ "$proposal" == "$private_root/"* && -f "$proposal" && ! -L "$proposal" ]] || exit 1
        [[ "$(realpath -e "$proposal")" == "$private_root/"* ]] || exit 1
        extra_paths+=("$proposal")
    done
fi
mkdir -p "$private_root/ops"
[[ ! -L "$private_root/ops" ]] || exit 1
stage="$(mktemp -d "$private_root/ops/.backup.XXXXXXXX")"
trap 'rm -rf -- "$stage"' EXIT
python3 - "$private_root/pnull.db" "$stage/pnull.db" <<'PY'
import os,sqlite3,sys
from pathlib import Path
source,target=map(Path,sys.argv[1:])
if source.is_symlink() or not source.is_file(): raise ValueError('unsafe database')
with sqlite3.connect(f'file:{source}?mode=ro',uri=True) as original:
    with sqlite3.connect(target) as backup:
        original.backup(backup)
        if backup.execute('PRAGMA integrity_check').fetchone()[0]!='ok': raise ValueError('database integrity failure')
os.chmod(target,0o600)
PY
# Stable archive path names make documented restores independent of temporary names.
# Source blobs are immutable, but remain locked through upload for a coherent set.
restic backup --tag panopticon --tag colorado-production \
    "$stage/pnull.db" "$private_root/evidence" "$private_root/records" "${extra_paths[@]}" "$release"
