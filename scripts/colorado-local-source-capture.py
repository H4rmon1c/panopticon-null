#!/usr/bin/env python3
"""Capture explicitly configured local-news leads into the private archive.

These are third-party leads only. They never create an official vote, contract,
or a public research record. Each exact HTML source and HTTP receipt is retained.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('colorado_research', ROOT / 'scripts/colorado-research.py')
research = importlib.util.module_from_spec(spec)
spec.loader.exec_module(research)
SEEDS = ROOT / 'configs/states/co-local-source-seeds.v1.json'
TRUSTED_HOSTS = {'kiowacountypress.net', 'kiowacountyindependent.com'}
KINDS = {'third_party_sheriff_statement', 'third_party_legal_notice'}


def validate_seeds(data):
    if (not isinstance(data, dict) or data.get('schema_version') != 1 or
            not isinstance(data.get('sources'), list) or len(data['sources']) > 100):
        raise ValueError('invalid bounded local-source manifest')
    seen = set()
    for item in data['sources']:
        if not isinstance(item, dict) or set(item) != {'jurisdiction_ref', 'url', 'source_type'}:
            raise ValueError('invalid local-source entry')
        if (not isinstance(item['jurisdiction_ref'], str)
                or not re.fullmatch(r'us-co-county-\d{5}', item['jurisdiction_ref'])
                or not isinstance(item['source_type'], str) or item['source_type'] not in KINDS):
            raise ValueError('invalid local-source identity or type')
        try:
            url = urlsplit(item['url'])
            if (url.scheme != 'https' or url.hostname not in TRUSTED_HOSTS or url.port not in (None, 443)
                    or url.username or url.password or url.fragment or url.query or not url.path.startswith('/')
                    or item['url'] in seen):
                raise ValueError('unsafe local-source URL')
        except (TypeError, ValueError):
            raise ValueError('unsafe local-source URL') from None
        seen.add(item['url'])
    return data['sources']


def collect(data_dir, sources):
    results = []
    budget = research.DownloadBudget(20 * 1024 * 1024)
    for item in sources:
        url = item['url']
        host = urlsplit(url).hostname
        try:
            rules, robots = research.get_robots(host, budget=budget)
            research.archive(data_dir, robots)
            if not research.allowed(rules, url):
                raise ValueError('source path disallowed')
            rows = research.get(url, {host}, maximum=2_000_000, timeout=30, redirects=0, budget=budget)
            research.archive(data_dir, rows)
            receipt, body = rows[-1]
            if receipt['status'] != 200 or 'html' not in receipt.get('content_type', '').lower():
                raise ValueError('source did not return HTML')
            visible = ' '.join(research.visible_text(body))
            if len(visible) < 50:
                raise ValueError('source has insufficient readable text')
            hits = sorted(term for term in research.TERMS if term.casefold() in visible.casefold())
            results.append({**item, 'status': 'captured', 'sha256': receipt['sha256'],
                            'captured_at': receipt['retrieved_at'], 'signals': hits})
        except (OSError, UnicodeError, ValueError, RuntimeError) as exc:
            results.append({**item, 'status': 'error', 'error': type(exc).__name__})
    return {'schema_version': 1, 'state': 'private_third_party_leads', 'sources': results}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data-dir', required=True, type=Path)
    args = parser.parse_args()
    data_dir = args.data_dir
    db = research.root_store(data_dir)
    try:
        seeds = validate_seeds(json.loads(research.read_blob(SEEDS, 50_000)))
        result = collect(data_dir, seeds)
        output = data_dir / 'local-source-leads.json'
        temporary = data_dir / ('.local-source-leads.' + str(os.getpid()) + '.tmp')
        fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        try:
            with os.fdopen(fd, 'w') as handle:
                json.dump(result, handle, indent=2)
                handle.write('\n')
            os.replace(temporary, output)
        finally:
            temporary.unlink(missing_ok=True)
        print(f"Captured {sum(x['status'] == 'captured' for x in result['sources'])} of {len(seeds)} third-party leads; no public records created")
    finally:
        db.close()


if __name__ == '__main__':
    main()
