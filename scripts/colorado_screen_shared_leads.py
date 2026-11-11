#!/usr/bin/env python3
"""Screen shared-host document links privately without district attribution."""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import json
import os
from pathlib import Path
from threading import Lock
from urllib.parse import urlsplit
import colorado_shared_districts as shared

r = shared.r


def validated_jobs(data_dir, manifest):
    geo = json.loads(r.GEOGRAPHY.read_bytes())['jurisdictions']
    allowed = dict(shared.leads(shared.d.pinned_source(), geo))
    receipts = r.observation_keys(data_dir)
    results = manifest.get('results')
    if not isinstance(results, list) or len(results) > 300:
        raise ValueError('invalid shared-host results')
    jobs, seen_hosts, seen_urls = [], set(), set()
    for item in results:
        if item.get('state') != 'site_checked':
            continue
        host, source, digest = item['host'], item['source_url'], item['sha256']
        parsed = urlsplit(source)
        if (host in seen_hosts or host not in allowed or parsed.scheme != 'https'
                or parsed.hostname != allowed[host][0][2] or parsed.path != '/'
                or parsed.query or parsed.fragment or (source, digest) not in receipts):
            raise ValueError('shared host does not match directory and receipt')
        seen_hosts.add(host)
        documents = item.get('documents')
        if not isinstance(documents, list) or len(documents) > 100:
            raise ValueError('invalid shared-host documents')
        for document in documents:
            candidate = dict(document, index_url=source, index_sha256=digest)
            if urlsplit(candidate['url']).hostname != parsed.hostname:
                raise ValueError('cross-host document')
            r.checked_document_candidate(data_dir, candidate['url'], candidate)
            if candidate['url'] not in seen_urls:
                jobs.append((host, candidate))
                seen_urls.add(candidate['url'])
    return jobs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data-dir', type=Path, required=True)
    parser.add_argument('--pnull-bin', type=Path, required=True)
    parser.add_argument('--offset', type=int, default=0)
    parser.add_argument('--limit', type=int, default=100)
    parser.add_argument('--workers', type=int, default=4)
    args = parser.parse_args()
    if (not args.data_dir.is_absolute() or not args.pnull_bin.is_absolute() or not args.pnull_bin.is_file()
            or args.pnull_bin.is_symlink() or args.offset < 0 or not 1 <= args.limit <= 300
            or not 1 <= args.workers <= 4):
        parser.error('invalid bounded parameters')
    db = r.root_store(args.data_dir)
    db.close()
    manifest = json.loads(r.read_blob(args.data_dir / f'shared-host-leads-{args.offset}.json', 8 * 1024 * 1024))
    jobs = validated_jobs(args.data_dir, manifest)[:args.limit]
    output = args.data_dir / f'shared-host-screening-{args.offset}.json'
    fd = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        locks = {urlsplit(c['url']).hostname: Lock() for _, c in jobs}
        budget, robots, results = r.DownloadBudget(200 * 1024 * 1024), {}, []
        with ThreadPoolExecutor(max_workers=args.workers) as pool:
            futures = {pool.submit(r.screen_one, args.data_dir, 'us-co-shared-site', candidate, str(args.pnull_bin),
                                   locks[urlsplit(candidate['url']).hostname], budget, False, robots): (host, candidate)
                       for host, candidate in jobs}
            for index, future in enumerate(as_completed(futures), 1):
                host, candidate = futures[future]
                result, observations = future.result()
                r.archive(args.data_dir, observations)
                results.append({'host': host, 'candidate': candidate, 'result': result})
                if index % 10 == 0 or index == len(futures):
                    print(f'Screened {index}/{len(futures)} shared-site document leads', flush=True)
        with os.fdopen(fd, 'w') as handle:
            json.dump({'notice': 'Private shared-site screens; no district, vote, or decision attribution established.',
                       'results': results}, handle, indent=2)
            handle.write('\n'); handle.flush(); os.fsync(handle.fileno())
    except BaseException:
        try:
            os.close(fd)
        except OSError:
            pass
        output.unlink(missing_ok=True)
        raise
    print('Matches:', sum(x['result']['state'] == 'matched' for x in results),
          'errors:', sum(x['result']['state'] == 'error' for x in results))


if __name__ == '__main__':
    main()
