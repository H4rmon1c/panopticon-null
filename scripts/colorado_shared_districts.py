#!/usr/bin/env python3
"""Capture shared district homepages as private, unattributed document leads."""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
import json
import os
from pathlib import Path
import re
from urllib.parse import urlsplit
import colorado_district_tenants as tenants

r = tenants.research
d = tenants.districts


def leads(source, geography):
    rows = json.loads(source)
    if not isinstance(rows, list) or len(rows) > 5000 or not all(isinstance(x, dict) for x in rows):
        raise ValueError('invalid bounded district directory')
    mapping = {x['code']: x for x in geography if x['kind'] == 'district_provisional'}
    if len(mapping) != sum(x['kind'] == 'district_provisional' for x in geography):
        raise ValueError('duplicate district geography ID')
    ids = [x.get('lgid') for x in rows if x.get('lgid')]
    if any(not isinstance(x, str) or not re.fullmatch(r'\d{5}', x) for x in ids) or len(ids) != len(set(ids)):
        raise ValueError('invalid district identifiers')
    groups = {}
    for row in rows:
        item = mapping.get(row.get('lgid'))
        parsed = d.website(row.get('url'))
        if (not item or not parsed or row.get('lgstatus_i') != '1.0' or item['name'] != row.get('name')
                or item['id'] != f"dola-district-{row.get('lgid')}"):
            continue
        parts, host = parsed
        if (parts.path not in ('', '/') or parts.query or parts.fragment or host in d.SHARED_HOSTS
                or host.endswith('.legistar.com')):
            continue
        groups.setdefault(host, []).append((item['id'], item['name'], parts.hostname))
    return sorted((host, sorted(items)) for host, items in groups.items() if len(items) >= 2)


def inspect(group, budget):
    host, items = group
    hostname = items[0][2]
    url = f'https://{hostname}/'
    result = {'host': host, 'directory_districts': [{'id': x[0], 'name': x[1]} for x in items],
              'state': 'unavailable', 'visible_names': [], 'documents': []}
    observations = []
    try:
        rules, rows = r.get_robots(hostname, budget=budget)
        observations.extend(rows)
        if not r.allowed(rules, url):
            return result, observations
        rows = r.get(url, {hostname}, maximum=r.MAX_PAGE_BYTES, redirects=0, stop_at_redirect=True, budget=budget)
        observations.extend(rows)
        meta, body = rows[-1]
        if meta['status'] != 200 or 'html' not in meta.get('content_type', '').lower():
            return result, observations
        result['visible_names'] = [x[0] for x in items if r.district_home_matches(body, x[1])]
        links, base = r.page_links(body, url)
        for ordinal, (href, label) in enumerate(links):
            try:
                target = r.resolve_page_link(url, base, href)
                if not target or urlsplit(target).hostname != hostname:
                    continue
                kind = r.candidate_kind(target, url, label)
                if kind:
                    result['documents'].append({'url': target, 'anchor_index': ordinal, 'kind': kind[0],
                                                'label': ' '.join(label.split())[:160]})
            except ValueError:
                continue
            if len(result['documents']) == 100:
                break
        result.update(state='site_checked', source_url=url, sha256=r.sha(body), captured_at=meta['retrieved_at'])
    except r.RobotsDenied as error:
        observations.extend(error.observations)
        result['reason'] = str(error)[:80]
    except (OSError, ValueError, RuntimeError, UnicodeError) as error:
        result['reason'] = type(error).__name__
    return result, observations


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data-dir', type=Path, required=True)
    parser.add_argument('--offset', type=int, default=0)
    parser.add_argument('--limit', type=int, default=80)
    parser.add_argument('--workers', type=int, default=4)
    args = parser.parse_args()
    if args.offset < 0 or not 1 <= args.limit <= 300 or not 1 <= args.workers <= 8:
        parser.error('invalid bounded scan parameters')
    db = r.root_store(args.data_dir)
    db.close()
    output = args.data_dir / f'shared-host-leads-{args.offset}.json'
    fd = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        geography = json.loads(r.GEOGRAPHY.read_bytes())['jurisdictions']
        entries = leads(d.pinned_source(), geography)[args.offset:args.offset + args.limit]
        budget = r.DownloadBudget(100 * 1024 * 1024)
        results = []
        with ThreadPoolExecutor(max_workers=args.workers) as pool:
            futures = [pool.submit(inspect, entry, budget) for entry in entries]
            for index, future in enumerate(as_completed(futures), 1):
                item, observations = future.result()
                r.archive(args.data_dir, observations)
                results.append(item)
                if index % 20 == 0 or index == len(futures):
                    print(f'Checked {index}/{len(futures)} shared district sites', flush=True)
        results.sort(key=lambda x: x['host'])
        with os.fdopen(fd, 'w') as handle:
            json.dump({'notice': 'Private leads only; shared sites do not establish district ownership or decisions.',
                       'results': results}, handle, indent=2)
            handle.write('\n'); handle.flush(); os.fsync(handle.fileno())
    except BaseException:
        try:
            os.close(fd)
        except OSError:
            pass
        output.unlink(missing_ok=True)
        raise
    print('Checked:', Counter(x['state'] for x in results)['site_checked'],
          'exact names visible:', sum(len(x['visible_names']) for x in results),
          'direct document links:', sum(len(x['documents']) for x in results))


if __name__ == '__main__':
    main()
