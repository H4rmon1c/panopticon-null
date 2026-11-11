#!/usr/bin/env python3
"""Screen one preserved large PDF privately; source mentions are not decisions."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import resource
import sqlite3
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('research', ROOT / 'scripts/colorado-research.py')
r = importlib.util.module_from_spec(spec); spec.loader.exec_module(r)
MAX_PAGES = 300
SEGMENT_PAGES = 25
MAX_OUTPUT = 8 * 1024 * 1024


def limits():
    resource.setrlimit(resource.RLIMIT_CPU, (20, 20))
    resource.setrlimit(resource.RLIMIT_AS, (1536 * 1024 * 1024,) * 2)
    resource.setrlimit(resource.RLIMIT_FSIZE, (MAX_OUTPUT,) * 2)


def sandbox(pdf, tool, arguments):
    if tool not in ('pdfinfo', 'pdfimages', 'pdftotext'):
        raise ValueError('unknown tool')
    command = ['bwrap', '--unshare-all', '--die-with-parent', '--new-session',
               '--tmpfs', '/', '--ro-bind', '/usr', '/usr', '--symlink', 'usr/lib', '/lib',
               '--symlink', 'usr/lib', '/lib64', '--symlink', 'usr/bin', '/bin',
               '--proc', '/proc', '--dev', '/dev', '--ro-bind', str(pdf), '/input.pdf',
               '/usr/bin/' + tool, *arguments]
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        try:
            run = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=output,
                                 stderr=errors, timeout=30, preexec_fn=limits,
                                 env={'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8'})
        except (subprocess.TimeoutExpired, OSError) as error:
            raise ValueError('sandbox unavailable or timed out') from error
        size = output.tell()
        if run.returncode or size > MAX_OUTPUT or errors.tell():
            raise ValueError('sandbox extraction failed or exceeded output limit')
        output.seek(0)
        return output.read().decode('utf8', 'strict')


def screen(pdf, run=sandbox):
    info = run(pdf, 'pdfinfo', ['/input.pdf'])
    match = re.search(r'^Pages:\s*(\d+)\s*$', info, re.M)
    if not match or not 101 <= int(match.group(1)) <= MAX_PAGES:
        raise ValueError('page count missing or outside 101–300')
    count = int(match.group(1)); findings = []
    for first in range(1, count + 1, SEGMENT_PAGES):
        last = min(count, first + SEGMENT_PAGES - 1)
        images = run(pdf, 'pdfimages', ['-f', str(first), '-l', str(last), '-list', '/input.pdf'])
        lines = images.splitlines()
        if len(lines) < 2 or not lines[0].lstrip().startswith('page') or not re.match(r'^\s*-+', lines[1]):
            raise ValueError('image inventory incomplete')
        # Fail closed on any image, including logos and image masks: even a
        # mostly textual page may conceal relevant image-only information.
        if any(line.strip() for line in lines[2:]):
            raise ValueError('image-bearing or scanned page requires OCR')
        text = run(pdf, 'pdftotext', ['-layout', '-f', str(first), '-l', str(last), '/input.pdf', '-'])
        pages = text.split('\f')
        if pages[-1] != '' or len(pages) != last - first + 2:
            raise ValueError('incomplete page mapping')
        for offset, page in enumerate(pages[:-1]):
            if not page.strip():
                raise ValueError('empty or scanned page')
            for number, line in enumerate(page.splitlines(), 1):
                terms = r.matching_terms(line)
                if terms:
                    normalized = ' '.join(line.split())
                    positions = [normalized.casefold().find(term.casefold()) for term in terms]
                    positions = [position for position in positions if position >= 0]
                    if not positions:
                        raise ValueError('keyword does not bind to extracted line')
                    start = max(0, min(positions) - 120)
                    excerpt = normalized[start:start + 400]
                    excerpt_terms = [term for term in terms if term.casefold() in excerpt.casefold()]
                    findings.append({'page': first + offset, 'line': number,
                                     'text': excerpt, 'terms': excerpt_terms})
                    if len(findings) > 1000:
                        raise ValueError('too many keyword matches')
    return {'state': 'private_keyword_lead' if findings else 'private_extracted_text_no_keyword',
            'pages': count, 'segment_pages': SEGMENT_PAGES, 'findings': findings}


def require_direct_candidate(candidate):
    if not isinstance(candidate, dict) or any(key in candidate for key in ('springs_source', 'redirect_receipt', 'indexed_redirect_receipt', 'linked_url', 'origin_url', 'capture_adjustment')):
        raise ValueError('redirect and multi-index candidates are unsupported')


def find_receipts(store, pairs):
    found = {}
    for path in (store / 'observations').iterdir():
        try:
            raw = r.read_blob(path, 65536); item = json.loads(raw)
            key = (item.get('url'), item.get('sha256'))
            if key in pairs and r.receipt_valid(item, *key):
                found[key] = (item, raw)
        except (OSError, ValueError, TypeError, json.JSONDecodeError):
            continue
    if set(found) != set(pairs):
        raise ValueError('required PDF or index source receipt missing')
    return found


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source-dir', type=Path, required=True)
    p.add_argument('--private-dir', type=Path, required=True)
    p.add_argument('--jurisdiction', required=True)
    p.add_argument('--url', required=True)
    a = p.parse_args()
    if not a.source_dir.is_absolute() or not a.private_dir.is_absolute() or a.private_dir.exists() or a.private_dir.is_symlink():
        p.error('absolute existing source and fresh private directory required')
    public = (ROOT / 'web/panopticon-terminal').resolve()
    if a.private_dir.resolve() == public or public in a.private_dir.resolve().parents:
        p.error('private directory cannot be under public web root')
    dbfile = a.source_dir / 'research.sqlite3'
    if dbfile.is_symlink() or not dbfile.is_file():
        p.error('source store database missing or symlinked')
    with sqlite3.connect(dbfile.as_uri() + '?mode=ro', uri=True) as db:
        row = db.execute('SELECT candidate_json,last_error FROM document_candidates WHERE jurisdiction_ref=? AND url=? AND state=\'error\'', (a.jurisdiction, a.url)).fetchone()
    if not row or 'pdf_page_limit' not in (row[1] or ''):
        p.error('source is not an existing page-limit candidate')
    candidate = json.loads(row[0])
    require_direct_candidate(candidate)
    r.checked_document_candidate(a.source_dir, a.url, candidate)
    # Locate a preserved successful PDF receipt for this exact URL. A second
    # matching receipt is ambiguous and requires an explicit digest selection.
    choices = {}
    for path in (a.source_dir / 'observations').iterdir():
        try:
            item = json.loads(r.read_blob(path, 65536)); digest = item.get('sha256')
            if item.get('url') == a.url and r.receipt_valid(item, a.url, digest): choices[digest] = item
        except (OSError, ValueError, TypeError, json.JSONDecodeError): pass
    if len(choices) != 1:
        p.error('expected exactly one preserved PDF digest for this URL')
    digest = next(iter(choices)); pairs = [(a.url, digest), (candidate['index_url'], candidate['index_sha256'])]
    receipts = find_receipts(a.source_dir, pairs)
    blobs = {d: r.source_blob(a.source_dir, d, 20 * 1024 * 1024) for _, d in pairs}
    if not blobs[digest].startswith(b'%PDF-'): p.error('source bytes are not a PDF')
    a.private_dir.mkdir(mode=0o700, parents=False, exist_ok=False)
    (a.private_dir / 'content').mkdir(mode=0o700); (a.private_dir / 'observations').mkdir(mode=0o700)
    for d, body in blobs.items():
        fd = os.open(a.private_dir / 'content' / d, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, 'wb') as f: f.write(body)
    for _, raw in receipts.values():
        key = hashlib.sha256(raw).hexdigest()
        fd = os.open(a.private_dir / 'observations' / (key + '.json'), os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, 'wb') as f: f.write(raw)
    # Prove the private archive independently reconstructs the index binding.
    r.checked_document_candidate(a.private_dir, a.url, candidate)
    try:
        result = screen(a.private_dir / 'content' / digest)
    except (ValueError, UnicodeError) as error:
        result = {'state': 'incomplete', 'reason': str(error)}
    result.update({'jurisdiction_ref': a.jurisdiction, 'url': a.url, 'document_sha256': digest,
                   'candidate': candidate, 'notice': 'Private source mentions only; no agency decision, contract, vote, or approval established.'})
    fd = os.open(a.private_dir / 'screen.json', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'w') as f: json.dump(result, f, indent=2); f.write('\n')
    print(result['state'])
    return 2 if result['state'] == 'incomplete' else 0

if __name__ == '__main__': raise SystemExit(main())
