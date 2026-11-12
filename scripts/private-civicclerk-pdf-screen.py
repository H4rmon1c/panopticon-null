#!/usr/bin/env python3
"""Privately screen one previously captured, source-bound CivicClerk PDF."""
import argparse
from datetime import date
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import resource
import shutil
import signal
import stat
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('civic', ROOT / 'scripts/private-civicclerk-discovery.py')
c = importlib.util.module_from_spec(spec); spec.loader.exec_module(c)
MAX_TOTAL = 64 * 1024 * 1024
MAX_FILES = 3000
MAX_JSON = 2 * 1024 * 1024
NOTICE = ('Source bytes and receipts remain in the separately retained private input archives. Private preliminary native-text screen only. Images, scanned pages, hidden text, '
          'document completeness, responsible body, approval, votes and jurisdiction coverage are unverified; '
          'absence of a keyword is not evidence of absence.')


def open_dir(path):
    if not path.is_absolute(): raise ValueError('absolute directory required')
    fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in path.parts[1:]:
            if part in ('.', '..'): raise ValueError('invalid path')
            nxt = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd); fd = nxt
        st = os.fstat(fd)
        if st.st_uid != os.geteuid() or st.st_mode & 0o077: raise ValueError('owner-only directory required')
        return fd
    except BaseException:
        os.close(fd); raise


def read_at(fd, name, maximum):
    if not isinstance(name, str) or '/' in name or name in ('', '.', '..'): raise ValueError('invalid entry')
    f = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
    try:
        st = os.fstat(f)
        if not stat.S_ISREG(st.st_mode) or st.st_size > maximum: raise ValueError('invalid or oversized input')
        chunks = []; total = 0
        while True:
            chunk = os.read(f, min(65536, maximum + 1 - total))
            if not chunk: break
            chunks.append(chunk); total += len(chunk)
            if total > maximum: raise ValueError('oversized input')
        return b''.join(chunks)
    finally: os.close(f)


def snapshot_dir(fd, target, budget):
    target.mkdir(mode=0o700)
    with os.scandir(fd) as entries:
        for ent in entries:
            budget[0] += 1
            if budget[0] > MAX_FILES: raise ValueError('archive entry limit exceeded')
            if not re.fullmatch(r'[0-9a-f]{64}(?:\.json)?', ent.name): raise ValueError('unexpected archive entry')
            body = read_at(fd, ent.name, c.MAX_PDF if not ent.name.endswith('.json') else 65536)
            budget[1] += len(body)
            if budget[1] > MAX_TOTAL: raise ValueError('archive byte limit exceeded')
            if hashlib.sha256(body).hexdigest() != ent.name.removesuffix('.json'): raise ValueError('archive content digest mismatch')
            (target / ent.name).write_bytes(body)


def snapshot(rootfd, target, report=False, seed_name="private-probe-results-v3.json"):
    target.mkdir(mode=0o700); budget = [0, 0]
    for child in ('content', 'observations'):
        fd = os.open(child, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=rootfd)
        try: snapshot_dir(fd, target / child, budget)
        finally: os.close(fd)
    if report:
        fd = os.open('report', os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=rootfd)
        try: (target / 'results.json').write_bytes(read_at(fd, 'results.json', MAX_JSON))
        finally: os.close(fd)
    else: (target / 'private-probe-results-v3.json').write_bytes(read_at(rootfd, seed_name, MAX_JSON))


def proof(root, lookup, url, digest, receipt_digest, mime):
    if not re.fullmatch(r'[0-9a-f]{64}', digest or '') or not re.fullmatch(r'[0-9a-f]{64}', receipt_digest or ''): raise ValueError('invalid proof digest')
    rows = lookup.get((url, digest), [])
    matches = [x for x in rows if x[0] == receipt_digest and isinstance(x[1].get('content_type'), str) and mime in x[1]['content_type'].lower()]
    if len(matches) != 1: raise ValueError('required exact receipt missing')
    body = c.r.source_blob(root, digest, c.MAX_PDF)
    return matches[0][1], body


