#!/usr/bin/env python3
"""Build a narrowly source-bound preliminary motion card, or withhold it."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import sqlite3

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('research', ROOT / 'scripts/colorado-research.py')
r = importlib.util.module_from_spec(spec); spec.loader.exec_module(r)

SCHEMA = 'pnull-garden-city-motion-v1'
REF = 'us-co-place-0829185'
RECORD = 'co-document-65cfe2b3b5d87f9ab45c4235'
URL = 'https://www.townofgardencity.com/files/415f86adc/Board+Packet+8.18.26+Amended.pdf'
PDF = 'a6470690665a246b48a279bccde538eb3c1b6a108793bee55dae249df981c5c7'
INDEX = 'https://www.townofgardencity.com/board-meetings'
INDEX_SHA = 'b3d4db4c22ef8241b8a6a4cea0567ea8293f26b309bfd07411ebc7fa9d31fc76'
TEXT_SHA = 'becbea24e7b1d83b25644c4c3a55381fe52cd1d9a8837f898aeeecea5f0ac9c5'
ADOPTION_URL = 'https://www.townofgardencity.com/files/a5a4e0d2c/Minutes+8.18.26.pdf'
ADOPTION_PDF = '1fadbfb3ebfc9409c6cb6c6088203048342fe3d90c9db0f91291b34077abf07e'
ADOPTION_INDEX = 'ba10f90f69057113bfa08ea546d7daa559c159837cc9f6efbf6013bb3e6d1485'
ADOPTION_TEXT = 'c39514d6a75f1d9d3a89e9c973e563c7066bfd281e35f349a4d54a628e95d3f5'
LINES = {
    1: 'Board of Trustees Regular Meeting Agenda Amended 8.17.2026',
    2: 'August 18, 2026 at 5:30 PM',
    11: 'Approve Minutes from August 03, 2026, Regular Meeting',
    70: 'Board of Trustees Regular Meeting Minutes',
    71: 'August 3, 2026 at 5:30 PM',
    72: 'Town Hall - 621 27th Street Road, Garden City, CO 80631',
    81: 'PRESENT: Mayor Fil Archuleta',
    82: 'Trustee Alex Lopez', 83: 'Trustee Leigh Sorensen',
    84: 'Trustee Katherine Rodriguez', 85: 'Trustee Gary Sorensen',
    86: 'Trustee Jasmine Marquez', 87: 'ABSENT: Trustee Rebecca Moreau',
    202: '9.', 203: 'Police Department Items', 204: 'a.',
    205: 'Consider new 5-year agreement with Axon Enterprise, Inc',
    206: 'Police Chief Jeremy Black presented the Master Services and Purchasing Agreement',
    207: 'with Axon Enterprises, Inc. He noted the agreement is a 5-year agreement.',
    208: 'Trustee Leigh Sorensen moved to approve a 5-year Agreement with Axon Enterprise,',
    209: 'Inc., seconded by Trustee Jasmine Marquez.',
    210: 'VOTING YES: Mayor Fil Archuleta, Trustee Alex Lopez, Trustee Leigh Sorensen,',
    211: 'Trustee Katherine Rodriguez, Trustee Gary Sorensen, Trustee Jasmine Marquez',
    212: 'VOTING NO: None', 213: 'Motion carried.', 214: 'b.',
}
VOTES = [
    {'name': 'Fil Archuleta', 'role': 'Mayor', 'position': 'Yes', 'locator': 'extracted line 210'},
    {'name': 'Alex Lopez', 'role': 'Trustee', 'position': 'Yes', 'locator': 'extracted line 210'},
    {'name': 'Leigh Sorensen', 'role': 'Trustee', 'position': 'Yes', 'locator': 'extracted line 210'},
    {'name': 'Katherine Rodriguez', 'role': 'Trustee', 'position': 'Yes', 'locator': 'extracted line 211'},
    {'name': 'Gary Sorensen', 'role': 'Trustee', 'position': 'Yes', 'locator': 'extracted line 211'},
    {'name': 'Jasmine Marquez', 'role': 'Trustee', 'position': 'Yes', 'locator': 'extracted line 211'},
]


def receipt_digest(data_dir, url, digest):
    found = []
    for path in (data_dir / 'observations').iterdir():
        try:
            raw = r.read_blob(path, 65536); item = json.loads(raw)
            if path.name == r.sha(raw) + '.json' and r.receipt_valid(item, url, digest):
                found.append(path.stem)
        except (OSError, ValueError, TypeError, KeyError, json.JSONDecodeError):
            continue
    if not found: raise ValueError('preserved receipt unavailable')
    return min(found)


def materialize_adoption(data_dir, db):
    row = db.execute('''SELECT state,candidate_json,result_json FROM document_candidates
                        WHERE jurisdiction_ref=? AND url=?''', (REF, ADOPTION_URL)).fetchone()
    if not row or row[0] not in ('matched', 'no_match'): raise ValueError('adoption source unavailable')
    result = json.loads(row[2]); candidate = json.loads(row[1])
    if (result.get('document_sha256') != ADOPTION_PDF or result.get('extracted_sha256') != ADOPTION_TEXT or
            candidate.get('url') != ADOPTION_URL or candidate.get('index_url') != INDEX or
            candidate.get('index_sha256') != ADOPTION_INDEX or candidate.get('anchor_index') != 89):
        raise ValueError('adoption source identity changed')
    receipt = result['receipt']; keys = r.observation_keys(data_dir)
    if ((ADOPTION_URL, ADOPTION_PDF) not in keys or (INDEX, ADOPTION_INDEX) not in keys or
            not r.receipt_valid(receipt, ADOPTION_URL, ADOPTION_PDF)):
        raise ValueError('adoption receipts unavailable')
    raw = (json.dumps(receipt, sort_keys=True, separators=(',', ':')) + '\n').encode()
    receipt_sha = r.sha(raw)
    if r.read_blob(data_dir / 'observations' / (receipt_sha + '.json'), 65536) != raw:
        raise ValueError('adoption receipt differs')
    index_receipt_sha = receipt_digest(data_dir, INDEX, ADOPTION_INDEX)
    r.checked_document_candidate(data_dir, ADOPTION_URL, candidate)
    if not r.source_blob(data_dir, ADOPTION_PDF, 20 * 1024 * 1024).startswith(b'%PDF-'):
        raise ValueError('adoption source is not a PDF')
    text = r.read_blob(data_dir / 'extracted' / ADOPTION_TEXT, 5 * 1024 * 1024)
    if r.sha(text) != ADOPTION_TEXT: raise ValueError('adoption text digest differs')
    lines = text.decode('utf8').splitlines()
    expected = {1: 'Board of Trustees Regular Meeting Minutes', 2: 'August 18, 2026 at 5:30 PM',
        22: 'Consent Agenda', 24: 'Approve Minutes from August 03, 2026, Regular Meeting',
        36: 'Approve Petty Cash Check for Bootleggin Days in the amount of $6,500.00',
        37: 'Trustee Rebecca Moreau moved to approve Consent Agenda a-g, seconded by Trustee',
        38: 'Leigh Sorensen.', 40: 'VOTING YES: Mayor Fil Archuleta, Trustee Alex Lopez, Trustee Leigh Sorensen,',
        41: 'Trustee Rebecca Moreau, Trustee Katherine Rodriguez, Trustee Gary Sorensen,',
        42: 'Trustee Jasmine Marquez', 43: 'VOTING NO: None', 44: 'Motion carried.', 45: '4.'}
    if any(n > len(lines) or lines[n-1] != value for n, value in expected.items()):
        raise ValueError('adoption consent passage differs')
    return {'date': '2026-08-18', 'scope': 'Consent agenda a–g, including approval of the August 3 minutes',
            'outcome': 'Motion carried', 'yes_count': 7,
            'notice': 'This later consent-block vote approved minutes and other items; it is not a vote on the Axon agreement.',
            'source': {'url': ADOPTION_URL, 'sha256': ADOPTION_PDF, 'text_sha256': ADOPTION_TEXT,
                       'meeting_page': 1, 'vote_page': 2, 'locator': 'extracted lines 22–44',
                       'index_url': INDEX, 'index_sha256': ADOPTION_INDEX, 'index_anchor': 89,
                       'receipt_sha256': receipt_sha, 'index_receipt_sha256': index_receipt_sha,
                       'captured_at': receipt['retrieved_at']}}


def materialize(data_dir, db):
    row = db.execute('''SELECT state,candidate_json,result_json,updated_at FROM document_candidates
                        WHERE jurisdiction_ref=? AND url=?''', (REF, URL)).fetchone()
    if not row or row[0] != 'matched': raise ValueError('matching source is unavailable')
    result = json.loads(row[2]); candidate = result.get('candidate', json.loads(row[1]))
    if (result.get('document_sha256') != PDF or result.get('extracted_sha256') != TEXT_SHA or
            candidate.get('url') != URL or candidate.get('index_url') != INDEX or
            candidate.get('index_sha256') != INDEX_SHA or candidate.get('anchor_index') != 90):
        raise ValueError('source identity changed')
    receipt = result['receipt']; keys = r.observation_keys(data_dir)
    if ((URL, PDF) not in keys or (INDEX, INDEX_SHA) not in keys or
            not r.receipt_valid(receipt, URL, PDF) or
            'townofgardencity.com' not in r.discovery_hosts(data_dir, db, REF)):
        raise ValueError('source receipts or jurisdiction binding unavailable')
    receipt_raw = (json.dumps(receipt, sort_keys=True, separators=(',', ':')) + '\n').encode()
    receipt_sha = r.sha(receipt_raw)
    if r.read_blob(data_dir / 'observations' / (receipt_sha + '.json'), 65536) != receipt_raw:
        raise ValueError('captured document receipt differs')
    index_receipt_sha = receipt_digest(data_dir, INDEX, INDEX_SHA)
    r.checked_document_candidate(data_dir, URL, candidate)
    if not r.source_blob(data_dir, PDF, 20 * 1024 * 1024).startswith(b'%PDF-'):
        raise ValueError('source is not a preserved PDF')
    text = r.read_blob(data_dir / 'extracted' / TEXT_SHA, 5 * 1024 * 1024)
    if r.sha(text) != TEXT_SHA: raise ValueError('preserved extraction digest differs')
    lines = text.decode('utf8').splitlines()
    if any(number > len(lines) or lines[number-1] != expected for number, expected in LINES.items()):
        raise ValueError('meeting, vote, or attendance passage differs')
    try: adoption = materialize_adoption(data_dir, db)
    except (OSError, ValueError, TypeError, KeyError, sqlite3.Error, json.JSONDecodeError): adoption = None
    return {
        'schema_version': SCHEMA, 'state': 'available', 'research_record': RECORD,
        'jurisdiction_ref': REF, 'jurisdiction': 'Garden City', 'meeting_date': '2026-08-03',
        'body': 'Board of Trustees', 'motion': 'Motion to approve a five-year agreement with Axon Enterprise, Inc.',
        'outcome': 'Motion carried', 'votes': VOTES,
        'attendance': {'name': 'Rebecca Moreau', 'role': 'Trustee', 'status': 'Absent at roll call', 'locator': 'extracted line 87'},
        'minutes_adoption': adoption,
        'source': {'url': URL, 'sha256': PDF, 'meeting_page': 3, 'motion_page': 6, 'text_sha256': TEXT_SHA,
                   'meeting_locator': 'extracted lines 70–71', 'motion_locator': 'extracted lines 203–213',
                   'index_url': INDEX, 'index_sha256': INDEX_SHA, 'index_anchor': 90, 'captured_at': receipt['retrieved_at'],
                   'receipt_sha256': receipt_sha, 'index_receipt_sha256': index_receipt_sha},
        'notice': 'Minutes included in a later board packet. Contract execution and a specific technology are not established by this record.',
    }


def require_research_binding(snapshot):
    records = [x for x in snapshot['records'] if x.get('id') == RECORD]
    if (len(records) != 1 or records[0].get('kind') != 'meeting_document' or
            records[0].get('historical') is not False or records[0].get('jurisdiction_ref') != REF or
            records[0].get('source_url') != URL or records[0].get('source_sha256') != PDF):
        raise ValueError('research source is unavailable or changed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data-dir', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--research-snapshot', type=Path)
    a = parser.parse_args()
    if not a.data_dir.is_absolute() or not a.output.is_absolute(): parser.error('absolute paths are required')
    try:
        with sqlite3.connect((a.data_dir / 'research.sqlite3').as_uri() + '?mode=ro', uri=True) as db:
            db.execute('BEGIN'); result = materialize(a.data_dir, db); db.rollback()
        if a.research_snapshot is not None:
            if not a.research_snapshot.is_absolute(): raise ValueError('snapshot path must be absolute')
            snapshot = json.loads(r.read_blob(a.research_snapshot, 8 * 1024 * 1024))
            require_research_binding(snapshot)
    except (OSError, ValueError, TypeError, KeyError, sqlite3.Error, json.JSONDecodeError):
        result = {'schema_version': SCHEMA, 'state': 'not_available'}
    a.output.parent.mkdir(parents=True, exist_ok=True)
    temporary = a.output.with_name('.' + a.output.name + f'.{os.getpid()}.tmp')
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
    try:
        with os.fdopen(fd, 'w') as out: json.dump(result, out, separators=(',', ':')); out.write('\n'); out.flush(); os.fsync(out.fileno())
        os.replace(temporary, a.output)
    finally:
        if temporary.exists(): temporary.unlink()


if __name__ == '__main__': main()
