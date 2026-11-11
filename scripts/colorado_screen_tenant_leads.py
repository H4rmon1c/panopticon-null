#!/usr/bin/env python3
"""Screen privately preserved tenant candidates; never create public claims."""
import argparse
import json
import os
from pathlib import Path
from threading import Lock
from urllib.parse import urlsplit
from concurrent.futures import ThreadPoolExecutor, as_completed
import colorado_district_tenants as tenants

r = tenants.research


def validated_jobs(data_dir, manifest):
    geography = json.loads(r.GEOGRAPHY.read_bytes())['jurisdictions']
    allowed = {(ref, name, url) for ref, name, url in tenants.leads(tenants.districts.pinned_source(), geography)}
    receipts = r.observation_keys(data_dir)
    results = manifest.get('results')
    if not isinstance(results, list) or len(results) > 500:
        raise ValueError('invalid tenant lead list')
    jobs = []
    for item in results:
        if item.get('state') != 'identity_heading_matched':
            continue
        ref, name, url, digest = (item[x] for x in ('jurisdiction_ref', 'name', 'url', 'source_sha256'))
        if (ref, name, url) not in allowed or (url, digest) not in receipts:
            raise ValueError('tenant lead has no preserved source receipt')
        body = r.source_blob(data_dir, digest)
        if not tenants.heading_matches(body, name):
            raise ValueError('tenant source no longer proves its exact heading')
        docs = item.get('documents')
        if not isinstance(docs, list) or len(docs) > 200:
            raise ValueError('invalid bounded tenant documents')
        for document in docs:
            candidate = dict(document, index_url=url, index_sha256=digest)
            if urlsplit(candidate['url']).hostname != urlsplit(url).hostname:
                raise ValueError('cross-host tenant document')
            r.checked_document_candidate(data_dir, candidate['url'], candidate)
            jobs.append((ref, candidate))
    return jobs


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--data-dir', type=Path, required=True)
    p.add_argument('--pnull-bin', type=Path, required=True)
    p.add_argument('--limit', type=int, default=100)
    p.add_argument('--workers', type=int, default=2)
    a = p.parse_args()
    if not a.data_dir.is_absolute() or not a.pnull_bin.is_absolute() or not a.pnull_bin.is_file() or a.pnull_bin.is_symlink():
        p.error('absolute data directory and regular pnull binary required')
    if not 1 <= a.limit <= 500 or not 1 <= a.workers <= 4:
        p.error('invalid bounded limit or worker count')
    db = r.root_store(a.data_dir)
    db.close()
    path = a.data_dir / 'tenant-leads.json'
    manifest = json.loads(r.read_blob(path, 8 * 1024 * 1024))
    jobs = validated_jobs(a.data_dir, manifest)[:a.limit]
    locks = {urlsplit(candidate['url']).hostname: Lock() for _, candidate in jobs}
    budget, robots = r.DownloadBudget(200 * 1024 * 1024), {}
    results = []
    with ThreadPoolExecutor(max_workers=a.workers) as pool:
        futures = {pool.submit(r.screen_one, a.data_dir, ref, candidate, str(a.pnull_bin),
                   locks[urlsplit(candidate['url']).hostname], budget, False, robots): (ref, candidate)
                   for ref, candidate in jobs}
        for i, future in enumerate(as_completed(futures), 1):
            ref, candidate = futures[future]
            result, obs = future.result()
            r.archive(a.data_dir, obs)
            results.append({'jurisdiction_ref': ref, 'candidate': candidate, 'result': result})
            print(f'Screened {i}/{len(futures)} tenant document leads', flush=True)
    out = a.data_dir / 'tenant-screening.json'
    fd = os.open(out, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'w') as f:
        json.dump({'notice': 'Private screening leads; district ownership and approval not established.',
                   'results': results}, f, indent=2)
        f.write('\n'); f.flush(); os.fsync(f.fileno())
    print('Matches:', sum(x['result']['state'] == 'matched' for x in results),
          'errors:', sum(x['result']['state'] == 'error' for x in results))


if __name__ == '__main__':
    main()
