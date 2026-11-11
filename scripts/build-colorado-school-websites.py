#!/usr/bin/env python3
"""Build school website leads by joining CDE district codes to NCES IDs.

The output is a directory of discovery leads, not evidence of a decision.
Inputs are pinned public source captures; no contact details are exported.
"""
import argparse
from collections import defaultdict
import csv
import hashlib
from html.parser import HTMLParser
import io
import json
from pathlib import Path
import re
from urllib.parse import urlsplit
import zipfile

ROOT = Path(__file__).resolve().parents[1]
CDE_URL = 'https://ed.cde.state.co.us/district-and-boces-websites-and-maps'
CDE_SHA = 'c88dc2680759fe648b1be1f9fa402cb62dce7bc144957db11afdceb514992474'
NCES_URL = 'https://nces.ed.gov/ccd/Data/zip/ccd_lea_029_2425_w_1a_073025.zip'
NCES_SHA = '2745169e4bc7cd53adff179830c3d4b9bb035ac79580ecea01ef389a161936a9'
SHARED_HOSTS = {'sites.google.com', 'facebook.com', 'www.facebook.com'}


def read_pinned(path, digest, maximum):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > maximum:
        raise ValueError('source must be a bounded regular file')
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != digest:
        raise ValueError('source digest differs from pinned capture')
    return data


class DirectoryItems(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.depth = 0
        self.text = ''
        self.links = []
        self.current = None
        self.rows = []

    def handle_starttag(self, tag, attrs):
        if tag == 'li':
            if self.depth == 0:
                self.text, self.links = '', []
            self.depth += 1
        elif self.depth and tag == 'a':
            self.current = [dict(attrs).get('href', ''), '']

    def handle_data(self, data):
        if self.depth:
            self.text += data
        if self.current is not None:
            self.current[1] += data

    def handle_endtag(self, tag):
        if tag == 'a' and self.current is not None:
            self.links.append(tuple(self.current))
            self.current = None
        if tag == 'li' and self.depth:
            self.depth -= 1
            if self.depth == 0:
                self.rows.append((self.text, self.links))


def cde_links(data):
    parser = DirectoryItems()
    parser.feed(data.decode('utf8'))
    codes = defaultdict(set)
    for text, links in parser.rows:
        identifiers = set(re.findall(r'(?<!\d)(\d{4})(?!\d)', text))
        if len(identifiers) != 1 or len(links) != 1:
            continue
        url, name = links[0]
        try:
            parsed = urlsplit(url)
            if (parsed.scheme not in ('http', 'https') or not parsed.hostname or parsed.username or parsed.password
                    or parsed.port not in (None, 80, 443) or parsed.query or parsed.fragment or len(url) > 2000):
                continue
        except ValueError:
            continue
        codes[next(iter(identifiers))].add((url, ' '.join(name.split())))
    return codes


def nces_ids(data):
    codes = defaultdict(set)
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        members = [item for item in archive.infolist() if item.filename.lower().endswith('.csv')]
        if len(members) != 1 or members[0].file_size > 50_000_000:
            raise ValueError('invalid bounded NCES directory')
        with archive.open(members[0]) as binary:
            reader = csv.DictReader(io.TextIOWrapper(binary, encoding='utf-8-sig', newline=''))
            if not {'ST', 'ST_LEAID', 'LEAID'} <= set(reader.fieldnames or []):
                raise ValueError('missing NCES identity fields')
            for row in reader:
                if (row['ST'] == 'CO' and re.fullmatch(r'CO-\d{4}', row['ST_LEAID'] or '')
                        and re.fullmatch(r'08\d{5}', row['LEAID'] or '')):
                    codes[row['ST_LEAID'][3:]].add(row['LEAID'])
    return codes


def build(cde, nces, geography):
    districts = {item['code']: item for item in geography if item['kind'] == 'school_district'}
    ids = nces_ids(nces)
    entries = []
    mapped = set()
    for state_code, values in cde_links(cde).items():
        matches = ids.get(state_code, set())
        names = {name for _, name in values}
        urls = {url for url, _ in values}
        if len(matches) != 1 or len(names) != 1:
            continue
        nces_id = next(iter(matches))
        if nces_id not in districts or nces_id in mapped:
            continue
        # A directory may list both a district homepage and a path on that
        # same host. Prefer the unique homepage; never merge different hosts.
        if len(urls) > 1:
            hosts = {urlsplit(url).hostname for url in urls}
            roots = {url for url in urls if urlsplit(url).path in ('', '/')}
            if len(hosts) != 1 or len(roots) != 1:
                continue
            url = next(iter(roots))
        else:
            url = next(iter(urls))
        if urlsplit(url).hostname.lower() in SHARED_HOSTS:
            continue
        mapped.add(nces_id)
        entries.append({'jurisdiction_ref': districts[nces_id]['id'], 'listing_name': next(iter(names)),
                        'website_url': url, 'state_district_code': state_code, 'nces_district_id': nces_id,
                        'source_locator': f'CDE district code {state_code}',
                        'crosswalk_locator': f'NCES ST_LEAID CO-{state_code}'})
    entries.sort(key=lambda item: item['jurisdiction_ref'])
    missing = sorted(({'jurisdiction_ref': value['id'], 'name': value['name']} for key, value in districts.items()
                      if key not in mapped), key=lambda item: item['jurisdiction_ref'])
    return {'schema_version': 1,
            'purpose': 'School district website discovery leads only; no source policy, decision, vote, or contract is verified.',
            'sources': {'cde_directory': {'url': CDE_URL, 'sha256': CDE_SHA},
                        'nces_directory': {'url': NCES_URL, 'sha256': NCES_SHA}, 'archived_on': '2026-11-11'},
            'counts': {'geography_school_districts': len(districts), 'with_website_lead': len(entries),
                       'without_website_lead': len(missing)},
            'school_districts': entries, 'unmapped': missing}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cde', type=Path, required=True)
    parser.add_argument('--nces', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    cde = read_pinned(args.cde, CDE_SHA, 2_000_000)
    nces = read_pinned(args.nces, NCES_SHA, 20_000_000)
    geography = json.loads((ROOT / 'web/panopticon-terminal/geography/colorado-jurisdictions.json').read_text())['jurisdictions']
    report = build(cde, nces, geography)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + '\n')
    print('School directory leads:', report['counts'])


if __name__ == '__main__':
    main()
