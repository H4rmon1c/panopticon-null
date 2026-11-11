#!/usr/bin/env python3
"""Follow labeled tenant document indexes and screen their direct document links.

Private research only: an exact name on both pages and a link do not prove a
document's legal ownership, any vote, or a surveillance purchase.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import json
import os
from pathlib import Path
import re
from threading import Lock
from urllib.parse import urlsplit
import colorado_district_tenants as tenants

r = tenants.research
NAV = re.compile(r'\b(?:documents?|meetings?|agendas?|minutes?|archives?)\b', re.I)


def navigation(parent_url, body):
    links, base = r.page_links(body, parent_url)
    host = urlsplit(parent_url).hostname
    selected = []
    for ordinal, (href, label) in enumerate(links):
        if not NAV.search(' '.join(label.split())):
            continue
        try:
            target = r.resolve_page_link(parent_url, base, href)
            parsed = urlsplit(target or '')
            if (parsed.scheme != 'https' or parsed.hostname != host or parsed.username or parsed.password
                    or parsed.port not in (None, 443) or parsed.query or parsed.fragment
                    or re.search(r'\.(?:pdf|docx?)$', parsed.path, re.I)):
                continue
        except ValueError:
            continue
        if target != parent_url and target not in [x[1] for x in selected]:
            selected.append((ordinal, target))
        if len(selected) == 3:
            break
    return selected


def direct_documents(index_url, body):
    links, base = r.page_links(body, index_url)
    host = urlsplit(index_url).hostname
    found = []
    for ordinal, (href, label) in enumerate(links):
        try:
            target = r.resolve_page_link(index_url, base, href)
            parsed = urlsplit(target or '')
            if parsed.scheme != 'https' or parsed.hostname != host:
                continue
            kind = r.candidate_kind(target, index_url, label)
            if kind:
                found.append({'url': target, 'index_url': index_url, 'index_sha256': r.sha(body),
                              'anchor_index': ordinal, 'kind': kind[0], 'label': ' '.join(label.split())[:160]})
        except ValueError:
            continue
        if len(found) == 100:
            break
    return found


def collect(data_dir, binary):
    manifest = json.loads(r.read_blob(data_dir / 'tenant-leads.json', 8 * 1024 * 1024))
    geo = json.loads(r.GEOGRAPHY.read_bytes())['jurisdictions']
    leads = set(tenants.leads(tenants.districts.pinned_source(), geo))
    receipts = r.observation_keys(data_dir)
    if not isinstance(manifest.get('results'), list) or len(manifest['results']) > 500:
        raise ValueError('invalid bounded tenant results')
    budget = r.DownloadBudget(200 * 1024 * 1024)
    jobs, pages = [], []
    for item in manifest['results']:
        if item.get('state') != 'identity_heading_matched':
            continue
        ref, name, parent, digest = (item[k] for k in ('jurisdiction_ref', 'name', 'url', 'source_sha256'))
        if (ref, name, parent) not in leads or (parent, digest) not in receipts:
            raise ValueError('parent page is not bound to directory and receipt')
        body = r.source_blob(data_dir, digest)
        if not tenants.heading_matches(body, name):
            raise ValueError('parent identity differs')
        for anchor, url in navigation(parent, body):
            host = urlsplit(url).hostname
            try:
                rules, observations = r.get_robots(host, budget=budget)
                r.archive(data_dir, observations)
                if not r.allowed(rules, url):
                    continue
                observations = r.get(url, {host}, maximum=r.MAX_PAGE_BYTES, redirects=0,
                                     stop_at_redirect=True, budget=budget)
                r.archive(data_dir, observations)
                meta, html = observations[-1]
                if meta['status'] != 200 or 'html' not in meta.get('content_type', '').lower():
                    continue
                if not tenants.heading_matches(html, name):
                    continue
                pages.append({'jurisdiction_ref': ref, 'name': name, 'parent_url': parent,
                              'parent_sha256': digest, 'parent_anchor_index': anchor,
                              'url': url, 'sha256': r.sha(html)})
                for candidate in direct_documents(url, html):
                    r.checked_document_candidate(data_dir, candidate['url'], candidate)
                    jobs.append((ref, candidate))
            except (OSError, ValueError, RuntimeError, UnicodeError):
                continue
    locks, robots = {}, {}
    for _, candidate in jobs:
        locks.setdefault(urlsplit(candidate['url']).hostname, Lock())
    results = []
    with ThreadPoolExecutor(max_workers=4) as pool:
        futures = {pool.submit(r.screen_one, data_dir, ref, candidate, str(binary),
                               locks[urlsplit(candidate['url']).hostname], budget, False, robots):
                   (ref, candidate) for ref, candidate in jobs[:200]}
        for i, future in enumerate(as_completed(futures), 1):
            ref, candidate = futures[future]
            result, observations = future.result()
            r.archive(data_dir, observations)
            results.append({'jurisdiction_ref': ref, 'candidate': candidate, 'result': result})
            if i % 10 == 0 or i == len(futures):
                print(f'Screened {i}/{len(futures)} secondary tenant document leads', flush=True)
    return {'notice': 'Private leads only; legal document ownership and decisions are not established.',
            'pages': pages, 'results': results}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data-dir', required=True, type=Path)
    parser.add_argument('--pnull-bin', required=True, type=Path)
    args = parser.parse_args()
    if (not args.data_dir.is_absolute() or not args.pnull_bin.is_absolute()
            or not args.pnull_bin.is_file() or args.pnull_bin.is_symlink()):
        parser.error('absolute data directory and regular pnull binary required')
    db = r.root_store(args.data_dir)
    db.close()
    output = args.data_dir / 'tenant-secondary-screening.json'
    # Refuse reruns before fetching to avoid collecting data we cannot record.
    fd = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        result = collect(args.data_dir, args.pnull_bin)
        with os.fdopen(fd, 'w') as handle:
            json.dump(result, handle, indent=2)
            handle.write('\n'); handle.flush(); os.fsync(handle.fileno())
    except BaseException:
        try:
            os.close(fd)
        except OSError:
            pass
        output.unlink(missing_ok=True)
        raise
    print(f"Matched {len(result['pages'])} secondary district pages; screened {len(result['results'])} documents")


if __name__ == '__main__':
    main()
