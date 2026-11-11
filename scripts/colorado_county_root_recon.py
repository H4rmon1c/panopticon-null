#!/usr/bin/env python3
"""Private county-root reconnaissance for previously reached but unscreened sites.

Source URLs are discovered from an existing private research snapshot/store.
The output cannot establish the responsible public body of a linked document.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import importlib.util
import json
import os
from pathlib import Path
import sqlite3

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('research', ROOT / 'scripts/colorado-research.py')
r = importlib.util.module_from_spec(spec); spec.loader.exec_module(r)


def unscreened(snapshot, geography):
    ids = {j['id'] for j in geography if j['kind'] == 'county'}
    attempts = snapshot.get('coverage', {}).get('search_attempts', [])
    if not isinstance(attempts, list) or len(attempts) > 5000:
        raise ValueError('invalid search attempts')
    seen = set(); result = []
    for item in attempts:
        if not isinstance(item, dict):
            raise ValueError('invalid county attempt')
        ref = item.get('jurisdiction_ref')
        if ref not in ids:
            continue
        if ref in seen:
            raise ValueError('duplicate county attempt')
        seen.add(ref)
        if any(type(item.get(k, 0)) is not int or item.get(k, 0) < 0 for k in
               ('documents_screened', 'portal_pages_checked', 'agenda_meeting_pages_screened')):
            raise ValueError('invalid county screening counts')
        if item.get('website_check') == 'site_checked' and not any(item.get(k, 0) for k in
                ('documents_screened', 'portal_pages_checked', 'agenda_meeting_pages_screened')):
            result.append(ref)
    return result


def private_result(item):
    if not isinstance(item, dict) or not isinstance(item.get('portals'), list):
        raise ValueError('invalid discovery result')
    # A county website can link a different municipality's or a shared
    # legislative portal. Never promote these links from reconnaissance.
    return {**item, 'portals': []}, len(item['portals'])


def create_private_archive(path):
    # This format deliberately has no research.sqlite3. Normal collection and
    # export cannot consume these county/body-unverified JSON leads as rows.
    public = (ROOT / 'web/panopticon-terminal').resolve()
    if path.resolve() == public or public in path.resolve().parents:
        raise ValueError('private archive must not be under the public web root')
    path.mkdir(mode=0o700, parents=False, exist_ok=False)
    (path / 'content').mkdir(mode=0o700)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--private-dir', type=Path, required=True)
    p.add_argument('--source-db', type=Path, required=True)
    p.add_argument('--snapshot', type=Path, required=True)
    p.add_argument('--limit', type=int, default=64)
    p.add_argument('--workers', type=int, default=4)
    a = p.parse_args()
    if (not all(x.is_absolute() for x in (a.private_dir, a.source_db, a.snapshot)) or
            not 1 <= a.limit <= 64 or not 1 <= a.workers <= 4 or
            a.source_db.is_symlink() or not a.source_db.is_file() or a.snapshot.is_symlink() or not a.snapshot.is_file()):
        p.error('invalid paths or bounds')
    if a.private_dir.exists() or a.private_dir.is_symlink():
        p.error('choose a fresh private output directory')
    snapshot = json.loads(r.read_blob(a.snapshot, 8 * 1024 * 1024))
    geography = json.loads(r.read_blob(r.GEOGRAPHY, 30 * 1024 * 1024))['jurisdictions']
    names = {x['id']: x['name'] for x in geography if x['kind'] == 'county'}
    candidates = unscreened(snapshot, geography)[:a.limit]
    with sqlite3.connect(a.source_db.as_uri() + '?mode=ro', uri=True) as source:
        leads = []
        for ref in candidates:
            row = source.execute('SELECT source_url FROM discovery WHERE jurisdiction_ref=?', (ref,)).fetchone()
            if row: leads.append((ref, names[ref], row[0]))
    create_private_archive(a.private_dir)
    budget = r.DownloadBudget(150 * 1024 * 1024)
    results = []
    with ThreadPoolExecutor(max_workers=a.workers) as pool:
        jobs = {pool.submit(r.discover_one, *item, budget, county_root_recon=True): item for item in leads}
        for future in as_completed(jobs):
            ref, name, url = jobs[future]
            try:
                item, observations = future.result()
                r.archive(a.private_dir, observations)
                # Do not map county-hosted multi-agency legislative portals.
                item, portal_leads = private_result(item)
                results.append({'jurisdiction_ref': ref, 'state': item['state'],
                                'document_links': len(item['documents']), 'excluded_portals': portal_leads,
                                'unverified_discovery': item})
                print(name, item['state'], 'document links', len(item['documents']), flush=True)
            except (OSError, UnicodeError, ValueError, RuntimeError) as error:
                results.append({'jurisdiction_ref': ref, 'state': 'failed', 'error': type(error).__name__})
    pending = sum(x.get('document_links', 0) for x in results)
    output = {'state': 'private_unverified_county_hosted_leads', 'notice': 'County-hosted pages may concern other bodies; a county decision, contract, or vote is not established.',
              'results': results, 'unverified_document_links': pending}
    fd = os.open(a.private_dir / 'county-root-recon.json', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'w') as stream:
        json.dump(output, stream, indent=2); stream.write('\n')
    print('County-root reconnaissance complete:', len(results), 'sites,', pending, 'unverified document links')

if __name__ == '__main__': main()
