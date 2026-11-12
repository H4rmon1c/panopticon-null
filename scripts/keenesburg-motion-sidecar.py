#!/usr/bin/env python3
"""Materialize the independently verified Keenesburg minutes, or withhold them."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import sqlite3

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('garden_motion', ROOT / 'scripts/garden-city-motion-sidecar.py')
g = importlib.util.module_from_spec(spec); spec.loader.exec_module(g)
r = g.r
SCHEMA = 'pnull-keenesburg-motion-v1'
REF = 'us-co-place-0840185'
RECORD = 'co-document-e71c09481c6f2e4f709aeec8'
URL = 'https://webgen1files1.revize.com/townofkeenesburgco/Documents/  Government/Agendas and Minutes/Signed Minutes/2022/04042022.pdf'
INDEX = 'https://www.townofkeenesburg.com/government/agendas_minutes/signed_minutes.php'
INDEX_SHA = '084f2c76a0d1cd0c7600de8bd984e802b4fdb01c4aec3abebf19f19f3716de98'
PDF = '9a2ff611f5ad837f0462a0959415544ac0d67148e198d54df993f4fa87d39d71'
TEXT = 'c5680b2637d5c795f15fb6bed4e623b80fc59c920978b3b6f34daf8a67b70983'
LINKED = 'https://www.townofkeenesburg.com/Documents/  Government/Agendas and Minutes/Signed Minutes/2022/04042022.pdf?t=202402190125430'
ORIGIN = LINKED.split('?')[0]
INDEXED_PDF = URL + '?' + LINKED.split('?')[1]
NAMES = [('Kenneth Gfeller', 'Mayor'), ('Wilbur Wafel', 'Trustee'), ('Bruce Sparrow', 'Trustee'), ('Juanda Hesse', 'Trustee'), ('Carl Fredrickson', 'Trustee'), ('Tim Smith', 'Trustee'), ('Anthony Erger', 'Trustee')]
PAGE1 = 'Members Present: Mayor Kenneth Gfeller, Trustees Wilbur Wafel, Bruce Sparrow, Juanda Hesse, Carl Fredrickson, Tim Smith, and Anthony Erger'
MOTION19 = 'A motion was made by Trustee Sparrow, with a second by Trustee Wafel, to approve Resolution No. 2022-19 A Resolution approving a Service Agreement with Flock Group Inc for license plate detection software and hardware. Motion carried 7-0 with a roll call vote: Mayor Gfeller, Trustees Wafel, Sparrow, Hesse, Fredrickson, Smith, and Erger voted yes.'
MOTION20 = "A motion was made by Trustee Erger, with a second by Trustee Sparrow, to approve Resolution No. 2022-20 A Resolution approving a Memorandum of Understanding with Flock Group INC. for the Keenesburg Police Department's access to Flock's technology platform and security dashboard. Motion carried 7- 0 with a roll call vote: Mayor Gfeller, Trustees Wafel, Sparrow, Hesse, Fredrickson, Smith, and Erger voting yes."

def digest_receipt(data_dir, receipt):
    raw = (json.dumps(receipt, sort_keys=True, separators=(',', ':')) + '\n').encode()
    digest = r.sha(raw)
    if r.read_blob(data_dir / 'observations' / (digest + '.json'), 65536) != raw:
        raise ValueError('archived receipt differs')
    return digest

def require_research_binding(snapshot):
    rows = [x for x in snapshot['records'] if x.get('id') == RECORD]
    if (len(rows) != 1 or rows[0].get('kind') != 'meeting_document' or rows[0].get('historical') is not False
            or rows[0].get('jurisdiction_ref') != REF or rows[0].get('source_url') != URL or rows[0].get('source_sha256') != PDF
            or rows[0].get('indexed_source_url') != LINKED or rows[0].get('source_index_url') != INDEX
            or rows[0].get('capture_adjustment') != 'timestamp_omitted'):
        raise ValueError('research record missing or changed')

def materialize(data_dir, db):
    row = db.execute('SELECT state,candidate_json,result_json FROM document_candidates WHERE jurisdiction_ref=? AND url=?', (REF, URL)).fetchone()
    if not row or row[0] != 'matched': raise ValueError('source unavailable')
    candidate = json.loads(row[1]); result = json.loads(row[2]); receipt = result['receipt']
    if (candidate != result.get('candidate') or candidate.get('url') != URL or candidate.get('index_url') != INDEX
            or candidate.get('index_sha256') != INDEX_SHA or candidate.get('anchor_index') != 220
            or candidate.get('linked_url') != LINKED or candidate.get('origin_url') != ORIGIN
            or candidate.get('capture_adjustment') != 'timestamp_omitted'
            or result.get('document_sha256') != PDF or result.get('extracted_sha256') != TEXT):
        raise ValueError('source identity changed')
    if 'townofkeenesburg.com' not in r.discovery_hosts(data_dir, db, REF): raise ValueError('jurisdiction source unavailable')
    r.checked_document_candidate(data_dir, URL, candidate)
    keys = r.observation_keys(data_dir)
    if (URL, PDF) not in keys or (INDEXED_PDF, PDF) not in keys or (INDEX, INDEX_SHA) not in keys or not r.receipt_valid(receipt, URL, PDF): raise ValueError('source receipt unavailable')
    if not r.source_blob(data_dir, PDF, 20 * 1024 * 1024).startswith(b'%PDF-'): raise ValueError('invalid PDF')
    text = r.read_blob(data_dir / 'extracted' / TEXT, 5 * 1024 * 1024)
    if r.sha(text) != TEXT: raise ValueError('extracted digest differs')
    pages = text.decode('utf-8').splitlines()
    if (len(pages) != 5 or 'Monday, April 4, 2022' not in pages[0] or PAGE1 not in pages[0]
            or pages[1].count(MOTION19) != 1 or pages[1].count(MOTION20) != 1
            or not pages[1].index(MOTION19) < pages[1].index('b. RESOLUTION NO. 2022-20') < pages[1].index(MOTION20)):
        raise ValueError('motion or attendance text differs')
    return {'schema_version': SCHEMA, 'state': 'available', 'research_record': RECORD,
            'jurisdiction_ref': REF, 'jurisdiction': 'Keenesburg', 'body': 'Board of Trustees', 'meeting_date': '2022-04-04',
            'motion': 'Resolution 2022-19: approve a Flock Group services agreement for license plate detection hardware and software',
            'outcome': 'Motion carried 7–0',
            'votes': [{'name': name, 'role': role, 'position': 'Yes'} for name, role in NAMES],
            'separate_motion': 'Resolution 2022-20 concerned a separate Flock platform access memorandum and had a separate 7–0 vote.',
            'notice': 'The minutes record approval. Agreement execution, actual camera installation, current use, and individual term dates are not established by this record. The timestamped vendor URL was also captured and yielded the same PDF digest.',
            'source': {'url': URL, 'sha256': PDF, 'text_sha256': TEXT, 'attendance_page': 1, 'motion_page': 2,
                'index_url': INDEX, 'index_sha256': INDEX_SHA, 'index_anchor': 220, 'indexed_url': LINKED,
                'origin_url': ORIGIN, 'indexed_document_url': INDEXED_PDF, 'capture_adjustment': 'timestamp_omitted', 'captured_at': receipt['retrieved_at'],
                'receipt_sha256': digest_receipt(data_dir, receipt), 'index_receipt_sha256': g.receipt_digest(data_dir, INDEX, INDEX_SHA),
                'redirect_receipt_sha256': digest_receipt(data_dir, candidate['redirect_receipt']),
                'indexed_redirect_receipt_sha256': digest_receipt(data_dir, candidate['indexed_redirect_receipt']),
                'indexed_document_receipt_sha256': g.receipt_digest(data_dir, INDEXED_PDF, PDF)}}

def main():
    p = argparse.ArgumentParser(description=__doc__); p.add_argument('--data-dir', type=Path, required=True); p.add_argument('--output', type=Path, required=True); p.add_argument('--research-snapshot', type=Path)
    a = p.parse_args()
    if not a.data_dir.is_absolute() or not a.output.is_absolute(): p.error('absolute paths required')
    try:
        with sqlite3.connect((a.data_dir / 'research.sqlite3').as_uri() + '?mode=ro', uri=True) as db:
            db.execute('BEGIN'); value = materialize(a.data_dir, db); db.rollback()
        if a.research_snapshot is not None:
            if not a.research_snapshot.is_absolute(): raise ValueError('absolute snapshot required')
            require_research_binding(json.loads(r.read_blob(a.research_snapshot, 8 * 1024 * 1024)))
    except (OSError, ValueError, TypeError, KeyError, sqlite3.Error): value = {'schema_version': SCHEMA, 'state': 'not_available'}
    a.output.parent.mkdir(parents=True, exist_ok=True)
    temp = a.output.with_name('.' + a.output.name + f'.{os.getpid()}.tmp')
    fd = os.open(temp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
    try:
        with os.fdopen(fd, 'w') as out: json.dump(value, out, separators=(',', ':')); out.write('\n'); out.flush(); os.fsync(out.fileno())
        os.replace(temp, a.output)
    finally:
        if temp.exists(): temp.unlink()

if __name__ == '__main__': main()