def validate(source, capture, ref):
    seeds = json.loads((source / 'private-probe-results-v3.json').read_bytes(), object_pairs_hook=c.unique_object)
    if not isinstance(seeds, dict): raise ValueError('invalid seeds')
    entries = seeds.get('results');
    if not isinstance(entries, list) or len(entries) > 100: raise ValueError('invalid seeds')
    selected = [e for e in entries if isinstance(e, dict) and e.get('jurisdiction_ref') == ref and e.get('state') == 'html_screened']
    bound, errors = c.bind_group(source, ref, selected, c.h.receipts(source))
    if not bound or errors: raise ValueError('unverified or ambiguous first-party binding')
    report = json.loads((capture / 'results.json').read_bytes(), object_pairs_hook=c.unique_object)
    if not isinstance(report, dict): raise ValueError('invalid report')
    if report.get('schema_version') not in ('pnull-private-civicclerk-v1', 'pnull-private-civicclerk-v2'): raise ValueError('unknown capture schema')
    rows = report.get('results')
    if not isinstance(rows, list) or len(rows) > 3: raise ValueError('invalid report rows')
    rows = [x for x in rows if isinstance(x, dict) and x.get('jurisdiction_ref') == ref]
    if len(rows) != 1: raise ValueError('ambiguous capture')
    row = rows[0]; item = row.get('file_probe')
    if row.get('state') != 'list_captured' or row.get('tenant') != bound['tenant'] or row.get('portal_url') != bound['portal_url'] or not isinstance(item, dict) or item.get('state') != 'pdf_captured_unscreened': raise ValueError('capture identity differs')
    cutoff = date.fromisoformat(report['cutoff']); host, list_url = c.list_url(bound['tenant'], cutoff)
    # This first implementation accepts the original zero-cursor captures only.
    if row.get('list_url') != list_url: raise ValueError('only first-page source captures supported')
    lookup = c.h.receipts(capture)
    _, raw = proof(capture, lookup, list_url, row['sha256'], row['receipt_sha256'], 'json')
    parsed = c.summarize(raw, cutoff)
    matches = [(event, file) for event in parsed['events'] for file in event['published_files'] if event['event_id'] == item.get('event_id') and file['file_id'] == item.get('file_id') and file['file_endpoint_eligible'] is True]
    if len(matches) != 1: raise ValueError('event and eligible file do not bind')
    event, file = matches[0]
    if event['json_path'] != item.get('event_json_path') or file['json_path'] != item.get('file_json_path') or file['type'] != item.get('file_type'): raise ValueError('event or file locator differs')
    docurl = 'https://' + host + '/v1/Meetings/GetMeetingFileStream(fileId=' + str(file['file_id']) + ',plainText=false)'
    if item.get('url') != docurl: raise ValueError('file URL differs')
    shell = next(x[3] for x in bound['capture_proofs'] if x[0] == bound['portal_url']); assets = c.Assets(); assets.feed(shell.decode('utf8','strict'))
    utility = 'https://' + bound['tenant'] + '.portal.civicclerk.com' + c.UTILITY_PATH
    if assets.paths.count(c.UTILITY_PATH) != 1 or item.get('utility_url') != utility or item.get('utility_sha256') != c.UTILITY_SHA: raise ValueError('utility binding differs')
    proof(capture, lookup, utility, c.UTILITY_SHA, item['utility_receipt_sha256'], 'javascript')
    pdf_receipt, pdf = proof(capture, lookup, docurl, item['sha256'], item['receipt_sha256'], 'pdf')
    if not pdf.startswith(b'%PDF-'): raise ValueError('not a PDF')
    proofs = []
    for url, digest, (receipt_digest, receipt), body in bound['capture_proofs']:
        if receipt_digest is None:
            encoded = (json.dumps(receipt,sort_keys=True,separators=(',',':'))+'\n').encode()
            receipt_digest = hashlib.sha256(encoded).hexdigest()
            if (source/'observations'/(receipt_digest+'.json')).read_bytes() != encoded: raise ValueError('redirect receipt unavailable')
        if hashlib.sha256(body).hexdigest()!=digest or not re.fullmatch(r'[0-9a-f]{64}',receipt_digest): raise ValueError('source proof differs')
        proofs.append({'url':url,'sha256':digest,'receipt_sha256':receipt_digest})
    anchor = bound['source_anchor']
    source_binding = {'index_url':anchor['index_url'],'index_sha256':anchor['index_sha256'],'anchor_index':anchor['anchor_index'],'portal_url':bound['portal_url'],'proofs':proofs}
    utility_binding = {'url':utility,'sha256':c.UTILITY_SHA,'receipt_sha256':item['utility_receipt_sha256']}
    return {'source_binding':source_binding,'utility_binding':utility_binding,'jurisdiction_ref':ref, 'tenant':bound['tenant'], 'portal_url':bound['portal_url'], 'list_url':list_url, 'list_sha256':row['sha256'], 'list_receipt_sha256':row['receipt_sha256'], 'event_id':event['event_id'], 'event_name':event['event_name'], 'event_date':event['date'], 'event_json_path':event['json_path'], 'file_id':file['file_id'], 'file_type':file['type'], 'file_json_path':file['json_path'], 'pdf_url':docurl, 'pdf_sha256':item['sha256'], 'pdf_receipt_sha256':item['receipt_sha256'], 'retrieved_at':pdf_receipt['retrieved_at']}, pdf


def resource_limits():
    resource.setrlimit(resource.RLIMIT_NOFILE, (64, 64))
    resource.setrlimit(resource.RLIMIT_FSIZE, (12*1024*1024, 12*1024*1024))


