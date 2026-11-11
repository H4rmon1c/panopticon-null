#!/usr/bin/env python3
"""Private DOLA tenant-page reconnaissance. Outputs leads, never published claims."""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
from html.parser import HTMLParser
import importlib.util
import json
import os
from pathlib import Path
import re
from urllib.parse import urlsplit, urlunsplit

import colorado_special_districts as districts

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('colorado_research', ROOT / 'scripts/colorado-research.py')
research = importlib.util.module_from_spec(spec)
spec.loader.exec_module(research)


def leads(source, geography):
    rows = json.loads(source)
    if not isinstance(rows, list) or len(rows) > 5000 or not all(isinstance(row, dict) for row in rows):
        raise ValueError('invalid bounded district directory')
    mapping = {x['code']: x for x in geography if x['kind'] == 'district_provisional'}
    if len(mapping) != sum(x['kind'] == 'district_provisional' for x in geography):
        raise ValueError('duplicate district geography ID')
    ids = [row.get('lgid') for row in rows if row.get('lgid')]
    if any(not isinstance(x, str) or not re.fullmatch(r'\d{5}', x) for x in ids) or len(ids) != len(set(ids)):
        raise ValueError('invalid or duplicate district ID')
    result = []
    for row in rows:
        item = mapping.get(row.get('lgid'))
        parsed = districts.website(row.get('url'))
        if (not item or not parsed or row.get('lgstatus_i') != '1.0' or item['name'] != row.get('name')
                or item['id'] != f"dola-district-{row.get('lgid')}"):
            continue
        parts, host = parsed
        path = parts.path
        if (host in districts.SHARED_HOSTS or host == 'sites.google.com' or host.endswith('.legistar.com')
                or not path or path == '/' or parts.query or parts.fragment
                or not re.fullmatch(r'/[A-Za-z0-9_./~-]{1,300}', path)
                or any(segment in ('.', '..') for segment in path.split('/'))):
            continue
        url = urlunsplit(('https', parts.hostname.lower(), path, '', ''))
        result.append((item['id'], item['name'], url))
    # A shared exact page cannot establish which of its tenants owns a link.
    def key(url):
        parsed = urlsplit(url)
        return parsed.hostname.removeprefix('www.'), parsed.path.rstrip('/')
    counts = Counter(key(url) for _, _, url in result)
    return sorted(x for x in result if counts[key(x[2])] == 1)


class Headings(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.current = None
        self.buffer = ''
        self.headings = []
        self.hidden = 0

    def handle_starttag(self, tag, attrs):
        if tag in ('script', 'style', 'template'):
            self.hidden += 1
        if not self.hidden and tag in ('h1', 'h2') and self.current is None:
            self.current, self.buffer = tag, ''

    def handle_endtag(self, tag):
        if tag == self.current:
            self.headings.append(' '.join(self.buffer.split())[:1000])
            self.current, self.buffer = None, ''
        if tag in ('script', 'style', 'template') and self.hidden:
            self.hidden -= 1

    def handle_data(self, data):
        if self.current and not self.hidden:
            self.buffer += data[:2000]


def heading_matches(body, name):
    def words(value):
        return ' '.join(re.findall(r'[^\W_]+', value.casefold().replace('&', ' and ')))
    expected = words(name)
    if len(expected) < 12 or len(expected.split()) < 3:
        return False
    parser = Headings()
    parser.feed(body.decode('utf8', 'replace'))
    return sum(words(h) == expected for h in parser.headings) == 1


def inspect(entry, budget):
    ref, name, url = entry
    host = urlsplit(url).hostname
    result = {'jurisdiction_ref': ref, 'name': name, 'url': url, 'state': 'unavailable', 'documents': []}
    observations = []
    try:
        rules, rows = research.get_robots(host, budget=budget)
        observations.extend(rows)
        if not research.allowed(rules, url):
            result['reason'] = 'robots disallow'
            return result, observations
        # Redirects must not silently turn a tenant page into a different page.
        rows = research.get(url, {host}, maximum=research.MAX_PAGE_BYTES, redirects=0,
                            stop_at_redirect=True, budget=budget)
        observations.extend(rows)
        meta, body = rows[-1]
        if meta['status'] != 200 or 'html' not in meta.get('content_type', '').lower():
            result['reason'] = 'page unavailable or redirected'
        elif not heading_matches(body, name):
            result['reason'] = 'exact district heading absent'
        else:
            links, base = research.page_links(body, url)
            for ordinal, (href, label) in enumerate(links):
                target = research.resolve_page_link(url, base, href)
                if not target or urlsplit(target).hostname != host:
                    continue
                kind = research.candidate_kind(target, url, label)
                if kind and len(result['documents']) < 200:
                    result['documents'].append({'url': target, 'anchor_index': ordinal,
                                                'kind': kind[0], 'label': ' '.join(label.split())[:160]})
            result.update(state='identity_heading_matched', source_sha256=research.sha(body),
                          captured_at=meta['retrieved_at'])
    except research.RobotsDenied as error:
        observations.extend(error.observations)
        result['reason'] = str(error)[:80]
    except (ValueError, OSError, UnicodeError, RuntimeError) as error:
        result['reason'] = type(error).__name__
    return result, observations


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data-dir', type=Path, required=True)
    parser.add_argument('--limit', type=int, default=300)
    parser.add_argument('--workers', type=int, default=4)
    args = parser.parse_args()
    if not 1 <= args.limit <= 500 or not 1 <= args.workers <= 8:
        parser.error('invalid bounded limit or worker count')
    db = research.root_store(args.data_dir)
    db.close()
    geography = json.loads(research.GEOGRAPHY.read_bytes())['jurisdictions']
    selected = leads(districts.pinned_source(), geography)[:args.limit]
    budget = research.DownloadBudget(200 * 1024 * 1024)
    results = []
    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        futures = [pool.submit(inspect, item, budget) for item in selected]
        for i, future in enumerate(as_completed(futures), 1):
            result, observations = future.result()
            research.archive(args.data_dir, observations)
            results.append(result)
            if i % 25 == 0 or i == len(futures):
                print(f'Checked {i}/{len(futures)} tenant pages', flush=True)
    results.sort(key=lambda x: x['jurisdiction_ref'])
    out = args.data_dir / 'tenant-leads.json'
    encoded = (json.dumps({'notice': 'Private discovery leads; no attribution or decision established.',
                           'results': results}, indent=2) + '\n').encode()
    fd = os.open(out, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'wb') as handle:
        handle.write(encoded)
        handle.flush()
        os.fsync(handle.fileno())
    print('Exact heading matches:', sum(x['state'] == 'identity_heading_matched' for x in results),
          'direct document leads:', sum(len(x['documents']) for x in results))


if __name__ == '__main__':
    main()
