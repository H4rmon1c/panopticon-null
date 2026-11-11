#!/usr/bin/env python3
"""Privately screen source-bound document links on shared district indexes.

Even a repeated district heading does not attribute a document or a decision
to that district. This command never modifies the public research database.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import json
import os
from pathlib import Path
from threading import Lock
from urllib.parse import urlsplit
import colorado_shared_districts as shared
import colorado_tenant_followup as followup

r = shared.r


def parents(data_dir, manifest):
    geo = json.loads(r.GEOGRAPHY.read_bytes())['jurisdictions']
    groups = dict(shared.leads(shared.d.pinned_source(), geo))
    receipts = r.observation_keys(data_dir)
    entries = manifest.get('results')
    if not isinstance(entries, list) or len(entries) > 300:
        raise ValueError('invalid shared-host results')
    out, seen = [], set()
    for item in entries:
        if item.get('state') != 'site_checked':
            continue
        host, url, digest = item['host'], item['source_url'], item['sha256']
        parsed = urlsplit(url)
        group = groups.get(host)
        if (not group or host in seen or parsed.scheme != 'https' or not parsed.hostname
                or parsed.hostname not in {x[2] for x in group} or parsed.path != '/'
                or parsed.username or parsed.password or parsed.port not in (None, 443)
                or parsed.query or parsed.fragment or (url, digest) not in receipts):
            raise ValueError('shared homepage does not match directory and receipt')
        seen.add(host)
        body = r.source_blob(data_dir, digest)
        names = [(ref, name) for ref, name, _ in group if r.district_home_matches(body, name)]
        if not names:
            continue
        for ordinal, target in followup.navigation(url, body):
            out.append({'host': host, 'parent_url': url, 'parent_sha256': digest,
                        'parent_anchor_index': ordinal, 'url': target})
    return out


def capture_page(data_dir, parent, budget):
    observations = []
    try:
        host = urlsplit(parent['url']).hostname
        rules, rows = r.get_robots(host, budget=budget)
        observations.extend(rows)
        if not r.allowed(rules, parent['url']):
            return None, observations
        rows = r.get(parent['url'], {host}, maximum=r.MAX_PAGE_BYTES, redirects=0,
                     stop_at_redirect=True, budget=budget)
        observations.extend(rows)
        meta, body = rows[-1]
        if meta['status'] != 200 or 'html' not in meta.get('content_type', '').lower():
            return None, observations
        return {**parent, 'sha256': r.sha(body), 'captured_at': meta['retrieved_at']}, observations
    except r.RobotsDenied as error:
        observations.extend(error.observations)
        return None, observations
    except (OSError, UnicodeError, ValueError, RuntimeError):
        return None, observations


def jobs_for_pages(data_dir, pages, known_parents):
    """Reopen and validate both indexes and every direct document anchor."""
    geo = json.loads(r.GEOGRAPHY.read_bytes())['jurisdictions']
    groups = dict(shared.leads(shared.d.pinned_source(), geo))
    receipts = r.observation_keys(data_dir)
    # An index must have been generated from a validated directory homepage.
    valid = {(x['host'], x['parent_url'], x['parent_sha256'], x['parent_anchor_index'], x['url'])
             for x in known_parents}
    jobs, seen_urls, accepted_pages = [], set(), []
    for page in pages:
        host, parent, parent_digest, anchor, url = (page[k] for k in
            ('host', 'parent_url', 'parent_sha256', 'parent_anchor_index', 'url'))
        if ((host, parent, parent_digest, anchor, url) not in valid or host not in groups
                or (parent, parent_digest) not in receipts or (url, page['sha256']) not in receipts):
            raise ValueError('secondary page lacks an exact parent or source receipt')
        parent_body = r.source_blob(data_dir, parent_digest)
        if (anchor, url) not in followup.navigation(parent, parent_body):
            raise ValueError('secondary page is not bound to parent anchor')
        body = r.source_blob(data_dir, page['sha256'])
        # The parent name and exact H1/H2 on the linked page both need to
        # agree with a full pinned directory name. This is not ownership proof.
        names = [(ref, name) for ref, name, _ in groups[host]
                 if r.district_home_matches(parent_body, name) and followup.tenants.heading_matches(body, name)]
        if not names:
            continue
        accepted_pages.append({**page, 'matching_directory_names': names})
        for candidate in followup.direct_documents(url, body):
            if urlsplit(candidate['url']).hostname != urlsplit(url).hostname:
                raise ValueError('cross-host document')
            r.checked_document_candidate(data_dir, candidate['url'], candidate)
            if candidate['url'] not in seen_urls:
                seen_urls.add(candidate['url'])
                jobs.append({'host': host, 'page': page, 'candidate': candidate})
    return jobs, accepted_pages


def run(data_dir, binary, offset, page_offset, document_offset, max_pages, max_docs, workers):
    manifest = json.loads(r.read_blob(data_dir / f'shared-host-leads-{offset}.json', 8 * 1024 * 1024))
    parent_pages = parents(data_dir, manifest)[page_offset:page_offset + max_pages]
    budget = r.DownloadBudget(200 * 1024 * 1024)
    pages = []
    with ThreadPoolExecutor(max_workers=workers) as pool:
        futures = [pool.submit(capture_page, data_dir, page, budget) for page in parent_pages]
        for index, future in enumerate(as_completed(futures), 1):
            page, observations = future.result()
            r.archive(data_dir, observations)
            if page:
                pages.append(page)
            if index % 20 == 0 or index == len(futures):
                print(f'Captured {index}/{len(futures)} shared index pages', flush=True)
    # Executor completion order is nondeterministic, so sort before applying
    # offsets; otherwise a resumed batch can silently skip different pages.
    pages.sort(key=lambda page: (page['host'], page['url']))
    jobs, accepted = jobs_for_pages(data_dir, pages, parent_pages)
    jobs.sort(key=lambda job: (job['host'], job['candidate']['url']))
    documents_available = len(jobs)
    jobs = jobs[document_offset:document_offset + max_docs]
    locks = {urlsplit(x['candidate']['url']).hostname: Lock() for x in jobs}
    robots, results = {}, []
    with ThreadPoolExecutor(max_workers=workers) as pool:
        futures = {pool.submit(r.screen_one, data_dir, 'us-co-shared-site', x['candidate'], str(binary),
                               locks[urlsplit(x['candidate']['url']).hostname], budget, False, robots): x for x in jobs}
        for index, future in enumerate(as_completed(futures), 1):
            job = futures[future]
            result, observations = future.result()
            r.archive(data_dir, observations)
            results.append({**job, 'result': result})
            if index % 10 == 0 or index == len(futures):
                print(f'Screened {index}/{len(futures)} shared-index documents', flush=True)
    return {'notice': 'Private shared-site screens; district ownership, votes and decisions are not established.',
            'parent_pages_considered': len(parent_pages), 'captured_pages': len(pages),
            'matching_pages': accepted, 'documents_available': documents_available,
            'document_candidates': len(jobs), 'results': results}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data-dir', type=Path, required=True)
    parser.add_argument('--pnull-bin', type=Path, required=True)
    parser.add_argument('--offset', type=int, default=0)
    parser.add_argument('--page-offset', type=int, default=0)
    parser.add_argument('--document-offset', type=int, default=0)
    parser.add_argument('--max-pages', type=int, default=100)
    parser.add_argument('--max-docs', type=int, default=200)
    parser.add_argument('--workers', type=int, default=4)
    args = parser.parse_args()
    if (not args.data_dir.is_absolute() or not args.pnull_bin.is_absolute() or args.pnull_bin.is_symlink()
            or not args.pnull_bin.is_file() or args.offset < 0 or args.page_offset < 0 or args.document_offset < 0
            or not 1 <= args.max_pages <= 300
            or not 1 <= args.max_docs <= 300 or not 1 <= args.workers <= 4):
        parser.error('invalid bounded parameters')
    db = r.root_store(args.data_dir)
    db.close()
    output = args.data_dir / f'shared-followup-{args.offset}-{args.page_offset}-{args.document_offset}.json'
    fd = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        result = run(args.data_dir, args.pnull_bin, args.offset, args.page_offset, args.document_offset,
                     args.max_pages, args.max_docs, args.workers)
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
    print('Matching index pages:', len(result['matching_pages']), 'documents:', len(result['results']))


if __name__ == '__main__':
    main()
