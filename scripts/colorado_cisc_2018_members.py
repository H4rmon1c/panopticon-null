#!/usr/bin/env python3
"""Capture a dated CISC membership list as private, source-bound OCR candidates.

The 2018 list establishes neither present membership nor technology use, votes,
contracts or purchasing decisions. This command does not publish records.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import unicodedata

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('colorado_research', ROOT / 'scripts/colorado-research.py')
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)

COMMITTEE = 'https://leg.colorado.gov/committee_meeting_hearing_summary/17501'
ATTACHMENT = 'https://leg.colorado.gov/summary_attachments/20890/download'
SOURCE_SHA256 = '006e916f7719b0f298d9688a4462eac14d03847b20f6f223d0158f460d5515e3'
ATTACHMENT_LABEL = '19HouseJud0129AttachD.pdf'
NOTICE = ('Private, manually pre-transcribed and OCR-matched candidates from a 2018 member-agency list. '
          'The source does not establish current membership, use of any specific technology, '
          'individual votes, a contract or purchasing decision.')

# Pinned page-one transcription, in printed row order. OCR must independently
# contain each row before it is emitted. The original PDF is authoritative.
NAMES = (
    "Adams County Sheriff's Office", "Arapahoe County Sheriff's Office", 'Arvada Police Department',
    'Aspen Police Department', 'Aurora Police Department', 'Avon Police Department',
    "Boulder County Sheriff's Office", 'Boulder Police Department', 'Brighton Police Department',
    'Broomfield Police Department', 'Carbondale Police Department', 'Castle Rock Police Department',
    'Cherry Hills Village Police Department', "Colorado Attorney General's Office", 'Colorado Department of Public Safety',
    'Colorado Springs Police Department', 'Commerce City Police Department', 'Denver District Attorney (2nd JD)',
    'Denver Police Department', "Douglas County Sheriff's Office", 'Durango Police Department',
    "Eagle County Sheriff's Office", 'Eagle Police Department', 'Edgewater Police Department',
    "Elbert County Sheriff's Office", 'Englewood Police Department', 'Erie Police Department',
    'Estes Park Police Department', 'Federal Heights Police Department', 'Fort Collins Police Services',
    'Fountain Police Department', 'Fruita Police Department', "Garfield County Sheriff's Office",
    'Glendale Police Department', 'Golden Police Department', 'Grand Junction Police Department',
    'Greenwood Village Police Department', 'JeffCom911', "Jefferson County District Attorney's Office (1st JD)",
    "Jefferson County Sheriff's Office", 'Lakewood Police Department', "Larimer County Sheriff's Office",
    'Littleton Police Department', 'Lone Tree Police Department', 'Longmont Police Department',
    'Louisville Police Department', 'Loveland Police Department', "Mesa County Sheriff's Office",
    'Morrison Police Department', 'Northglenn Police Department', 'Parker Police Department',
    'Pueblo Police Department', 'Rocky Mountain HIDTA', 'Sheridan Police Department', 'Silt Police Department',
    'Thornton Police Department', 'Timnath Police Department', 'United States Attorney (Ex-Officio)',
    'University of Colorado Denver Police Department (Anschutz)', 'Vail Police Department',
    "Weld County Sheriff's Office", 'Westminster Police Department', 'Wheat Ridge Police Department',
    'Windsor Police Department', "17th Judicial District Attorney's Office (Adams, Broomfield)",
    "18th Judicial District Attorney's Office (Arapahoe, Elbert, Lincoln)",
)


def normalize(value):
    value = unicodedata.normalize('NFKC', value).casefold().replace('\u2019', "'")
    return ' '.join(re.findall(r'[a-z0-9]+', value))


def row_needle(number, name):
    # OCR did not reliably preserve the first nine row numbers; their full
    # names must still appear exactly once. Two JD suffixes are visibly clear
    # but OCR corrupted their superscripts, so match the office label itself.
    if number in (18, 39):
        name = name.split(' (', 1)[0]
    if number in (65, 66):
        name = re.sub(r'^(17|18)th\b', r'\1', name)
    value = normalize(name)
    return value if number < 10 else f'{number} {value}'


def parse_ocr(text, source_digest, extracted_digest, captured_at):
    if source_digest != SOURCE_SHA256 or not re.fullmatch(r'[0-9a-f]{64}', extracted_digest):
        raise ValueError('unexpected CISC source or OCR digest')
    if not isinstance(text, str) or len(text) > 2_000_000:
        raise ValueError('invalid bounded OCR text')
    normalized = normalize(text)
    if '2018 cisc member agencies' not in normalized or 'additional memberships pending' not in normalized:
        raise ValueError('OCR does not establish the expected dated list and qualifier')
    rows, omitted = [], []
    for number, name in enumerate(NAMES, 1):
        needle = row_needle(number, name)
        count = len(re.findall(r'(?<![a-z0-9])' + re.escape(needle) + r'(?![a-z0-9])', normalized))
        if count != 1:
            omitted.append({'row': number, 'reason': 'OCR row missing or ambiguous'})
            continue
        rows.append({'agency_name': name, 'state': 'ocr_candidate',
                     'transcription_method': 'manual_pretranscription_ocr_matched', 'source_year': 2018,
                     'locator': f"PDF page 1, {'left' if number <= 34 else 'right'} column, row {number}",
                     'source_url': ATTACHMENT, 'source_sha256': source_digest,
                     'ocr_sha256': extracted_digest, 'captured_at': captured_at})
    return {'schema_version': 1, 'state': 'private_ocr_candidates', 'notice': NOTICE,
            'source_revision': '2018-11', 'rows': rows, 'omitted_rows': omitted}


def committee_anchor(body):
    links, base = r.page_links(body, COMMITTEE)
    matches = [(index, r.resolve_page_link(COMMITTEE, base, href)) for index, (href, label) in enumerate(links)
               if ' '.join(label.split()) == ATTACHMENT_LABEL]
    if len(matches) != 1 or matches[0][1] != ATTACHMENT:
        raise ValueError('committee page does not uniquely link the expected attachment')
    return matches[0][0]


def capture(data_dir, binary, override):
    host = 'leg.colorado.gov'
    budget = r.DownloadBudget(10 * 1024 * 1024)
    rules, robots = r.get_robots(host, budget=budget)
    r.archive(data_dir, robots)
    if not r.allowed(rules, COMMITTEE):
        raise ValueError('committee page disallowed')
    page, observations = r.html_page(COMMITTEE, host, rules, budget)
    r.archive(data_dir, observations)
    if not r.receipt_valid(observations[-1][0], COMMITTEE, r.sha(page)):
        raise ValueError('committee page capture receipt is invalid')
    anchor = committee_anchor(page)
    disallowed = not r.allowed(rules, ATTACHMENT)
    if disallowed and not override:
        raise ValueError('attachment disallowed; explicit owner override required')
    observations = r.get(ATTACHMENT, {host}, maximum=2_000_000, redirects=0,
                         stop_at_redirect=True, budget=budget)
    r.archive(data_dir, observations)
    receipt, pdf = observations[-1]
    if (receipt['status'] != 200 or not pdf.startswith(b'%PDF-') or r.sha(pdf) != SOURCE_SHA256
            or not r.receipt_valid(receipt, ATTACHMENT, SOURCE_SHA256)):
        raise ValueError('attachment response is not the pinned captured PDF')
    result = r.extract_captured_pdf(data_dir, 'us-co-state-08', {'url': ATTACHMENT}, str(binary), receipt, True)
    if (result.get('extraction_status') != 'complete_with_ocr' or result.get('ocr_page_set_validated') is not True
            or result.get('ocr_page_text_mapping_version') != 2):
        raise ValueError('OCR page set not validated')
    extracted_sha = result['extracted_sha256']
    raw = r.read_blob(data_dir / 'extracted' / extracted_sha, 2_000_000)
    if r.sha(raw) != extracted_sha:
        raise ValueError('OCR text digest mismatch')
    payload = parse_ocr(raw.decode('utf8'), SOURCE_SHA256, extracted_sha, receipt['retrieved_at'])
    payload['committee_source'] = {'url': COMMITTEE, 'sha256': r.sha(page), 'anchor_index': anchor}
    payload['attachment_source'] = {'url': ATTACHMENT, 'sha256': SOURCE_SHA256,
                                    'captured_at': receipt['retrieved_at'], 'robots_override_used': disallowed}
    return payload


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data-dir', type=Path, required=True)
    parser.add_argument('--pnull-bin', type=Path, required=True)
    parser.add_argument('--allow-attachment-robots-override', action='store_true',
                        help='explicitly disregard robots policy for this one attachment')
    args = parser.parse_args()
    if (not args.data_dir.is_absolute() or not args.pnull_bin.is_absolute()
            or not args.pnull_bin.is_file() or args.pnull_bin.is_symlink()):
        parser.error('absolute private data directory and regular pnull binary required')
    db = r.root_store(args.data_dir)
    db.close()
    output = args.data_dir / 'cisc-2018-private-ocr-candidates.json'
    fd = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        result = capture(args.data_dir, args.pnull_bin, args.allow_attachment_robots_override)
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
    print(f"Private OCR candidate rows: {len(result['rows'])}; omitted: {len(result['omitted_rows'])}")


if __name__ == '__main__':
    main()
