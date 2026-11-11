#!/usr/bin/env python3
"""Bounded EFF Colorado search capture; results are private unverified leads."""
import argparse
from html.parser import HTMLParser
import importlib.util
import json
import os
from pathlib import Path
import re
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('research', ROOT / 'scripts/colorado-research.py')
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)
HOST = 'www.atlasofsurveillance.org'

class Entries(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.row = None
        self.record = None
        self.cell = None
        self.source_links = False
        self.entries = []
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == 'tr':
            classes = (attrs.get('class') or '').split()
            if attrs.get('data-ref') and 'desktop-only' in classes:
                self.row = 'record'; self.record = {'ref': attrs['data-ref'], 'cells': [], 'links': []}
            elif self.record and 'desktop-only' in classes:
                self.row = 'details'
            else:
                self.row = None
        elif tag == 'td' and self.row == 'record':
            self.cell = ''
        elif tag == 'p' and self.row == 'details' and 'links' in (attrs.get('class') or '').split():
            self.source_links = True
        elif tag == 'a' and self.row == 'details' and self.source_links and attrs.get('href'):
            self.record['links'].append(attrs['href'])
    def handle_data(self, data):
        if self.cell is not None:
            self.cell += data
    def handle_endtag(self, tag):
        if tag == 'td' and self.cell is not None:
            self.record['cells'].append(' '.join(self.cell.split())); self.cell = None
        if tag == 'p' and self.source_links:
            self.source_links = False
        if tag == 'tr':
            if self.row == 'details' and self.record:
                self.entries.append(self.record); self.record = None
            self.row = None

def parse_entries(body):
    parser = Entries(); parser.feed(body.decode('utf-8', 'strict'))
    if len(parser.entries) > 100:
        raise ValueError('EFF page exceeds expected page size')
    refs = set()
    for item in parser.entries:
        if (not re.fullmatch(r'AOS[0-9]{6,12}', item['ref']) or item['ref'] in refs or
                len(item['cells']) != 6 or item['cells'][3] != 'CO' or len(item['links']) > 30 or
                any(len(x) > 1000 for x in item['cells'])):
            raise ValueError('invalid EFF row')
        refs.add(item['ref'])
    return parser.entries

def safe_official_url(url, hosts, include_gov=False):
    try:
        parsed = urlsplit(url)
        host = parsed.hostname or ''
        is_gov = include_gov and bool(re.fullmatch(r'(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+gov', host))
        return (parsed.scheme == 'https' and (host in hosts or is_gov) and parsed.port in (None, 443)
                and not parsed.username and not parsed.password and not parsed.fragment and not parsed.query
                and len(url) <= 2000)
    except (ValueError, TypeError):
        return False

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--data-dir', type=Path, required=True)
    p.add_argument('--source-db', type=Path, required=True)
    p.add_argument('--pages', type=int, default=4)
    p.add_argument('--official-limit', type=int, default=50)
    p.add_argument('--include-gov', action='store_true', help='also capture public .gov source links as unverified leads')
    args = p.parse_args()
    if not args.data_dir.is_absolute() or not args.source_db.is_absolute() or not 1 <= args.pages <= 10 or not 0 <= args.official_limit <= 100:
        p.error('invalid bounded options')
    if args.source_db.is_symlink() or not args.source_db.is_file():
        p.error('source database must be a regular file')
    import sqlite3
    uri = args.source_db.as_uri() + '?mode=ro'
    with sqlite3.connect(uri, uri=True) as db:
        rows = db.execute("select source_url,result_json from discovery").fetchall()
    official_hosts = set()
    for url, raw in rows:
        try:
            if json.loads(raw).get('state') != 'directory_site_checked': continue
            host = urlsplit(r.normalized_home(url)).hostname
            if host: official_hosts.add(host.lower())
        except (ValueError, TypeError, json.JSONDecodeError): continue
    db = r.root_store(args.data_dir); db.close()
    budget = r.DownloadBudget(120 * 1024 * 1024)
    rules, observations = r.get_robots(HOST, budget=budget); r.archive(args.data_dir, observations)
    entries = []; seen = set(); pages = []
    for page in range(1, args.pages + 1):
        url = f'https://{HOST}/search?location=Colorado&page={page}'
        if not r.allowed(rules, url): raise ValueError('EFF robots disallows search')
        observations = r.get(url, {HOST}, maximum=2_000_000, redirects=0, timeout=30, budget=budget)
        r.archive(args.data_dir, observations); meta, body = observations[-1]
        if meta['status'] != 200 or 'html' not in meta.get('content_type', '').lower(): raise ValueError('EFF search unavailable')
        found = parse_entries(body)
        if not found and page == 1:
            raise ValueError('EFF first page has no parseable records')
        pages.append({'url': url, 'sha256': meta['sha256'], 'captured_at': meta['retrieved_at'], 'rows': len(found)})
        for ordinal, item in enumerate(found):
            if item['ref'] in seen: raise ValueError('duplicate EFF record across pages')
            seen.add(item['ref']); entries.append({**item, 'index_url': url, 'index_sha256': meta['sha256'], 'row_ordinal': ordinal})
        if len(found) < 100: break
    sources = []; visited = set(); policy = {}
    for item in entries:
        for ordinal, url in enumerate(item['links']):
            if len(sources) >= args.official_limit: break
            if url in visited or not safe_official_url(url, official_hosts, args.include_gov): continue
            visited.add(url); host = urlsplit(url).hostname
            result = {'url': url, 'eff_ref': item['ref'], 'link_ordinal': ordinal, 'status': 'unavailable'}
            try:
                if host not in policy:
                    policy[host], obs = r.get_robots(host, budget=budget); r.archive(args.data_dir, obs)
                if not r.allowed(policy[host], url): raise ValueError('source disallowed')
                obs = r.get(url, {host}, maximum=5_000_000, redirects=0, timeout=30, budget=budget)
                r.archive(args.data_dir, obs); meta, body = obs[-1]
                result.update(status='captured' if meta['status'] == 200 else 'unavailable',
                              http_status=meta['status'], sha256=meta['sha256'], captured_at=meta['retrieved_at'])
            except (OSError, ValueError, RuntimeError) as error:
                result['error'] = type(error).__name__
            sources.append(result)
    output = args.data_dir / 'eff-colorado-private-leads.json'
    result = {'state': 'private_unverified_third_party_leads', 'notice': 'EFF references do not establish approval, vote, identity, or source attribution. Captured official-host links require independent verification.', 'last_page_partial': pages[-1]['rows'] < 100, 'pages': pages, 'entries': entries, 'official_host_sources': sources}
    fd = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'w') as file: json.dump(result, file, indent=2); file.write('\n')
    print(f"EFF Colorado: {len(entries)} private leads across {len(pages)} pages; {sum(s['status']=='captured' for s in sources)}/{len(sources)} allowed-host links fetched")

if __name__ == '__main__': main()