def screen(binary, scratch, provenance, runtime=120):
    if type(runtime) is not int or not 1 <= runtime <= 120: raise ValueError('invalid scope runtime')
    content_fd = os.open(scratch/'content', os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
    cmd = ['/usr/bin/systemd-run','--user','--scope','--quiet',
           '--property=MemoryMax=512M','--property=MemorySwapMax=0','--property=TasksMax=16',
           '--property=CPUQuota=100%',f'--property=RuntimeMaxSec={runtime}','--',
           '/usr/bin/bwrap','--unshare-user','--unshare-pid','--unshare-net','--unshare-ipc','--unshare-uts',
           '--die-with-parent','--new-session','--size','1048576','--tmpfs','/', '--ro-bind','/usr','/usr',
           '--symlink','usr/lib','/lib','--symlink','usr/lib','/lib64','--symlink','usr/bin','/bin',
           '--proc','/proc','--dev','/dev','--size','134217728','--tmpfs','/tmp',
           '--size','134217728','--tmpfs','/archive','--dir','/archive/content',
           '--ro-bind',f'/proc/self/fd/{content_fd}','/archive/content',
           '--ro-bind','/sys/fs/cgroup','/sys/fs/cgroup','--ro-bind',str(binary),'/pnull',
           '--ro-bind',str(ROOT/'rules'),'/rules','--ro-bind',str(ROOT/'configs'),'/configs',
           '--ro-bind',str(ROOT/'scripts/private-civicclerk-pdf-sandbox.py'),'/helper.py']
    if Path('/nix/store').is_dir(): cmd.extend(['--ro-bind','/nix/store','/nix/store'])
    cmd.extend(['--clearenv','--setenv','PATH','/usr/bin:/bin','--setenv','LANG','C.UTF-8','--chdir','/',
                '/usr/bin/python3','/helper.py','--digest',provenance['pdf_sha256'],
                '--source-url',provenance['pdf_url'],'--jurisdiction-ref',provenance['jurisdiction_ref'],
                '--retrieved-at',provenance['retrieved_at']])
    try:
        with tempfile.TemporaryFile(dir=scratch) as out, tempfile.TemporaryFile(dir=scratch) as err:
            p = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=out, stderr=err,
                                 start_new_session=True, pass_fds=(content_fd,), preexec_fn=resource_limits,
                                 env={**os.environ,'PATH':'/usr/bin:/bin','LANG':'C.UTF-8'})
            try: code = p.wait(timeout=runtime+10)
            except subprocess.TimeoutExpired:
                os.killpg(p.pid, signal.SIGKILL); p.wait(); return {'state':'incomplete', 'reason':'extractor_timeout'}, None
            if code or out.tell() > 12*1024*1024 or err.tell() > 12*1024*1024: return {'state':'incomplete', 'reason':'extractor_failed_or_output_limit'}, None
            out.seek(0); envelope = json.load(out)
    finally: os.close(content_fd)
    result = envelope['result']; text = envelope['text']
    if not isinstance(text,str) or hashlib.sha256(text.encode()).hexdigest()!=result.get('extracted_sha256') or result.get('state') not in ('matched','no_match') or result.get('document_sha256') != provenance['pdf_sha256']: raise ValueError('extractor output identity differs')
    result['state'] = 'preliminary_keyword_lead' if result['state'] == 'matched' else 'unverified_no_keyword_in_extracted_text'
    result.pop('extraction_status', None)  # The legacy extractor's 'complete' does not cover image text.
    result['native_text_status'] = 'extracted'
    result['document_coverage'] = 'unverified'
    return result, text


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--seed-name', default='private-probe-results-v3.json'); p.add_argument('--source-archive', type=Path, required=True); p.add_argument('--capture-archive', type=Path, required=True); p.add_argument('--output-parent', type=Path, required=True); p.add_argument('--output-name', required=True); p.add_argument('--jurisdiction', required=True); p.add_argument('--binary', type=Path, required=True); a = p.parse_args()
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,63}\.json', a.seed_name) or not re.fullmatch(r'us-co-place-[0-9]{7}', a.jurisdiction) or not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,63}', a.output_name): p.error('invalid jurisdiction or output name')
    fds = [open_dir(x) for x in (a.source_archive, a.capture_archive, a.output_parent)]
    try:
        with tempfile.TemporaryDirectory(prefix='.civic-screen-', dir=f'/proc/self/fd/{fds[2]}') as td:
            temp = Path(td); snapshot(fds[0], temp/'source', seed_name=a.seed_name); snapshot(fds[1], temp/'capture', True)
            provenance, pdf = validate(temp/'source', temp/'capture', a.jurisdiction)
            work = temp/'work'; work.mkdir(mode=0o700); (work/'content').mkdir(mode=0o700); (work/'content'/provenance['pdf_sha256']).write_bytes(pdf)
            result, text = screen(a.binary, work, provenance)
            if text is not None: result['extracted_text'] = text
            payload = {'schema_version':'pnull-private-civic-pdf-screen-v1', 'notice':NOTICE, 'source':provenance, 'screen':result}
            c.h.atomic_output(Path(f'/proc/self/fd/{fds[2]}')/a.output_name, c.h.bounded_json(payload))
            print('Private screen:', result['state'])
    finally:
        for fd in fds: os.close(fd)

if __name__ == '__main__': main()
