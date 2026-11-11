#!/usr/bin/env python3
"""Verify this exact preserved revision; never normalize future captures for release."""
import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def presentation_values(raw):
    for name in (b'__VIEWSTATE', b'__EVENTVALIDATION'):
        pattern = (rb'(<input type="hidden" name="' + name + rb'" id="' + name
                   + rb'" value=")[A-Za-z0-9+/=]+(" />)')
        raw, count = re.subn(pattern, rb'\1PRESERVED_STATE\2', raw)
        if count != 1:
            raise ValueError('Hidden state field shape or count changed')
    raw, count = re.subn(
        rb'((?:href="|src=\')/WebResource\.axd\?d=[A-Za-z0-9_-]+&amp;t=)\d{18}(["\'])',
        rb'\1CACHE_TIME\2', raw)
    if count != 11:
        raise ValueError('Resource cache parameter shape or count changed')
    return raw


def main():
    revision = json.loads((ROOT / 'fixtures/co-castlerock/detail-revision-2026-11-10.json').read_text())
    streams = []
    for entry in (revision['prior'], revision['captured']):
        raw = (ROOT / entry['fixture']).read_bytes()
        if hashlib.sha256(raw).hexdigest() != entry['sha256']:
            raise ValueError('Preserved HTML revision digest changed')
        streams.append(raw)
    if streams[0] == streams[1] or presentation_values(streams[0]) != presentation_values(streams[1]):
        raise ValueError('Revision differs beyond the documented presentation fields')
    changed_fact = streams[1].replace(b'Flock', b'Different Vendor', 1)
    if presentation_values(streams[0]) == presentation_values(changed_fact):
        raise ValueError('A substantive text change was ignored')
    case = json.loads((ROOT / 'fixtures/atlas/cases/castle-rock-res-2025-071.v2.json').read_text())
    if revision['source_id'] in json.dumps(case):
        raise ValueError('Published case still relies on volatile HTML')
    registry = json.loads((ROOT / 'fixtures/atlas/sources.co.v1.json').read_text())
    row = next(row for row in registry['sources'] if row['id'] == revision['source_id'])
    if row['fixture'] != revision['captured']['fixture'] or row['sha256'] != revision['captured']['sha256']:
        raise ValueError('Registry does not preserve the captured revision')
    print('Castle Rock: exact prior/captured bytes preserved; only 2 state fields and 11 resource timestamps differ.')


if __name__ == '__main__':
    main()
