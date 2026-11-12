#!/usr/bin/env python3
"""Materialize the independently verified Georgetown minutes, or withhold them."""
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
SCHEMA = 'pnull-georgetown-motion-v1'
REF = 'us-co-place-0829735'
RECORD = 'co-document-07ceaa6a4b1625e277378647'
URL = 'https://cms7files.revize.com/georgetownco/BOS 26.09.22 Minutes.pdf'
INDEX = 'https://www.townofgeorgetown.us/government/boards_and_commissions/board_of_selectmen/agendas_and_minutes.php'
INDEX_SHA = 'ca4a4476e3346653ab91f6522a222aaf6a4781864bbbd0c2a8b6dbcd0eec6094'
PDF = '450cf7773ab1ac3142995d2d2fcbe772f65b6a5037f04211b65eaf97c63419dd'
TEXT = '2f0b3b39e0e2d4e97bdece77f7b608cee42a924ae51bc95c457d43bd38cdb046'
LINKED = 'https://www.townofgeorgetown.us/BOS 26.09.22 Minutes.pdf?t=202611041800110'
ORIGIN = LINKED.split('?')[0]
INDEXED_PDF = 'https://cms7files.revize.com/georgetownco/BOS%2026.09.22%20Minutes.pdf?t=202611041800110'
ROSTER = 'ELECTED OFFICIALS PRESENT: Police Judge Georgia Kofoed Selectman Mike Huggins Selectman Amanda Cooper Selectman Alex Sassoon Selectman Mary Smith Selectman Rich Barrows Selectman Dwayne Goscinski'
CONTEXT = 'Consideration of the Flock Service Renewal for the provision of license plate reading cameras and software.'
MOTION = 'Selectman Smith made a motion to table the renewal of the Flock contract and to direct Staff to gather additional information on Flock’s privacy and security policies to bring to the October 13, 2026 meeting. This motion was seconded by Selectman Barrows. Selectman Goscinski, Selectman Smith, and Selectman Barrows voted aye. Selectman Cooper, Selectman Sassoon, and Selectman Huggins voted no. As the vote was tied, Police Judge Kofoed voted aye; therefore, the motion was carried, and the Flock contract renewal was tabled until the next meeting.'
VOTES = [{'name':'Dwayne Goscinski','role':'Selectman','position':'Aye'}, {'name':'Mary Smith','role':'Selectman','position':'Aye'}, {'name':'Rich Barrows','role':'Selectman','position':'Aye'}, {'name':'Amanda Cooper','role':'Selectman','position':'No'}, {'name':'Alex Sassoon','role':'Selectman','position':'No'}, {'name':'Mike Huggins','role':'Selectman','position':'No'}, {'name':'Georgia Kofoed','role':'Police Judge (tie-break)','position':'Aye'}]

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
            or candidate.get('index_sha256') != INDEX_SHA or candidate.get('anchor_index') != 251
            or candidate.get('linked_url') != LINKED or candidate.get('origin_url') != ORIGIN
            or candidate.get('capture_adjustment') != 'timestamp_omitted'
            or result.get('document_sha256') != PDF or result.get('extracted_sha256') != TEXT):
        raise ValueError('source identity changed')
    if 'townofgeorgetown.us' not in r.discovery_hosts(data_dir, db, REF): raise ValueError('jurisdiction source unavailable')
    r.checked_document_candidate(data_dir, URL, candidate)
    keys = r.observation_keys(data_dir)
    if (URL, PDF) not in keys or (INDEXED_PDF, PDF) not in keys or (INDEX, INDEX_SHA) not in keys or not r.receipt_valid(receipt, URL, PDF): raise ValueError('source receipt unavailable')
    if not r.source_blob(data_dir, PDF, 20 * 1024 * 1024).startswith(b'%PDF-'): raise ValueError('invalid PDF')
    text = r.read_blob(data_dir / 'extracted' / TEXT, 5 * 1024 * 1024)
    if r.sha(text) != TEXT: raise ValueError('extracted digest differs')
    pages = text.decode('utf-8').splitlines()
    if (len(pages) != 9 or 'Regular Meeting Minutes September 22, 2026' not in pages[1] or pages[1].count(ROSTER) != 1
            or pages[3].count(CONTEXT) != 1 or pages[4].count(MOTION) != 1):
        raise ValueError('meeting, motion or recorded vote differs')
    from urllib.parse import quote
    if quote(candidate['indexed_redirect_receipt']['redirect_target'], safe=':/?=') != INDEXED_PDF:
        raise ValueError('timestamped destination differs')
    return {'schema_version': SCHEMA, 'state': 'available', 'research_record': RECORD,
            'jurisdiction_ref': REF, 'jurisdiction': 'Georgetown', 'body': 'Board of Selectmen', 'meeting_date': '2026-09-22',
            'motion': 'Table the Flock contract renewal and direct staff to gather privacy and security information',
            'outcome': 'Motion carried after a 3–3 tie and the Police Judge’s aye',
            'motion_details': 'Selectman Mary Smith moved; Selectman Rich Barrows seconded.',
            'votes': VOTES,
            'notice': 'This vote tabled renewal for further information; it did not approve or reject renewal or establish who approved the original contract. Any later decision, contract execution, current camera use, and office terms are not established by this record.',
            'source': {'url': URL, 'sha256': PDF, 'text_sha256': TEXT, 'roster_page': 2, 'context_page': 4, 'motion_page': 5,
                'attendance_locator':'OCR page 2', 'motion_locator':'OCR page 5',
                'index_url': INDEX, 'index_sha256': INDEX_SHA, 'index_anchor': 251, 'indexed_url': LINKED,
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
