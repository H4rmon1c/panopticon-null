#!/usr/bin/env python3
"""Production default must stay inside the twelve attested exact endpoints."""
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
packet=json.loads((ROOT/'docs/colorado-source-review-packet.json').read_text())
expected={(row['archive_config'],row['archive_source_id'],'archive') for row in packet['sources'] if row['release_cited']}
rows=[]
for line in (ROOT/'deploy/selfhost/collection-release-sources.tsv').read_text().splitlines():
    if line and not line.startswith('#'): rows.append(tuple(line.split('\t')))
assert len(rows)==12 and len(set(rows))==12 and set(rows)==expected
assert not any(line and not line.startswith('#') for line in (ROOT/'deploy/selfhost/collection-release-fallbacks.tsv').read_text().splitlines())
env=(ROOT/'deploy/selfhost/panopticon.env.example').read_text()
assert 'PNUL_COLLECT_MATRIX_PATH=deploy/selfhost/collection-release-sources.tsv' in env
assert 'PNUL_COLLECT_FALLBACKS_PATH=deploy/selfhost/collection-release-fallbacks.tsv' in env
for name in ('collect','candidates','publish'):
    unit=(ROOT/f'deploy/selfhost/panopticon-{name}.service').read_text()
    assert 'TemporaryFileSystem=/tmp:rw,nosuid,nodev,size=512M,nr_inodes=32768,mode=1777' in unit
    assert 'Environment=TMPDIR=/tmp' in unit and 'PrivateTmp=true' not in unit
print('Production schedule contains only twelve attested endpoints; temporary workspaces are bounded.')
