#!/usr/bin/env python3
"""Compile source-reviewed context seeds into the strict v1 browser allowlist."""
import argparse
import json
from pathlib import Path
import re
from urllib.parse import urlsplit
import colorado_official_context as context

ROOT = Path(__file__).resolve().parents[1]
API = ROOT / 'web/panopticon-terminal/research-api.js'
BEGIN = '      // BEGIN GENERATED OFFICIAL CONTEXT ALLOWLIST\n'
END = '      // END GENERATED OFFICIAL CONTEXT ALLOWLIST\n'
GEOGRAPHY = ROOT / 'web/panopticon-terminal/geography/colorado-jurisdictions.json'


def allowlist(seeds):
    if not isinstance(seeds, dict) or not seeds or len(seeds) > 500:
        raise ValueError('invalid reviewed context registry')
    municipalities = {}
    for row in json.loads(GEOGRAPHY.read_text())['jurisdictions']:
        if row.get('kind') == 'municipality':
            if row['id'] in municipalities:
                raise ValueError('duplicate municipality in geography')
            municipalities[row['id']] = re.sub(r'\s+(?:city|town)$', '', row['name'], flags=re.I)
    records = {}
    for name, seed in sorted(seeds.items()):
        if not isinstance(name, str) or not re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*', name):
            raise ValueError('invalid context seed id')
        if not isinstance(seed, dict) or not re.fullmatch(r'us-co-place-[0-9]{7}', seed.get('jurisdiction_ref', '')):
            raise ValueError('invalid jurisdiction binding')
        for key, limit in [('jurisdiction', 300), ('url', 2000), ('title', 400), ('agency', 1000), ('claim', 1000)]:
            value = seed.get(key)
            if not isinstance(value, str) or not value.strip() or len(value) > limit or re.search(r'[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]', value):
                raise ValueError('invalid context registry text')
        if municipalities.get(seed['jurisdiction_ref']) != seed['jurisdiction']:
            raise ValueError('context name and municipality identity do not match geography')
        url = urlsplit(seed['url'])
        if (url.scheme != 'https' or not url.hostname or url.username or url.password or url.port not in (None, 443)
                or url.query or url.fragment):
            raise ValueError('invalid context source URL')
        if not re.fullmatch(r'[a-f0-9]{64}', seed.get('context_sha256', '')):
            raise ValueError('missing pinned contextual passage')
        mode = bool(seed.get('combined_start'))
        if mode == bool(seed.get('section_id') or seed.get('tab_panel_id')) or (seed.get('section_id') and seed.get('tab_panel_id')):
            raise ValueError('invalid source binding mode')
        if mode:
            start = seed['combined_start']
            if not isinstance(start, str) or not start.startswith(seed['agency']) or seed['claim'] not in start:
                raise ValueError('combined statement does not bind agency and claim')
        else:
            scope = seed.get('section_id') or seed.get('tab_panel_id')
            if not isinstance(scope, str) or not re.fullmatch(r'[A-Za-z0-9_-]{1,150}', scope):
                raise ValueError('invalid context section identifier')
        signals = seed.get('signals')
        if not isinstance(signals, list) or not signals or len(signals) > 24 or any(
                not isinstance(x, str) or not x.strip() or len(x) > 100 or re.search(r'[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]', x) for x in signals):
            raise ValueError('invalid technology signals')
        records['co-official-context-' + name] = [seed['jurisdiction_ref'], seed['jurisdiction'], seed['url'], seed['title'], signals]
    return records


def generated(seeds):
    lines = [BEGIN, '      const allowed = {\n']
    for key, value in allowlist(seeds).items():
        lines.append('        ' + json.dumps(key, ensure_ascii=True) + ': ' + json.dumps(value, ensure_ascii=True) + ',\n')
    lines.extend(['      }[record.id];\n', END])
    return ''.join(lines)


def sync(source, seeds):
    if source.count(BEGIN) != 1 or source.count(END) != 1:
        raise ValueError('generated allowlist markers missing or duplicated')
    start = source.index(BEGIN); end = source.index(END, start) + len(END)
    return source[:start] + generated(seeds) + source[end:]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--write', action='store_true', help='update the checked-in browser allowlist')
    args = p.parse_args()
    old = API.read_text(); new = sync(old, context.SEEDS)
    if args.write:
        API.write_text(new)
    elif old != new:
        raise SystemExit('official context allowlist is out of sync; run this script with --write')
    print('Official context allowlist matches reviewed source registry.')

if __name__ == '__main__': main()
