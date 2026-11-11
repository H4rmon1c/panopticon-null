#!/usr/bin/env python3
"""Offline source-bound discovery and preliminary-export regressions."""
import importlib.util
import io
import json
from contextlib import redirect_stdout
from datetime import datetime, timezone
from pathlib import Path
import sqlite3
import sys
import tempfile
import threading
from types import SimpleNamespace
import unittest
from urllib.parse import parse_qs, urlsplit
import zipfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('colorado_research', ROOT / 'scripts/colorado-research.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
docx_spec = importlib.util.spec_from_file_location('extract_docx', ROOT / 'scripts/extract-docx.py')
docx_module = importlib.util.module_from_spec(docx_spec)
docx_spec.loader.exec_module(docx_module)
school_spec = importlib.util.spec_from_file_location('school_directory', ROOT / 'scripts/build-colorado-school-websites.py')
school_module = importlib.util.module_from_spec(school_spec)
school_spec.loader.exec_module(school_module)
from public_http import checked_url, get, public_ip  # noqa: E402


def receipt(url, body):
    return {'url': url, 'retrieved_at': '2026-11-11T00:00:00+00:00', 'status': 200,
            'resolved_ips': ['8.8.8.8'], 'connected_ip': '8.8.8.8',
            'content_type': 'application/json', 'sha256': module.sha(body), 'bytes': len(body)}


def word_fixture(xml):
    output = io.BytesIO()
    with zipfile.ZipFile(output, 'w', zipfile.ZIP_DEFLATED) as archive:
        archive.writestr('[Content_Types].xml', '<Types/>')
        archive.writestr('word/document.xml', xml)
    return output.getvalue()


class ResearchTests(unittest.TestCase):
    def test_explicit_immediate_redirect_retry_is_scoped_and_keeps_other_rows(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); db = module.root_store(root)
            try:
                city, other = 'us-co-place-0816000', 'us-co-place-0816495'
                for ref, suffix, error, state in [
                    (city, 'redirect', 'not a PDF (HTTP 301)', 'error'),
                    (city, 'notfound', 'not a PDF (HTTP 404)', 'error'),
                    (other, 'other', 'not a PDF (HTTP 301)', 'error'),
                    (city, 'pending', '', 'pending'),
                ]:
                    url = f'https://example.org/{suffix}.pdf'
                    db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,
                                  result_json,updated_at,last_attempt_at) VALUES (?,?,?,?,?,?,?)''',
                               (ref, url, json.dumps({'url': url}), state, json.dumps({'error': error}), module.utcnow(), module.utcnow()))
                db.commit(); selected = []
                other_before = db.execute("select * from document_candidates where url like '%other.pdf'").fetchone()
                def screen(_root, ref, candidate, *_args):
                    selected.append((ref, candidate['url']))
                    return {'state': 'error', 'error': 'still unavailable'}, []
                with patch.object(module, 'screen_one', side_effect=screen):
                    module.screen_documents(root, db, '/bin/true', 10, 10, 1, retry_errors=True,
                                            jurisdiction=city, retry_now=True, retry_redirects=True)
                self.assertEqual(selected, [(city, 'https://example.org/redirect.pdf')])
                self.assertEqual(db.execute("select count(*) from document_candidates where state='pending'").fetchone()[0], 1)
                self.assertEqual(db.execute("select * from document_candidates where url like '%other.pdf'").fetchone(), other_before)
                with self.assertRaises(ValueError):
                    module.screen_documents(root, db, '/bin/true', 1, 1, 1, retry_errors=True, retry_now=True)
                with self.assertRaises(ValueError):
                    module.screen_documents(root, db, '/bin/true', 1, 1, 1, jurisdiction=city, retry_redirects=True)
            finally:
                db.close()
    def test_colorado_springs_document_page_proves_exact_download_anchor(self):
        old = 'https://coloradosprings.gov/sites/default/files/inline-images/agenda_02.14.19.pdf'
        page = 'https://coloradosprings.gov/document/agenda02.14.19.pdf'
        pdf = 'https://coloradosprings.gov/system/files/inline-images/agenda_02.14.19.pdf'
        self.assertTrue(module.springs_document_page(old, page))
        for bad in [page.replace('coloradosprings.gov', 'attacker.example'), page.replace('agenda02', 'agenda03'),
                    page + '?token=1', page.replace('/document/', '/other/')]:
            self.assertFalse(module.springs_document_page(old, bad))
        candidate = {'url': old, 'meeting_date': '2019-02-14'}
        html = f'<a href="{pdf}">Download PDF</a>'.encode()
        result = module.springs_page_candidate(candidate, page, html)
        self.assertEqual(result['url'], pdf)
        self.assertEqual(result['index_sha256'], module.sha(html))
        self.assertIsNone(module.springs_page_candidate(candidate, page, html.replace(b'agenda_02', b'agenda_03')))
        self.assertIsNone(module.springs_page_candidate(candidate, page, html + html))
        self.assertIsNone(module.springs_page_candidate(candidate, page, html.replace(b'Download PDF', b'Other')))

    def test_colorado_springs_document_page_is_archived_before_document_screening(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); db = module.root_store(root)
            try:
                origin = 'https://coloradosprings.gov/sites/default/files/agenda_02.14.19.pdf'
                page = 'https://coloradosprings.gov/document/agenda02.14.19.pdf'
                pdf = 'https://coloradosprings.gov/system/files/agenda_02.14.19.pdf'
                index = 'https://coloradosprings.gov/agenda'
                source = f'<a href="{origin}">Agenda PDF</a>'.encode()
                html = f'<a href="{pdf}">Download PDF</a>'.encode()
                module.archive(root, [(receipt(index, source), source)])
                candidate = {'url': origin, 'index_url': index, 'index_sha256': module.sha(source),
                             'anchor_index': 0, 'meeting_date': None}
                redirect = receipt(origin, b''); redirect.update(status=301, redirect_target=page)
                landing = receipt(page, html); landing['content_type'] = 'text/html'
                def get(url, *_args, **_kwargs):
                    return [(redirect, b'')] if url == origin else [(landing, html)]
                with patch.object(module, 'cached_robots', return_value=(None, [])), patch.object(module, 'get', side_effect=get):
                    result, observations = module.screen_one(root, 'us-co-place-0816000', candidate, '/bin/false',
                                                               __import__('threading').Lock(), module.DownloadBudget(50_000_000), False, {})
                self.assertEqual(result['error'], 'source redirected; alternate document queued')
                module.archive(root, observations)
                alternate = result['alternate_candidate']
                self.assertIn((page, module.sha(html)), module.observation_keys(root))
                module.checked_document_candidate(root, pdf, alternate)
                with patch.object(module, 'cached_robots', return_value=(None, [])), patch.object(module, 'get', side_effect=get):
                    result, _ = module.screen_one(root, 'us-co-place-0816495', candidate, '/bin/false',
                                                  __import__('threading').Lock(), module.DownloadBudget(50_000_000), False, {})
                self.assertNotIn('alternate_candidate', result)
            finally:
                db.close()
    def test_machuga_cross_agency_portrait_requires_all_captures_and_exact_vote(self):
        with tempfile.TemporaryDirectory() as tmp:
            assets = Path(tmp) / 'assets'
            calls = []
            def fake_source(_root, digest, _limit=2_000_000):
                calls.append(digest)
                return b'\xff\xd8\xfffixture'
            class EveryReceipt:
                def __init__(self): self.missing = None
                def __contains__(self, pair): return pair[0] != self.missing
            receipts = EveryReceipt()
            vote = {'name': 'Craig Machuga', 'person_id': 637}
            motion = {'body': 'Urban Renewal Authority'}
            with patch.object(module, 'source_blob', side_effect=fake_source):
                portrait = module.authority_external_portrait(Path(tmp), None, receipts, 'us-co-place-0816495', 'commerce', motion, vote, assets)
                self.assertEqual(portrait['identity_agenda_sha256'], '2936f9b6bee4c0a1b1556579dd8fb807fa0bfc5a7ec357e1fb22e352ec39a67a')
                self.assertEqual(len(calls), 6)
                for changed_ref, client, changed_motion, changed_vote in [
                    ('us-co-county-08001', 'commerce', motion, vote),
                    ('us-co-place-0816495', 'other', motion, vote),
                    ('us-co-place-0816495', 'commerce', {'body': 'City Council'}, vote),
                    ('us-co-place-0816495', 'commerce', motion, {**vote, 'person_id': 638}),
                    ('us-co-place-0816495', 'commerce', motion, {**vote, 'name': 'Other Person'}),
                ]:
                    self.assertIsNone(module.authority_external_portrait(Path(tmp), None, receipts, changed_ref, client, changed_motion, changed_vote, assets))
                receipts.missing = 'https://ompnetwork.s3-us-west-2.amazonaws.com/sites/1717/documents/agenda_22_1.pdf'
                with self.assertRaises(ValueError): module.machuga_portrait(Path(tmp), receipts, assets)

    def test_school_directory_joins_exact_ids_and_rejects_ambiguous_or_shared_hosts(self):
        html = b'''<ul>
          <li><a href="https://one.example/">One District</a> - 1234 - TOWN</li>
          <li><a href="https://same.example/">Four District</a> - 4567 - TOWN</li>
          <li><a href="https://same.example/sub">Four District</a> - 4567 - TOWN</li>
          <li><a href="https://two.example/">Two District</a> - 2345 - TOWN</li>
          <li><a href="https://sites.google.com/district">Three District</a> - 3456 - TOWN</li>
        </ul>'''
        csv_data = ('ST,ST_LEAID,LEAID\nCO,CO-1234,0800001\nCO,CO-4567,0800004\n'
                    'CO,CO-2345,0800002\nCO,CO-2345,0800999\nCO,CO-3456,0800003\n').encode()
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, 'w', zipfile.ZIP_DEFLATED) as archive:
            archive.writestr('directory.csv', csv_data)
        geography = [{'id': f'us-co-school-080000{i}', 'kind': 'school_district',
                      'code': f'080000{i}', 'name': f'District {i}'} for i in range(1, 5)]
        result = school_module.build(html, stream.getvalue(), geography)
        self.assertEqual(result['counts'], {'geography_school_districts': 4, 'with_website_lead': 2, 'without_website_lead': 2})
        self.assertEqual({row['website_url'] for row in result['school_districts']}, {'https://one.example/', 'https://same.example/'})
        self.assertEqual({row['jurisdiction_ref'] for row in result['unmapped']}, {'us-co-school-0800002', 'us-co-school-0800003'})

    def test_school_directory_rejects_altered_identity_or_source(self):
        self.assertEqual(len(module.school_website_leads()), 177)
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'school-directory.json'
            original = json.loads(module.SCHOOL_DIRECTORY.read_text())
            for mutation in ('nces', 'code', 'source', 'shared_host'):
                data = json.loads(json.dumps(original))
                if mutation == 'nces': data['school_districts'][0]['nces_district_id'] = '0809999'
                if mutation == 'code': data['school_districts'][0]['state_district_code'] = '9999'
                if mutation == 'source': data['sources']['nces_directory']['sha256'] = '0' * 64
                if mutation == 'shared_host': data['school_districts'][0]['website_url'] = 'https://sites.google.com/other'
                path.write_text(json.dumps(data))
                with patch.object(module, 'SCHOOL_DIRECTORY', path), self.assertRaises(ValueError):
                    module.school_website_leads()

    def test_discovery_schedules_school_leads_without_manual_case_review(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            directory = root / 'cities.json'
            directory.write_text(json.dumps({'municipalities': []}))
            school = ('us-co-school-0801920', 'Academy 20', 'https://school.example.org/')
            def discovered(ref, name, website, _budget):
                return ({'jurisdiction_ref': ref, 'name': name, 'website_lead': website,
                         'checked_at': module.utcnow(), 'state': 'directory_site_checked', 'portals': [], 'documents': []}, [])
            try:
                with patch.object(module, 'DIRECTORY', directory), patch.object(module, 'bootstrap_counties', return_value={}), \
                     patch.object(module, 'school_website_leads', return_value=[school]), \
                     patch.object(module, 'special_district_website_leads', return_value=[]), \
                     patch.object(module, 'discover_one', side_effect=discovered):
                    module.discover(root, db, limit=2, workers=1, refresh=False)
                self.assertEqual(db.execute('select jurisdiction_ref from discovery').fetchone()[0], school[0])
            finally:
                db.close()

    def test_special_district_directory_excludes_shared_hosts_paths_and_changed_names(self):
        entries = [
            ('01001', 'One Metropolitan District', 'one.example/', '1.0'),
            ('01002', 'Two Metropolitan District', 'shared.example/', '1.0'),
            ('01003', 'Three Metropolitan District', 'shared.example/three', '1.0'),
            ('01004', 'Four Metropolitan District', 'path.example/four', '1.0'),
            ('01005', 'Five Metropolitan District', 'city.example/', '1.0'),
            ('01006', 'Six Metropolitan District', 'six.example/', '2.0'),
            ('01007', 'Seven Metropolitan District', 'seven.example/', '1.0'),
        ]
        geography = [{'id': 'dola-district-' + code, 'code': code, 'kind': 'district_provisional',
                      'name': name if code != '01007' else 'Different District'}
                     for code, name, _, _ in entries]
        source = json.dumps([{'lgid': code, 'name': name, 'url': url, 'lgstatus_i': status}
                             for code, name, url, status in entries]).encode()
        leads = module.special_districts.build(source, geography, {'city.example'})
        self.assertEqual(leads, [('dola-district-01001', 'One Metropolitan District', 'https://one.example/')])
        with self.assertRaises(ValueError):
            module.special_districts.build(json.dumps(json.loads(source) * 2).encode(), geography)
        self.assertIsNone(module.special_districts.website('https://user:password@one.example/'))
        self.assertIsNone(module.special_districts.website('https://127.0.0.1/'))

    def test_special_district_home_requires_the_whole_name_in_displayed_content(self):
        name = 'One Metropolitan District'
        self.assertTrue(module.district_home_matches(b'<h1>One Metropolitan District</h1>', name))
        self.assertTrue(module.district_home_matches(b'<img alt="One Metropolitan District">', name))
        self.assertFalse(module.district_home_matches(b'<img alt/>', name))
        self.assertFalse(module.district_home_matches(b'<script>One Metropolitan District</script>', name))
        self.assertFalse(module.district_home_matches(b'<h1>Other Metropolitan District</h1>', name))
        self.assertFalse(module.district_home_matches(b'<h1>One Metropolitan District Two</h1>', 'One Metropolitan District Two Three'))

    def test_special_district_evidence_requires_captured_home_identity_and_exact_directory_url(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            ref, name, home = 'dola-district-01001', 'One Metropolitan District', 'https://one.example/'
            body = b'<h1>One Metropolitan District</h1>'
            proof = receipt(home, body)
            proof['content_type'] = 'text/html'
            module.archive(root, [(proof, body)])
            try:
                with db:
                    db.execute('insert into discovery values (?,?,?,?)',
                               (ref, home, module.utcnow(), json.dumps({'district_identity_receipt': proof})))
                with patch.object(module, 'special_district_website_leads', return_value=[(ref, name, home)]):
                    self.assertEqual(module.discovery_hosts(root, db, ref), {'one.example'})
                    with db:
                        db.execute('update discovery set source_url=?', ('https://other.example/',))
                    self.assertEqual(module.discovery_hosts(root, db, ref), set())
                    with db:
                        db.execute('update discovery set source_url=?, result_json=?', (home, '{}'))
                    self.assertEqual(module.discovery_hosts(root, db, ref), set())
            finally:
                db.close()

    def test_special_district_does_not_bind_another_agency_portal(self):
        home = 'https://one.example/'
        body = (b'<h1>One Metropolitan District</h1>'
                b'<a href="https://city.legistar.com/Calendar.aspx">City agenda</a>'
                b'<a href="/agenda.pdf">Agenda</a>')
        def fetch(url, _hosts, **_kwargs):
            content = body if url == home else b''
            meta = receipt(url, content)
            meta.update(content_type='text/html', status=200 if url == home else 404)
            return [(meta, content)]
        robots = receipt(home + 'robots.txt', b'')
        with patch.object(module, 'get_robots', return_value=(None, [(robots, b'')])), patch.object(module, 'get', side_effect=fetch):
            result, _ = module.discover_one('dola-district-01001', 'One Metropolitan District', home, module.DownloadBudget(30_000_000))
            wrong, _ = module.discover_one('dola-district-01002', 'Two Metropolitan District', home, module.DownloadBudget(30_000_000))
        self.assertEqual(result['state'], 'directory_site_checked')
        self.assertEqual(result['portals'], [])
        self.assertEqual([item['url'] for item in result['documents']], ['https://one.example/agenda.pdf'])
        self.assertEqual(wrong['state'], 'failed')
        self.assertEqual(wrong['documents'], [])

    def test_discovery_prioritizes_unseen_then_oldest_sites(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            directory = root / 'cities.json'
            directory.write_text(json.dumps({'municipalities': []}))
            leads = [('dola-district-01001', 'Old One', 'https://one.example/'),
                     ('dola-district-01002', 'Old Two', 'https://two.example/'),
                     ('dola-district-01003', 'Unseen Three', 'https://three.example/')]
            checked = []
            def discovered(ref, name, website, _budget):
                checked.append(ref)
                return ({'jurisdiction_ref': ref, 'name': name, 'website_lead': website,
                         'checked_at': module.utcnow(), 'state': 'directory_site_checked', 'portals': [], 'documents': []}, [])
            try:
                with db:
                    for ref, name, url in leads[:2]:
                        db.execute('insert into discovery values (?,?,?,?)', (ref, url, '2020-01-01T00:00:00+00:00', '{}'))
                    db.execute('insert into discovery_attempts values (?,?,?,?)', (leads[0][0], '2021-01-01T00:00:00+00:00', 'failed', None))
                    db.execute('insert into discovery_attempts values (?,?,?,?)', (leads[1][0], '2019-01-01T00:00:00+00:00', 'failed', None))
                with patch.object(module, 'DIRECTORY', directory), patch.object(module, 'bootstrap_counties', return_value={}), \
                     patch.object(module, 'school_website_leads', return_value=[]), \
                     patch.object(module, 'special_district_website_leads', return_value=leads), \
                     patch.object(module, 'discover_one', side_effect=discovered):
                    module.discover(root, db, limit=2, workers=1, refresh=False)
                self.assertEqual(checked, ['dola-district-01003', 'dola-district-01002'])
            finally:
                db.close()

    def test_unexpected_site_failure_does_not_abandon_other_discovery_jobs(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            directory = root / 'cities.json'
            directory.write_text(json.dumps({'municipalities': []}))
            leads = [('dola-district-01001', 'One District', 'https://one.example/'),
                     ('dola-district-01002', 'Two District', 'https://two.example/')]
            def discovered(ref, name, website, _budget):
                if ref == leads[0][0]:
                    raise TypeError('malformed source content')
                return ({'jurisdiction_ref': ref, 'name': name, 'website_lead': website,
                         'checked_at': module.utcnow(), 'state': 'directory_site_checked', 'portals': [], 'documents': []}, [])
            try:
                with patch.object(module, 'DIRECTORY', directory), patch.object(module, 'bootstrap_counties', return_value={}), \
                     patch.object(module, 'school_website_leads', return_value=[]), \
                     patch.object(module, 'special_district_website_leads', return_value=leads), \
                     patch.object(module, 'discover_one', side_effect=discovered):
                    module.discover(root, db, limit=2, workers=2, refresh=False)
                rows = dict(db.execute('select jurisdiction_ref,state from discovery_attempts'))
                self.assertEqual(rows, {leads[0][0]: 'failed', leads[1][0]: 'directory_site_checked'})
            finally:
                db.close()

    def test_document_screening_rotates_toward_untried_and_oldest_jurisdictions(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            refs = ['dola-district-01001', 'dola-district-01002', 'dola-district-01003']
            attempted = []
            try:
                with db:
                    for i, ref in enumerate(refs):
                        url = f'https://site{i}.example/agenda.pdf'
                        db.execute('''insert into document_candidates(jurisdiction_ref,url,candidate_json,state,updated_at)
                                      values (?,?,?,'pending',?)''', (ref, url, json.dumps({'url': url}), module.utcnow()))
                    for ref, timestamp in [(refs[0], '2021-01-01T00:00:00+00:00'), (refs[1], '2019-01-01T00:00:00+00:00')]:
                        url = f'https://{ref}.example/prior.pdf'
                        db.execute('''insert into document_candidates(jurisdiction_ref,url,candidate_json,state,updated_at,last_attempt_at)
                                      values (?,?,?,'no_match',?,?)''', (ref, url, json.dumps({'url': url}), timestamp, timestamp))
                def screen(_root, ref, _candidate, _binary, _lock, _budget, _ocr, _cache):
                    attempted.append(ref)
                    return {'state': 'error', 'error': 'fixture'}, []
                with patch.object(module, 'screen_one', side_effect=screen):
                    module.screen_documents(root, db, '/usr/bin/true', limit=2, per_place=1, workers=1)
                self.assertEqual(attempted, [refs[2], refs[1]])
            finally:
                db.close()

    def test_school_website_links_do_not_automatically_bind_another_agency_portal(self):
        home = 'https://school.example.org/'
        body = b'<a href="https://city.legistar.com/Calendar.aspx">City council agenda</a>'
        def fetch(url, _hosts, **_kwargs):
            content = body if url == home else b''
            meta = receipt(url, content)
            meta.update(content_type='text/html', status=200 if url == home else 404)
            return [(meta, content)]
        robots = receipt(home + 'robots.txt', b'')
        with patch.object(module, 'get_robots', return_value=(None, [(robots, b'')])), patch.object(module, 'get', side_effect=fetch):
            result, _ = module.discover_one('us-co-school-0801920', 'Academy 20', home, module.DownloadBudget(30_000_000))
        self.assertEqual(result['state'], 'directory_site_checked')
        self.assertEqual(result['portals'], [])

    def test_network_scope_rejects_private_mixed_and_credential_targets(self):
        for address in ['127.0.0.1', '10.0.0.1', '169.254.169.254', '224.0.0.1',
                        '::1', '2001:db8::1', '3fff::1', '64:ff9b::1', '::ffff:8.8.8.8']:
            self.assertFalse(public_ip(address), address)
        self.assertTrue(public_ip('8.8.8.8'))
        for url in ['http://example.org/', 'https://x:y@example.org/', 'https://example.org:444/',
                    'https://other.example/', 'https://example.org/#x']:
            with self.assertRaises(ValueError): checked_url(url, {'example.org'})
        answers = [(2, 1, 6, '', ('8.8.8.8', 443)), (2, 1, 6, '', ('127.0.0.1', 443))]
        with patch('public_http.socket.getaddrinfo', return_value=answers), patch('public_http.socket.create_connection') as connect:
            with self.assertRaises(ValueError): get('https://example.org/', {'example.org'}, interval=0)
            connect.assert_not_called()

    def test_http_encodes_literal_spaces_and_unicode_without_changing_receipts(self):
        url = 'https://example.org/Agenda 2024/caf\u00e9%20memo.pdf?file=city board%2Bvote&x=1'
        answers = [(2, 1, 6, '', ('8.8.8.8', 443))]
        with patch('public_http.socket.getaddrinfo', return_value=answers), \
             patch('public_http.socket.create_connection'), \
             patch('public_http.ssl.create_default_context') as context, \
             patch('public_http.http.client.HTTPResponse') as response_class:
            connection = context.return_value.wrap_socket.return_value
            response = response_class.return_value
            response.status = 200
            response.getheader.return_value = None
            response.read.side_effect = [b'%PDF-example', b'']
            result = get(url, {'example.org'}, interval=0)
        request = connection.sendall.call_args.args[0]
        self.assertEqual(request.split(b'\r\n', 1)[0],
                         b'GET /Agenda%202024/caf%C3%A9%20memo.pdf?file=city%20board%2Bvote&x=1 HTTP/1.1')
        self.assertEqual(result[0][0]['url'], url)
        self.assertEqual(result[0][1], b'%PDF-example')

    def test_http_can_return_a_redirect_without_contacting_its_destination(self):
        answers = [(2, 1, 6, '', ('8.8.8.8', 443))]
        with patch('public_http.socket.getaddrinfo', return_value=answers), \
             patch('public_http.socket.create_connection') as connect, \
             patch('public_http.ssl.create_default_context'), \
             patch('public_http.http.client.HTTPResponse') as response_class:
            response = response_class.return_value
            response.status = 302
            response.getheader.side_effect = lambda name, default=None: 'https://outside.example/robots.txt' if name == 'Location' else default
            response.read.side_effect = [b'']
            result = get('https://example.org/robots.txt', {'example.org'}, interval=0,
                         redirects=0, stop_at_redirect=True)
        self.assertEqual(result[0][0]['redirect_target'], 'https://outside.example/robots.txt')
        self.assertEqual(connect.call_count, 1)

    def test_vendor_robots_redirect_is_bounded_and_does_not_authorize_vendor_content(self):
        original = 'https://town.example.org/robots.txt'
        vendor = 'https://cms3.revize.com/revize/town/robots.txt'
        body = b'User-agent: *\nDisallow: /private\n'
        first = receipt(original, b'')
        first.update(status=302, redirect_target=vendor)
        final = receipt(vendor, body)
        final['content_type'] = 'text/plain'
        def fetch(url, hosts, **kwargs):
            self.assertEqual(hosts, {urlsplit(url).hostname})
            self.assertTrue(kwargs['stop_at_redirect'])
            self.assertEqual(kwargs['redirects'], 0)
            return [(first, b'')] if url == original else [(final, body)]
        aliases = {'town.example.org', 'www.town.example.org'}
        with patch.object(module, 'get', side_effect=fetch):
            rules, observations = module.get_robots('town.example.org', aliases)
        self.assertFalse(module.allowed(rules, 'https://town.example.org/private/document'))
        self.assertTrue(module.allowed(rules, 'https://town.example.org/agendas'))
        self.assertEqual(len(observations), 2)
        self.assertEqual(aliases, {'town.example.org', 'www.town.example.org'})

    def test_canonical_gov_redirect_requires_the_full_jurisdiction_in_the_registered_domain(self):
        self.assertTrue(module.canonical_gov_host('Arvada city', 'www.arvadaco.gov'))
        self.assertTrue(module.canonical_gov_host('Arapahoe County', 'files.arapahoeco.gov'))
        self.assertTrue(module.canonical_gov_host('Crested Butte town', 'townofcrestedbutte.colorado.gov'))
        for host in ('arvada.attacker.gov', 'arvadaco.gov.attacker.example', 'other.gov', 'arvadaco.com'):
            self.assertFalse(module.canonical_gov_host('Arvada city', host))
        original = 'https://www.arvada.org/robots.txt'
        target = 'https://www.arvadaco.gov/robots.txt'
        first = receipt(original, b'')
        first.update(status=301, redirect_target=target)
        body = b'User-agent: *\nDisallow: /private\n'
        final = receipt(target, body)
        final['content_type'] = 'text/plain'
        def fetch(url, _hosts, **_kwargs):
            return [(first, b'')] if url == original else [(final, body)]
        hosts = {'www.arvada.org', 'arvada.org'}
        with patch.object(module, 'get', side_effect=fetch):
            rules, rows = module.get_robots('www.arvada.org', hosts,
                                            canonical_redirect=lambda host: module.canonical_gov_host('Arvada', host))
        self.assertIn('www.arvadaco.gov', hosts)
        self.assertEqual(len(rows), 2)
        self.assertFalse(module.allowed(rules, 'https://www.arvada.org/private'))
        first['redirect_target'] = 'https://arvada.attacker.gov/robots.txt'
        with patch.object(module, 'get', return_value=[(first, b'')]):
            with self.assertRaises(module.RobotsDenied):
                module.get_robots('www.arvada.org', {'www.arvada.org'},
                                  canonical_redirect=lambda host: module.canonical_gov_host('Arvada', host))

    def test_canonical_government_sources_require_both_archived_redirect_and_final_page(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref, home, final_url = 'us-co-place-0803455', 'https://www.arvada.org/', 'https://www.arvadaco.gov/'
                redirect = receipt(home, b'')
                redirect.update(status=301, redirect_target=final_url)
                body = b'<a href="/AgendaCenter/ViewFile/Minutes/_09152026-1206">Minutes</a>'
                final = receipt(final_url, body)
                final['content_type'] = 'text/html'
                module.archive(root, [(redirect, b''), (final, body)])
                entry = {'jurisdiction_ref': ref, 'name': 'Arvada', 'website_lead': home,
                         'checked_at': module.utcnow(), 'state': 'directory_site_checked', 'portals': [], 'documents': [],
                         'canonical_url': final_url, 'canonical_receipt': final, 'canonical_redirects': [redirect]}
                module.record_discovery(db, entry)
                self.assertEqual(module.discovery_hosts(root, db, ref), {'arvada.org', 'arvadaco.gov'})
                document_url = final_url + 'AgendaCenter/ViewFile/Minutes/_09152026-1206'
                document = b'PK\x03\x04fixture'
                capture = receipt(document_url, document)
                module.archive(root, [(capture, document)])
                candidate = {'url': document_url, 'index_url': final_url, 'index_sha256': module.sha(body), 'anchor_index': 0}
                response = SimpleNamespace(returncode=0, stdout=b'Flock Safety discussion\n', stderr=b'')
                with patch.object(module.shutil, 'which', side_effect=lambda name, **_: '/usr/bin/' + name), \
                     patch.object(module.subprocess, 'run', return_value=response):
                    screened = module.extract_saved_docx(root, ref, candidate, capture)
                with db:
                    db.execute('''insert into document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at)
                                  values (?,?,?,'matched',?,?)''', (ref, document_url, json.dumps(candidate), json.dumps(screened), module.utcnow()))
                output = root / 'public.json'
                module.export_report(root, db, output)
                self.assertEqual(len(json.loads(output.read_text())['records']), 1)
                bad = {**entry, 'canonical_url': 'https://attacker.gov/'}
                with db: db.execute('update discovery set result_json=? where jurisdiction_ref=?', (json.dumps(bad), ref))
                self.assertEqual(module.discovery_hosts(root, db, ref), {'arvada.org'})
                module.export_report(root, db, output)
                self.assertEqual(json.loads(output.read_text())['records'], [])
                with db: db.execute('update discovery set result_json=? where jurisdiction_ref=?', (json.dumps(entry), ref))
                encoded = (json.dumps(redirect, sort_keys=True, separators=(',', ':')) + '\n').encode()
                (root / 'observations' / (module.sha(encoded) + '.json')).unlink()
                self.assertEqual(module.discovery_hosts(root, db, ref), {'arvada.org'})
            finally:
                db.close()

    def test_discovery_preserves_the_full_government_website_move(self):
        old, new = 'https://www.arvada.org/', 'https://www.arvadaco.gov/'
        robots_old, robots_new = old + 'robots.txt', new + 'robots.txt'
        redirect = receipt(old, b'')
        redirect.update(status=301, redirect_target=new)
        robot_redirect = receipt(robots_old, b'')
        robot_redirect.update(status=301, redirect_target=robots_new)
        robots_body = b'User-agent: *\nAllow: /\n'
        robot_final = receipt(robots_new, robots_body)
        robot_final['content_type'] = 'text/plain'
        body = b'<a href="/AgendaCenter/ViewFile/Minutes/_09152026-1206">Minutes</a>'
        final = receipt(new, body)
        final['content_type'] = 'text/html'
        def fetch(url, hosts, **kwargs):
            if url == robots_old:
                return [(robot_redirect, b'')]
            if url == robots_new:
                return [(robot_final, robots_body)]
            if url == old:
                self.assertIn('www.arvadaco.gov', hosts)
                self.assertTrue(kwargs['redirect_guard'](new))
                return [(redirect, b''), (final, body)]
            missing = receipt(url, b'')
            missing.update(status=404, content_type='text/html')
            return [(missing, b'')]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                with patch.object(module, 'get', side_effect=fetch):
                    result, observations = module.discover_one('us-co-place-0803455', 'Arvada', old,
                                                               module.DownloadBudget(20_000_000))
                self.assertEqual(result['state'], 'directory_site_checked')
                self.assertEqual(result['canonical_url'], new)
                self.assertEqual(len(result['documents']), 1)
                module.archive(root, observations)
                module.record_discovery(db, result)
                self.assertIn('arvadaco.gov', module.discovery_hosts(root, db, result['jurisdiction_ref']))
            finally:
                db.close()

    def test_robots_rejects_untrusted_redirects_and_html_homepages(self):
        original = 'https://town.example.org/robots.txt'
        for destination in ('http://cms3.revize.com/revize/town/robots.txt',
                            'https://attacker.example/robots.txt',
                            'https://cms3.revize.com.attacker.example/robots.txt',
                            'https://cms3.revize.com:444/revize/town/robots.txt',
                            'https://cms3.revize.com/revize/town/robots.txt?other=1',
                            'https://cms3.revize.com/revize/town/index.html',
                            'https://user@cms3.revize.com/revize/town/robots.txt'):
            first = receipt(original, b'')
            first.update(status=302, redirect_target=destination)
            with self.subTest(destination=destination), patch.object(module, 'get', return_value=[(first, b'')]) as fetch:
                with self.assertRaises(module.RobotsDenied):
                    module.get_robots('town.example.org')
                self.assertEqual(fetch.call_count, 1)
        for content_type, body in [('text/html', b'User-agent: *'), ('text/plain', b'<!DOCTYPE html><html>')]:
            meta = receipt(original, body)
            meta['content_type'] = content_type
            with self.subTest(content_type=content_type), patch.object(module, 'get', return_value=[(meta, body)]):
                with self.assertRaises(module.RobotsDenied):
                    module.get_robots('town.example.org')

    def test_robots_wildcards_use_longest_match_and_include_query(self):
        rules = module.RobotsPolicy(b'User-agent: *\nDisallow: /\nAllow: /*.pdf$\nDisallow: /private/*.pdf$\n')
        self.assertTrue(module.allowed(rules, 'https://town.example.org/agendas/meeting.pdf'))
        self.assertFalse(module.allowed(rules, 'https://town.example.org/private/meeting.pdf'))
        self.assertFalse(module.allowed(rules, 'https://town.example.org/agendas/meeting.pdf?t=123'))
        self.assertFalse(module.allowed(rules, 'https://town.example.org/agendas/meeting.html'))
        # Regression for GHSA-wjmf-p669-5m5p: many wildcards must not trigger
        # exponential regex backtracking when a remote site supplies robots.txt.
        hostile = module.RobotsPolicy(('User-agent: *\nDisallow: /' + '*1' * 100 + '*Z\n').encode())
        self.assertTrue(module.allowed(hostile, 'https://town.example.org/' + '1' * 2000))

    def test_low_storage_stops_new_collection_before_it_fills_the_disk(self):
        with patch.object(module.os, 'statvfs', return_value=SimpleNamespace(f_bavail=100, f_frsize=4096, f_favail=100000)):
            with self.assertRaises(ValueError): module.require_storage_headroom(Path('/tmp'))
        with patch.object(module.os, 'statvfs', return_value=SimpleNamespace(f_bavail=1000000, f_frsize=4096, f_favail=10)):
            with self.assertRaises(ValueError): module.require_storage_headroom(Path('/tmp'))
        with patch.object(module.os, 'statvfs', return_value=SimpleNamespace(f_bavail=1000000, f_frsize=4096, f_favail=100000)):
            module.require_storage_headroom(Path('/tmp'))

    def test_partial_jurisdiction_counts_do_not_claim_complete_coverage(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref = 'us-co-county-08051'
                with db:
                    db.execute('INSERT INTO discovery VALUES (?,?,?,?)',
                               (ref, 'https://example.org/', '2026-11-11T00:00:00+00:00',
                                json.dumps({'jurisdiction_ref': ref, 'state': 'directory_site_checked'})))
                    for number, state in enumerate(['no_match', 'no_match', 'error', 'pending']):
                        db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,updated_at)
                                      VALUES (?,?,?,?,?)''', (ref, f'https://example.org/{number}.pdf', '{}', state, '2026-11-11T00:00:00+00:00'))
                    redirected = {'error': 'source redirected; alternate document queued',
                                  'alternate_candidate': {'url': 'https://example.org/alternate.pdf'}}
                    db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at)
                                  VALUES (?,?,?,'error',?,?)''', (ref, 'https://example.org/redirect.pdf', '{}', json.dumps(redirected), module.utcnow()))
                    db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,updated_at)
                                  VALUES (?,?,?,'pending',?)''', (ref, redirected['alternate_candidate']['url'], '{}', module.utcnow()))
                    db.execute('INSERT INTO civic_history_checks VALUES (?,?,?,?,?,?,?,?,?)',
                               (ref, 'example.org', 1, 2025, '{}', module.utcnow(), 'ok', None, '{}'))
                    db.execute('INSERT INTO civic_history_checks VALUES (?,?,?,?,?,?,?,?,?)',
                               (ref, 'example.org', 2, 2025, '{}', module.utcnow(), 'error', 'timeout', None))
                output = root / 'public.json'
                module.export_report(root, db, output)
                attempt = json.loads(output.read_text())['coverage']['search_attempts'][0]
                self.assertEqual((attempt['documents_screened'], attempt['documents_failed'], attempt['documents_queued']), (2, 1, 2))
                self.assertEqual(attempt['documents_redirected'], 1)
                self.assertEqual(attempt['historical_index_pages_checked'], 1)
                self.assertEqual(attempt['historical_index_errors'], 1)
                self.assertIsNone(attempt['lookback_period'])
            finally:
                db.close()

    def test_export_uses_a_consistent_snapshot_during_collection(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            reader = module.root_store(root)
            writer = sqlite3.connect(root / 'research.sqlite3')
            try:
                def export(_root, db, _output, _assets):
                    self.assertEqual(db.execute('SELECT count(*) FROM discovery').fetchone()[0], 0)
                    writer.execute('INSERT INTO discovery VALUES (?,?,?,?)',
                                   ('us-co-county-08051', 'https://example.org/', module.utcnow(), '{}'))
                    writer.commit()
                    self.assertEqual(db.execute('SELECT count(*) FROM discovery').fetchone()[0], 0)
                with patch.object(module, 'export_report', side_effect=export):
                    module.export_consistent(root, reader, root / 'public.json')
                self.assertEqual(reader.execute('SELECT count(*) FROM discovery').fetchone()[0], 1)
            finally:
                writer.close()
                reader.close()

    def test_legacy_result_keeps_its_index_binding_before_rediscovery(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            url = 'https://example.org/meeting.pdf'
            candidate = {'url': url, 'index_sha256': 'a' * 64, 'anchor_index': 2}
            db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,attempts,result_json,updated_at)
                          VALUES (?,?,?,?,?,?,?)''', ('us-co-county-08051', url, json.dumps(candidate), 'no_match', 0,
                          json.dumps({'state': 'no_match'}), module.utcnow()))
            db.commit()
            db.close()
            db = module.root_store(root)
            try:
                db.execute('UPDATE document_candidates SET candidate_json=?', (json.dumps({**candidate, 'index_sha256': 'b' * 64}),))
                saved = json.loads(db.execute('SELECT result_json FROM document_candidates').fetchone()[0])
                self.assertEqual(saved['candidate'], candidate)
            finally:
                db.close()

    def test_failed_discovery_preserves_previous_evidence_and_reports_latest_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref = 'us-co-county-08051'
                original = {'jurisdiction_ref': ref, 'website_lead': 'https://example.org/',
                            'checked_at': '2026-11-10T00:00:00+00:00', 'state': 'directory_site_checked',
                            'portals': [{'client': 'example'}], 'documents': []}
                module.record_discovery(db, original)
                failed = {**original, 'checked_at': '2026-11-11T00:00:00+00:00', 'state': 'failed',
                          'portals': [], 'documents': [], 'error': 'timeout'}
                module.record_discovery(db, failed)
                self.assertEqual(json.loads(db.execute('SELECT result_json FROM discovery').fetchone()[0]), original)
                self.assertEqual(db.execute('SELECT client FROM portals').fetchone()[0], 'example')
                output = root / 'public.json'
                module.export_report(root, db, output)
                attempt = json.loads(output.read_text())['coverage']['search_attempts'][0]
                self.assertEqual((attempt['website_check'], attempt['checked_at']),
                                 ('unavailable', failed['checked_at']))
                recovered = {**original, 'checked_at': '2026-11-12T00:00:00+00:00', 'portals': []}
                module.record_discovery(db, recovered)
                self.assertEqual(db.execute('SELECT client FROM portals').fetchone()[0], 'example')
                module.export_report(root, db, output)
                attempt = json.loads(output.read_text())['coverage']['search_attempts'][0]
                self.assertEqual((attempt['website_check'], attempt['checked_at']),
                                 ('site_checked', recovered['checked_at']))
            finally:
                db.close()

    def test_portal_scans_rotate_clients_and_resume_bounded_pagination(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            calls = []
            def fetch(url, *_args, **_kwargs):
                parsed = urlsplit(url)
                client = parsed.path.split('/')[2]
                offset = int(parse_qs(parsed.query)['$skip'][0])
                calls.append((client, offset))
                body = b'[]' if (client, offset) == ('a', 4) else b'[{}]'
                return [(receipt(url, body), body)]
            try:
                with patch.object(module, 'clients_from_store', return_value={'a': 'A', 'b': 'B'}), \
                     patch.object(module, 'get_robots', return_value=(None, [])), \
                     patch.object(module, 'allowed', return_value=True), \
                     patch.object(module, 'archive'), patch.object(module, 'get', side_effect=fetch):
                    module.scan(root, db, 1, 2, 1)
                    module.scan(root, db, 1, 2, 1)
                    module.scan(root, db, 1, 2, 1)
                    module.scan(root, db, 1, 2, 1, only='a')
                self.assertEqual(calls, [('a', 0), ('a', 1), ('b', 0), ('b', 1), ('a', 2), ('a', 3), ('a', 4)])
                self.assertEqual(db.execute("SELECT next_page FROM scan_progress WHERE client='a'").fetchone()[0], 0)
                self.assertEqual(db.execute("SELECT next_page FROM scan_progress WHERE client='b'").fetchone()[0], 2)
                with db:
                    db.execute("UPDATE scan_progress SET next_page=5 WHERE client='a'")
                with patch.object(module, 'clients_from_store', return_value={'a': 'A'}), \
                     patch.object(module, 'get_robots', return_value=(None, [])), \
                     patch.object(module, 'allowed', return_value=True), patch.object(module, 'archive'), \
                     patch.object(module, 'get', side_effect=OSError('fixture timeout')):
                    module.scan(root, db, 1, 2, 1)
                self.assertEqual(db.execute("SELECT next_page FROM scan_progress WHERE client='a'").fetchone()[0], 5)
                with patch.object(module, 'clients_from_store', return_value={'a': 'A'}), \
                     patch.object(module, 'get_robots', return_value=(None, [])), \
                     patch.object(module, 'allowed', return_value=True), patch.object(module, 'archive'), \
                     patch.object(module, 'get', side_effect=fetch):
                    module.scan(root, db, 1, 1, 2)
                self.assertEqual(calls[-1], ('a', 0))
                self.assertEqual(db.execute("SELECT next_page,page_size FROM scan_progress WHERE client='a'").fetchone(), (0, 2))
            finally:
                db.close()

    def test_portal_search_batches_expand_terms_without_losing_legacy_queries(self):
        self.assertIn('ALPR', module.matching_terms('ALPR camera purchase'))
        self.assertIn('RTCC', module.matching_terms('RTCC technology agreement'))
        self.assertNotIn('StingRay', module.matching_terms('Stingray fish exhibit'))
        self.assertNotIn('Motorola Solutions', module.matching_terms('Motorola Solutions radio replacements'))
        self.assertIn('Motorola Solutions', module.matching_terms('Motorola Solutions license plate cameras'))
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            calls = []
            def fetch(url, *_args, **_kwargs):
                calls.append(url)
                body = b'[]'
                return [(receipt(url, body), body)]
            try:
                with db:
                    db.execute('insert into scan_progress values (?,?,?,?)', ('a', 9, 50, module.utcnow()))
                    db.execute('insert into scan_term_versions values (?,?,?)', ('a', module.LEGACY_TERM_VERSION, 0))
                with patch.object(module, 'clients_from_store', return_value={'a': 'A'}), \
                     patch.object(module, 'get_robots', return_value=(None, [])), \
                     patch.object(module, 'archive'), patch.object(module, 'get', side_effect=fetch):
                    module.scan(root, db, 1, 1, 50)
                    module.scan(root, db, 1, 1, 50)
                self.assertEqual(calls, [module.query_url('a', 0, 50, module.TERM_VERSION, 0),
                                         module.query_url('a', 0, 50, module.TERM_VERSION, 1)])
                self.assertEqual(db.execute("select version,batch from scan_term_versions where client='a'").fetchone(),
                                 (module.TERM_VERSION, 0))
                self.assertNotEqual(module.query_url('a', 0, 50, module.LEGACY_TERM_VERSION), calls[0])
                with self.assertRaises(ValueError): module.query_url('a', 0, 50, 'unrecognized')
            finally:
                db.close()

    def test_legacy_query_receipts_remain_publishable_after_search_terms_change(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                client, ref = 'denver', 'us-co-place-0820000'
                url = module.query_url(client, 0, 100, module.LEGACY_TERM_VERSION)
                source = [{'MatterId': 123, 'MatterTitle': 'Flock camera contract', 'MatterFile': '24-1', 'MatterStatusName': 'Filed'}]
                body = json.dumps(source).encode()
                obs = receipt(url, body)
                module.archive(root, [(obs, body)])
                with db:
                    db.execute('insert into scan_history values (?,?,?,?)', (client, 0, obs['sha256'], json.dumps(obs)))
                    db.execute('insert into leads values (?,?,?,?,?,?,?,?,?)',
                               ('co-legistar-denver-123', ref, client, 123, 0, 0, obs['sha256'],
                                json.dumps({'title': source[0]['MatterTitle'], 'rule_version': module.LEGACY_TERM_VERSION}), module.utcnow()))
                output = root / 'public.json'
                module.export_report(root, db, output)
                self.assertEqual(len(json.loads(output.read_text())['records']), 1)
            finally:
                db.close()

    def test_radio_only_vendor_leads_remain_private_without_surveillance_context(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                client, ref = 'denver', 'us-co-place-0820000'
                url = module.query_url(client, 0, 100, module.TERM_VERSION, 1)
                source = [{'MatterId': 123, 'MatterTitle': 'Motorola Solutions radio replacements', 'MatterFile': '24-1'}]
                body = json.dumps(source).encode()
                obs = receipt(url, body)
                module.archive(root, [(obs, body)])
                with db:
                    db.execute('insert into scan_history values (?,?,?,?)', (client, 0, obs['sha256'], json.dumps(obs)))
                    db.execute('insert into leads values (?,?,?,?,?,?,?,?,?)',
                               ('co-legistar-denver-123', ref, client, 123, 0, 0, obs['sha256'],
                                json.dumps({'title': source[0]['MatterTitle']}), module.utcnow()))
                output = root / 'public.json'
                module.export_report(root, db, output)
                report = json.loads(output.read_text())
                self.assertEqual(report['records'], [])
                self.assertEqual(report['coverage']['non_surveillance_mentions_withheld'], 1)
                self.assertEqual(report['coverage']['omitted_failed_provenance'], 0)
            finally:
                db.close()

    def test_legacy_substring_false_positive_is_filtered_without_provenance_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                client, ref = 'denver', 'us-co-place-0820000'
                url = module.query_url(client, 0, 100, module.LEGACY_TERM_VERSION)
                source = [{'MatterId': 123, 'MatterTitle': 'Development plan on Faxon Court', 'MatterFile': '24-1'}]
                body = json.dumps(source).encode()
                obs = receipt(url, body)
                module.archive(root, [(obs, body)])
                with db:
                    db.execute('insert into scan_history values (?,?,?,?)', (client, 0, obs['sha256'], json.dumps(obs)))
                    db.execute('insert into leads values (?,?,?,?,?,?,?,?,?)',
                               ('co-legistar-denver-123', ref, client, 123, 0, 0, obs['sha256'],
                                json.dumps({'title': source[0]['MatterTitle']}), module.utcnow()))
                output = root / 'public.json'
                module.export_report(root, db, output)
                report = json.loads(output.read_text())
                self.assertEqual(report['records'], [])
                self.assertEqual(report['coverage']['non_surveillance_mentions_withheld'], 1)
                self.assertEqual(report['coverage']['omitted_failed_provenance'], 0)
            finally:
                db.close()

    def test_retry_and_action_queues_prioritize_unattempted_then_oldest(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                for ref, attempted in [('a', '2000-01-02T00:00:00+00:00'),
                                       ('b', '2000-01-01T00:00:00+00:00'), ('z', None)]:
                    url = f'https://example.org/{ref}.pdf'
                    db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,
                                  result_json,updated_at,last_attempt_at) VALUES (?,?,?,'error','{}',?,?)''',
                               (ref, url, json.dumps({'url': url}), module.utcnow(), attempted))
                db.commit()
                selected = []
                def screen(_root, ref, *_args):
                    selected.append(ref)
                    return {'state': 'error', 'error': 'fixture failure'}, []
                with patch.object(module, 'screen_one', side_effect=screen):
                    module.screen_documents(root, db, '/bin/true', 1, 1, 1, retry_errors=True)
                    module.screen_documents(root, db, '/bin/true', 1, 1, 1, retry_errors=True)
                self.assertEqual(selected, ['z', 'b'])
                for client, matter in [('a', 1), ('z', 2)]:
                    db.execute('INSERT INTO leads VALUES (?,?,?,?,?,?,?,?,?)',
                               (client, 'ref', client, matter, 0, 0, 'a' * 64, '{}', module.utcnow()))
                db.execute("INSERT INTO matter_action_checks(client,matter_id,checked_at,status) VALUES ('a',1,'2000-01-01T00:00:00+00:00','error')")
                db.commit()
                with patch.object(module, 'clients_from_store', return_value={}), \
                     patch.object(module, 'get_robots', return_value=(None, [])), patch.object(module, 'archive'):
                    module.collect_actions(root, db, 1)
                self.assertEqual(db.execute("SELECT status FROM matter_action_checks WHERE client='z'").fetchone()[0], 'unbound_client')
                self.assertEqual(db.execute("SELECT status FROM matter_action_checks WHERE client='a'").fetchone()[0], 'error')
            finally:
                db.close()

    def test_portrait_queue_rotates_after_failed_lookups(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                content = b'fixture\n'
                digest = module.sha(content)
                (root / 'extracted').mkdir()
                (root / 'extracted' / digest).write_bytes(content)
                result = json.dumps({'extracted_sha256': digest,
                                     'finding': {'citations': [{'locator': {'start': 1}}]}})
                for ref in ['a', 'b', 'z']:
                    db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (ref, 'https://example.org/', module.utcnow(), '{}'))
                    db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at)
                                  VALUES (?,?,?,'matched',?,?)''', (ref, f'https://example.org/{ref}.pdf', '{}', result, module.utcnow()))
                db.commit()
                with patch.object(module, 'officials_near_reference', return_value=[{'name': 'Test Official'}]), \
                     patch.object(module, 'observation_keys', return_value=set()), \
                     patch.object(module, 'clients_from_store', return_value={}), \
                     patch.object(module, 'get_robots', return_value=(None, [])), patch.object(module, 'archive'), \
                     patch.object(module, 'html_page', side_effect=OSError('fixture unavailable')):
                    module.collect_portraits(root, db, 2)
                    self.assertEqual({row[0] for row in db.execute('SELECT jurisdiction_ref FROM portrait_attempts')}, {'a', 'b'})
                    module.collect_portraits(root, db, 2)
                self.assertEqual({row[0] for row in db.execute('SELECT jurisdiction_ref FROM portrait_attempts')}, {'a', 'b', 'z'})
            finally:
                db.close()

    def test_portrait_search_continues_to_linked_council_when_directory_is_missing(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref, name = 'us-co-county-08051', 'Alex Example'
                home, council = 'https://town.example.org/', 'https://town.example.org/council'
                image_url = 'https://town.example.org/alex.jpg'
                extracted = b'fixture\n'
                digest = module.sha(extracted)
                (root / 'extracted').mkdir()
                (root / 'extracted' / digest).write_bytes(extracted)
                finding = json.dumps({'extracted_sha256': digest, 'finding': {'citations': [{'locator': {'start': 1}}]}})
                with db:
                    db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (ref, home, module.utcnow(), '{}'))
                    db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at)
                                  VALUES (?,?,?,'matched',?,?)''', (ref, home + 'minutes.pdf', '{}', finding, module.utcnow()))
                html = {home: b'<a href="/council">Town Council</a>',
                        council: b'<img alt="Councilmember Alex Example" src="/alex.jpg">'}
                calls = []
                def fetch_page(url, *_args):
                    calls.append(url)
                    if url not in html:
                        raise ValueError('HTTP 404')
                    return html[url], [(receipt(url, html[url]), html[url])]
                jpeg = b'\xff\xd8\xfforiginal'
                with patch.object(module, 'officials_near_reference', return_value=[{'name': name}]), \
                     patch.object(module, 'clients_from_store', return_value={}), \
                     patch.object(module, 'get_robots', return_value=(None, [])), \
                     patch.object(module, 'html_page', side_effect=fetch_page), \
                     patch.object(module, 'get', return_value=[(receipt(image_url, jpeg), jpeg)]):
                    module.collect_portraits(root, db, 1)
                self.assertEqual(calls, [home + 'm/Directory', home, council])
                result = module.checked_portrait(root, db, module.observation_keys(root), ref, name, root / 'assets')
                self.assertEqual(result['source_url'], council)
            finally:
                db.close()

    def test_candidate_document_scope(self):
        index = 'https://city.example.org/agenda'
        good = 'https://city.example.org/AgendaCenter/ViewFile/Minutes/_09152026-1206'
        self.assertEqual(module.candidate_kind(good, index, ''), ('minutes', '2026-09-15'))
        self.assertEqual(module.candidate_kind('https://city.example.org/uploads/council-minutes.pdf', index, 'Minutes')[0], 'minutes')
        self.assertEqual(module.candidate_kind('https://city.example.org/uploads/council-minutes.docx', index, 'Minutes')[0], 'minutes')
        self.assertEqual(module.candidate_kind('https://city.example.org/uploads/council%20minutes.pdf', index, 'Minutes')[0], 'minutes')
        for bad in ['http://city.example.org/uploads/agenda.pdf', 'https://evil.example/uploads/agenda.pdf',
                    'https://city.example.org/uploads/agenda.pdf?token=x', 'https://city.example.org/uploads/%2e%2e/agenda.pdf',
                    'https://city.example.org/uploads/agenda.pdf?t=123&redirect=x',
                    'https://city.example.org/uploads/agenda.pdf?t=1%32',
                    'https://city.example.org/uploads/%252e%252e/agenda.pdf', 'https://city.example.org/uploads%2fagenda.pdf',
                    'https://city.example.org/uploads/%0aagenda.pdf', 'https://city.example.org/uploads/%ZZagenda.pdf',
                    'https://city.example.org/uploads/unrelated.pdf']:
            self.assertIsNone(module.candidate_kind(bad, index, ''))
        self.assertEqual(module.candidate_kind('https://city.example.org/uploads/agenda.pdf?t=202611110000000', index, '')[0], 'agenda')

    def test_towncloud_requires_official_link_exact_tenant_and_public_pdf_shape(self):
        index = 'https://town.example.org/agendas'
        origin = 'https://towncloud.io/go/walden-co/agendas/293/PDF/get_agenda_document_link'
        target = 'https://towncloud-core-prod.s3.us-west-2.amazonaws.com/uploads/walden-co/meetings/agenda_packet/pdf/289/meeting.pdf'
        self.assertEqual(module.candidate_kind(origin, index, 'Agenda PDF'), ('agenda', None))
        self.assertIsNone(module.candidate_kind(origin, index, 'Unrelated'))
        self.assertTrue(module.towncloud_file_redirect(origin, target))
        for bad in [target.replace('/walden-co/', '/other-town/'),
                    target.replace('towncloud-core-prod.s3.us-west-2.amazonaws.com', 'attacker.example'),
                    target + '?token=1', target.replace('/289/', '/%2e%2e/'),
                    target.replace('.pdf', '.html')]:
            self.assertFalse(module.towncloud_file_redirect(origin, bad))
        for bad in [origin + '?other=1', origin.replace('towncloud.io', 'towncloud.io.attacker.example'),
                    origin.replace('/293/', '/0/'), origin.replace('PDF/get_agenda_document_link', 'other')]:
            self.assertIsNone(module.candidate_kind(bad, index, 'Agenda PDF'))

    def test_towncloud_override_requires_setting_and_preserved_redirect_receipt(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                index = 'https://town.example.org/agendas'
                origin = 'https://towncloud.io/go/walden-co/agendas/293/PDF/get_agenda_document_link'
                target = 'https://towncloud-core-prod.s3.us-west-2.amazonaws.com/uploads/walden-co/meetings/agenda_packet/pdf/289/meeting.pdf'
                body = f'<a href="{origin}">Agenda PDF</a>'.encode()
                redirect = receipt(origin, b'')
                redirect.update(status=302, redirect_target=target)
                module.archive(root, [(receipt(index, body), body), (redirect, b'')])
                original = {'url': origin, 'index_url': index, 'index_sha256': module.sha(body), 'anchor_index': 0}
                alternate = {**original, 'url': target, 'linked_url': origin, 'origin_url': origin,
                             'capture_adjustment': 'direct_redirect', 'redirect_receipt': redirect}
                module.checked_document_candidate(root, target, alternate)
                with patch.dict(module.os.environ, {'PNUL_ALLOW_TOWNCLOUD_PUBLIC': '0'}):
                    self.assertFalse(module.towncloud_override(original, origin))
                    self.assertFalse(module.towncloud_override(alternate, target))
                with patch.dict(module.os.environ, {'PNUL_ALLOW_TOWNCLOUD_PUBLIC': '1'}):
                    self.assertTrue(module.towncloud_override(original, origin))
                    self.assertTrue(module.towncloud_override(alternate, target))
                    self.assertFalse(module.towncloud_override(alternate, target.replace('/walden-co/', '/other/')))
                broken = {**alternate, 'redirect_receipt': {**redirect, 'redirect_target': target + '?changed=1'}}
                with self.assertRaises(ValueError):
                    module.checked_document_candidate(root, target, broken)
            finally:
                db.close()

    def test_towncloud_override_only_fetches_source_bound_documents(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                index = 'https://town.example.org/agendas'
                origin = 'https://towncloud.io/go/walden-co/agendas/293/PDF/get_agenda_document_link'
                target = 'https://towncloud-core-prod.s3.us-west-2.amazonaws.com/uploads/walden-co/meetings/agenda_packet/pdf/289/meeting.pdf'
                body = f'<a href="{origin}">Agenda PDF</a>'.encode()
                redirect = receipt(origin, b''); redirect.update(status=302, redirect_target=target)
                module.archive(root, [(receipt(index, body), body), (redirect, b'')])
                original = {'url': origin, 'index_url': index, 'index_sha256': module.sha(body), 'anchor_index': 0}
                denied = SimpleNamespace(can_fetch=lambda *_: False)
                with patch.object(module, 'cached_robots', return_value=(denied, [])), \
                     patch.object(module, 'get', return_value=[(redirect, b'')]) as fetch:
                    with patch.dict(module.os.environ, {'PNUL_ALLOW_TOWNCLOUD_PUBLIC': '0'}):
                        result, _ = module.screen_one(root, 'ref', original, '/bin/true', threading.Lock(),
                                                      module.DownloadBudget(30_000_000), False, {})
                    self.assertEqual(result['state'], 'error');fetch.assert_not_called()
                    with patch.dict(module.os.environ, {'PNUL_ALLOW_TOWNCLOUD_PUBLIC': '1'}):
                        result, _ = module.screen_one(root, 'ref', original, '/bin/true', threading.Lock(),
                                                      module.DownloadBudget(30_000_000), False, {})
                    alternate = result['alternate_candidate']
                    self.assertEqual(alternate['url'], target)
                robots = receipt('https://towncloud-core-prod.s3.us-west-2.amazonaws.com/robots.txt', b'')
                robots['status'] = 403
                pdf = b'%PDF-fixture'
                with patch.object(module, 'cached_robots', side_effect=module.RobotsDenied(403, [(robots, b'')])), \
                     patch.object(module, 'get', return_value=[(receipt(target, pdf), pdf)]) as fetch, \
                     patch.object(module, 'extract_captured_pdf', return_value={'state': 'no_match'}):
                    with patch.dict(module.os.environ, {'PNUL_ALLOW_TOWNCLOUD_PUBLIC': '0'}):
                        result, _ = module.screen_one(root, 'ref', alternate, '/bin/true', threading.Lock(),
                                                      module.DownloadBudget(30_000_000), False, {})
                    self.assertEqual(result['state'], 'error');fetch.assert_not_called()
                    with patch.dict(module.os.environ, {'PNUL_ALLOW_TOWNCLOUD_PUBLIC': '1'}):
                        result, _ = module.screen_one(root, 'ref', alternate, '/bin/true', threading.Lock(),
                                                      module.DownloadBudget(30_000_000), False, {})
                    self.assertEqual(result['state'], 'no_match');fetch.assert_called_once()
            finally:
                db.close()

    def test_same_site_html_base_and_timestamp_remain_bound_to_the_original_anchor(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                index = 'https://city.example.org/your_city/agendas.php'
                page = (b'<base href="https://city.example.org/">'
                        b'<a href="Documents/City Council/Minutes.pdf?t=202611110000000">Minutes</a>')
                url = 'https://city.example.org/Documents/City Council/Minutes.pdf?t=202611110000000'
                module.archive(root, [(receipt(index, page), page)])
                links, base = module.page_links(page, index)
                self.assertEqual(module.resolve_page_link(index, base, links[0][0]), url)
                candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0}
                module.checked_document_candidate(root, url, candidate)
                with self.assertRaises(ValueError):
                    module.checked_document_candidate(root, url.split('?')[0], {**candidate, 'url': url.split('?')[0]})
                with self.assertRaises(ValueError):
                    module.checked_document_candidate(root, url.replace('202611110000000', '1'),
                                                      {**candidate, 'url': url.replace('202611110000000', '1')})
            finally:
                db.close()

    def test_document_redirect_is_preserved_without_exhausting_the_byte_budget(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                index, url = 'https://city.example.org/agenda', 'https://city.example.org/agenda.pdf'
                page = b'<a href="/agenda.pdf">Agenda</a>'
                module.archive(root, [(receipt(index, page), page)])
                candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0}
                redirect_body = b'redirect'
                redirect = receipt(url, redirect_body)
                redirect.update(status=302, redirect_target='https://other.example/agenda.pdf')
                budget = module.DownloadBudget(25 * 1024 * 1024)
                with patch.object(module, 'cached_robots', return_value=(None, [])), \
                     patch.object(module, 'get', return_value=[(redirect, redirect_body)]) as fetch:
                    result, observations = module.screen_one(root, 'us-co-county-08051', candidate,
                                                             '/unused', threading.Lock(), budget, False, {})
                self.assertEqual(result['state'], 'error')
                self.assertIn('HTTP 302', result['error'])
                self.assertEqual(observations, [(redirect, redirect_body)])
                self.assertEqual(budget.remaining, 25 * 1024 * 1024 - len(redirect_body))
                self.assertTrue(fetch.call_args.kwargs['stop_at_redirect'])
            finally:
                db.close()

    def test_small_remaining_run_budget_defers_instead_of_mislabeling_a_document(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                index, url = 'https://city.example.org/agenda', 'https://city.example.org/agenda.pdf'
                page = b'<a href="/agenda.pdf">Agenda</a>'
                module.archive(root, [(receipt(index, page), page)])
                candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0}
                with patch.object(module, 'cached_robots', return_value=(None, [])), \
                     patch.object(module, 'get', side_effect=ValueError('HTTP response exceeds byte limit')):
                    small, _ = module.screen_one(root, 'us-co-county-08051', candidate, '/unused', threading.Lock(),
                                                 module.DownloadBudget(2 * 1024 * 1024), False, {})
                    large, _ = module.screen_one(root, 'us-co-county-08051', candidate, '/unused', threading.Lock(),
                                                 module.DownloadBudget(25 * 1024 * 1024), False, {})
                self.assertEqual(small['state'], 'deferred')
                self.assertEqual(large['state'], 'error')
            finally:
                db.close()

    def test_vendor_document_requires_preserved_redirect_and_marks_timestamp_uncertainty(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref, index = 'us-co-county-08051', 'https://example.org/agenda'
                linked = 'https://example.org/Documents/Meeting Minutes.docx?t=123'
                origin = linked.split('?')[0]
                target = 'https://cms3.revize.com/revize/town/Documents/Meeting Minutes.docx'
                page = b'<a href="/Documents/Meeting Minutes.docx?t=123">Minutes</a>'
                module.archive(root, [(receipt(index, page), page)])
                candidate = {'url': linked, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0, 'label': 'Minutes'}
                first = receipt(linked, b'')
                first.update(status=302, redirect_target=target + '?t=123')
                second = receipt(origin, b'')
                second.update(status=302, redirect_target=target)
                def fetch(url, _hosts, **_kwargs):
                    return [(first, b'')] if url == linked else [(second, b'')]
                with patch.object(module, 'cached_robots', return_value=(None, [])), patch.object(module, 'get', side_effect=fetch):
                    result, observations = module.screen_one(root, ref, candidate, '/unused', threading.Lock(),
                                                             module.DownloadBudget(50 * 1024 * 1024), False, {})
                self.assertEqual(result['state'], 'error')
                alternate = result['alternate_candidate']
                self.assertEqual(alternate['capture_adjustment'], 'timestamp_omitted')
                module.archive(root, observations)
                module.checked_document_candidate(root, target, alternate)
                for changed in ({'linked_url': origin}, {'origin_url': linked}, {'capture_adjustment': 'direct_redirect'},
                                {'indexed_redirect_receipt': {}},
                                {'url': target.replace('cms3.revize.com', 'attacker.example')},
                                {'url': target.replace('Meeting Minutes', 'Other Minutes')}):
                    bad = {**alternate, **changed}
                    with self.subTest(changed=changed), self.assertRaises(ValueError):
                        module.checked_document_candidate(root, bad['url'], bad)
                document = b'PK\x03\x04fixture'
                capture = receipt(target, document)
                module.archive(root, [(capture, document)])
                response = SimpleNamespace(returncode=0, stdout=b'Flock Safety discussion\n', stderr=b'')
                with patch.object(module.shutil, 'which', side_effect=lambda name, **_: '/usr/bin/' + name), \
                     patch.object(module.subprocess, 'run', return_value=response):
                    screened = module.extract_saved_docx(root, ref, alternate, capture)
                with db:
                    db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (ref, 'https://example.org/', module.utcnow(), '{}'))
                    db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at)
                                  VALUES (?,?,?,'matched',?,?)''', (ref, target, json.dumps(alternate), json.dumps(screened), module.utcnow()))
                output = root / 'public.json'
                module.export_report(root, db, output)
                record = json.loads(output.read_text())['records'][0]
                self.assertEqual(record['source_url'], target)
                self.assertEqual(record['indexed_source_url'], linked)
                self.assertEqual(record['source_index_url'], index)
                self.assertEqual(record['capture_adjustment'], 'timestamp_omitted')
                (root / 'observations' / (module.sha((json.dumps(second, sort_keys=True, separators=(',', ':')) + '\n').encode()) + '.json')).unlink()
                with self.assertRaises(ValueError):
                    module.checked_document_candidate(root, target, alternate)
            finally:
                db.close()

    def test_document_queue_prioritizes_preserved_vendor_redirects_per_jurisdiction(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref = 'us-co-county-08051'
                normal = {'url': 'https://www.example.org/z-agenda.pdf'}
                vendor = {'url': 'https://cms3.revize.com/revize/example/a-agenda.pdf', 'redirect_receipt': {'status': 302}}
                with db:
                    for candidate in (normal, vendor):
                        db.execute("insert into document_candidates(jurisdiction_ref,url,candidate_json,state,updated_at) values (?,?,?,'pending',?)",
                                   (ref, candidate['url'], json.dumps(candidate), module.utcnow()))
                with patch.object(module, 'screen_one', return_value=({'state': 'error', 'error': 'fixture'}, [])) as screen:
                    module.screen_documents(root, db, '/bin/true', 1, 1, 1)
                self.assertEqual(screen.call_args.args[2]['url'], vendor['url'])
            finally:
                db.close()

    def test_foreign_html_base_cannot_rebind_relative_documents_or_navigation(self):
        page = 'https://city.example.org/agendas/index.html'
        body = (b'<base href="https://attacker.example/">'
                b'<a href="minutes.pdf">Minutes</a>'
                b'<a href="https://city.example.org/real-minutes.pdf">Minutes</a>')
        links, base = module.page_links(body, page)
        self.assertIsNone(base)
        self.assertIsNone(module.resolve_page_link(page, base, links[0][0]))
        self.assertEqual(module.resolve_page_link(page, base, links[1][0]), 'https://city.example.org/real-minutes.pdf')
        self.assertNotIn('https://city.example.org/agendas/minutes.pdf', module.discovery_targets(links, page, base))

    def test_civic_year_selectors_accept_only_bounded_numeric_values(self):
        body = (b'<a href="javascript:changeYear(2025, 1, \'a1\')">2025</a>'
                b'<a href="javascript:changeYear(2025, 1, \'a1\')">duplicate</a>'
                b'<a href="javascript:changeYear(2024, 9, \'a2\');alert(1)">bad</a>'
                b'<a href="javascript:changeYear(1999, 9, \'a2\')">old</a>'
                b'<a href="javascript:changeYear(2026, 9, \'a2\')">current</a>')
        self.assertEqual(module.civic_year_selectors(body, 2026), [(2025, 1, 0)])

    def test_civic_history_binds_official_index_and_queues_once(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                year = datetime.now(timezone.utc).year - 1
                page = 'https://town.example.org/AgendaCenter'
                selector = f'<a href="javascript:changeYear({year}, 1, \'a1\')">{year}</a>'.encode()
                observed = receipt(page, selector)
                observed['content_type'] = 'text/html'
                module.archive(root, [(observed, selector)])
                candidate = {'index_url': page, 'index_sha256': module.sha(selector)}
                entry = {'state': 'directory_site_checked', 'documents': [candidate]}
                db.execute('INSERT INTO discovery VALUES (?,?,?,?)', ('us-co-place-0810105', 'https://town.example.org/', module.utcnow(), json.dumps(entry)))
                db.commit()
                calls = []

                def fetch(url, *_args, **_kwargs):
                    calls.append(url)
                    body = (f'<a href="/AgendaCenter/ViewFile/Minutes/_0128{year}-1883">Minutes</a>'
                            f'<a href="/AgendaCenter/ViewFile/Minutes/_0128{year-1}-1884">Wrong year</a>').encode()
                    metadata = receipt(url, body)
                    metadata['content_type'] = 'text/html'
                    return [(metadata, body)]

                with patch.object(module, 'get_robots', return_value=(None, [])), patch.object(module, 'get', side_effect=fetch):
                    module.civic_history(root, db, 5, 5, 1)
                    module.civic_history(root, db, 5, 5, 1)
                self.assertEqual(len(calls), 1)
                self.assertEqual(parse_qs(urlsplit(calls[0]).query)['year'], [str(year)])
                row = db.execute('SELECT url,candidate_json,state FROM document_candidates').fetchone()
                self.assertEqual(row[2], 'pending')
                self.assertEqual(db.execute('SELECT count(*) FROM document_candidates').fetchone()[0], 1)
                module.checked_document_candidate(root, row[0], json.loads(row[1]))
                self.assertEqual(db.execute('SELECT state FROM civic_history_checks').fetchone()[0], 'ok')
                db.execute('DELETE FROM civic_history_checks')
                db.execute('DELETE FROM document_candidates')
                db.commit()
                with patch.object(module, 'get_robots', return_value=(None, [])), \
                     patch.object(module, 'get', side_effect=module.BudgetExhausted('job budget exhausted')):
                    module.civic_history(root, db, 5, 5, 1)
                self.assertEqual(db.execute('SELECT count(*) FROM civic_history_checks').fetchone()[0], 0)
            finally:
                db.close()

    def test_civic_history_rotates_places_and_respects_robots(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                year = datetime.now(timezone.utc).year - 1
                for ref, host in [('us-co-county-08001', 'a.example.org'), ('us-co-county-08003', 'b.example.org')]:
                    page = f'https://{host}/AgendaCenter'
                    body = f'<a href="javascript:changeYear({year}, 1, \'a1\')">{year}</a>'.encode()
                    metadata = receipt(page, body)
                    metadata['content_type'] = 'text/html'
                    module.archive(root, [(metadata, body)])
                    entry = {'state': 'directory_site_checked', 'documents': [{'index_url': page, 'index_sha256': module.sha(body)}]}
                    db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (ref, f'https://{host}/', module.utcnow(), json.dumps(entry)))
                db.commit()
                calls = []

                def fetch(url, *_args, **_kwargs):
                    calls.append(urlsplit(url).hostname)
                    metadata = receipt(url, b'<p>No entries</p>')
                    metadata['content_type'] = 'text/html'
                    return [(metadata, b'<p>No entries</p>')]

                with patch.object(module, 'get_robots', return_value=(None, [])), patch.object(module, 'get', side_effect=fetch):
                    module.civic_history(root, db, 1, 1, 1)
                    module.civic_history(root, db, 1, 1, 1)
                self.assertEqual(calls, ['a.example.org', 'b.example.org'])
                db.execute('DELETE FROM civic_history_checks')
                db.commit()
                denied = SimpleNamespace(can_fetch=lambda *_args: False)
                with patch.object(module, 'get_robots', return_value=(denied, [])), patch.object(module, 'get') as fetch_mock:
                    module.civic_history(root, db, 2, 1, 2)
                fetch_mock.assert_not_called()
                self.assertEqual(db.execute("SELECT count(*) FROM civic_history_checks WHERE state='error'").fetchone()[0], 2)
                self.assertEqual(db.execute('SELECT count(*) FROM document_candidates').fetchone()[0], 0)
            finally:
                db.close()

    def test_deferred_document_is_not_reported_as_a_completed_screen(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                url = 'https://town.example.org/agenda.pdf'
                db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,updated_at)
                              VALUES (?,?,?,'pending',?)''', ('us-co-county-08001', url, json.dumps({'url': url}), module.utcnow()))
                db.commit()
                output = io.StringIO()
                with patch.object(module, 'screen_one', return_value=({'state': 'deferred'}, [])), redirect_stdout(output):
                    module.screen_documents(root, db, sys.executable, 1, 1, 1)
                self.assertIn('completed screens 0; errors 0; deferred 1', output.getvalue())
                self.assertEqual(db.execute('SELECT state FROM document_candidates').fetchone()[0], 'pending')
            finally:
                db.close()

    def test_queued_redirect_is_counted_separately_from_errors(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                url = 'https://town.example.org/agenda.pdf'
                alt = {'url': 'https://town.example.org/alternate.pdf'}
                db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,updated_at)
                              VALUES (?,?,?,'pending',?)''', ('us-co-county-08001', url, json.dumps({'url': url}), module.utcnow()))
                db.commit()
                result = {'state': 'error', 'error': 'source redirected; alternate document queued', 'alternate_candidate': alt}
                output = io.StringIO()
                with patch.object(module, 'screen_one', return_value=(result, [])), redirect_stdout(output):
                    module.screen_documents(root, db, sys.executable, 1, 1, 1)
                self.assertIn('completed screens 0; errors 0; deferred 0; redirected 1', output.getvalue())
                self.assertEqual(db.execute("SELECT state FROM document_candidates WHERE url=?", (alt['url'],)).fetchone()[0], 'pending')
            finally:
                db.close()

    def test_civic_history_waits_for_an_overfull_document_queue(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,updated_at)
                              VALUES (?,?,?,'pending',?)''', ('us-co-county-08001', 'https://example.org/agenda.pdf', '{}', module.utcnow()))
                db.commit()
                output = io.StringIO()
                with patch.object(module, 'get') as fetch, redirect_stdout(output):
                    module.civic_history(root, db, 2, 1, 1, pending_cap=1)
                fetch.assert_not_called()
                self.assertIn('document queue has 1 pending', output.getvalue())
            finally:
                db.close()

    def test_civic_history_rejects_a_foreign_or_unobserved_index(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                year = datetime.now(timezone.utc).year - 1
                page = 'https://foreign.example.org/AgendaCenter'
                body = f'<a href="javascript:changeYear({year}, 1, \'a1\')">year</a>'.encode()
                module.archive(root, [(receipt(page, body), body)])
                entry = {'state': 'directory_site_checked', 'documents': [{'index_url': page, 'index_sha256': module.sha(body)}]}
                db.execute('INSERT INTO discovery VALUES (?,?,?,?)', ('us-co-county-08001', 'https://town.example.org/', module.utcnow(), json.dumps(entry)))
                db.commit()
                with patch.object(module, 'get_robots') as robots, patch.object(module, 'get') as fetch:
                    module.civic_history(root, db, 2, 1, 1)
                    entry['documents'][0]['index_url'] = 'https://town.example.org/AgendaCenter'
                    db.execute('UPDATE discovery SET result_json=?', (json.dumps(entry),))
                    db.commit()
                    module.civic_history(root, db, 2, 1, 1)
                robots.assert_not_called()
                fetch.assert_not_called()
                self.assertEqual(db.execute('SELECT count(*) FROM civic_history_checks').fetchone()[0], 0)
            finally:
                db.close()

    def test_discovery_prioritizes_same_host_agendas_with_a_bounded_probe(self):
        page = 'https://town.example.org/'
        links = [('/council', 'Council'), ('/commission', 'Commission'), ('/meetings', 'Meetings'),
                 ('/minutes', 'Minutes'), ('/agendas', 'Agenda packets'), ('/legislation', 'Legislation'),
                 ('/more-council', 'More council'), ('https://evil.example.org/agenda', 'Agenda'),
                 ('http://town.example.org/agenda', 'Agenda'), ('https://[broken', 'Agenda'), ('/minutes', 'Minutes')]
        targets = module.discovery_targets(links, page)
        self.assertEqual(targets[:3], ['https://town.example.org/minutes', 'https://town.example.org/agendas',
                                       'https://town.example.org/AgendaCenter'])
        self.assertEqual(len(targets), 6)
        self.assertEqual(len(set(targets)), len(targets))
        self.assertTrue(all(url.startswith('https://town.example.org/') for url in targets))
        self.assertEqual(module.discovery_targets([('/AgendaCenter', 'Agendas')], page),
                         ['https://town.example.org/AgendaCenter'])

    def test_discovery_skips_malformed_links_without_losing_the_site(self):
        home = 'https://town.example.org/'
        body = b'<a href="https://[broken">Agenda</a><a href="/minutes">Minutes</a>'
        def page(url, *_args, **_kwargs):
            meta = receipt(url, body)
            meta['content_type'] = 'text/html'
            return [(meta, body)]
        robots = receipt('https://town.example.org/robots.txt', b'')
        with patch.object(module, 'get_robots', return_value=(None, [(robots, b'')])), \
             patch.object(module, 'get', side_effect=page):
            result, _ = module.discover_one('us-co-county-08051', 'Fixture', home, module.DownloadBudget(10_000_000))
        self.assertEqual(result['state'], 'directory_site_checked')
        self.assertGreaterEqual(result['pages_checked'], 1)

    def test_discovery_reaches_a_second_level_archive_with_bounded_same_host_requests(self):
        home = 'https://town.example.org/'
        pages = {
            home: b'<a href="/meetings">Meetings</a>',
            home + 'meetings': b'<a href="/meeting-archive">Minutes archive</a>',
            home + 'meeting-archive': b'<a href="/documents/council-minutes.pdf">Minutes</a>',
        }
        calls = []
        def fetch(url, hosts, **_kwargs):
            calls.append(url)
            self.assertIn('town.example.org', hosts)
            body = pages.get(url, b'')
            meta = receipt(url, body)
            meta['content_type'] = 'text/html'
            if url not in pages: meta['status'] = 404
            return [(meta, body)]
        robots = receipt(home + 'robots.txt', b'')
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                with patch.object(module, 'get_robots', return_value=(None, [(robots, b'')])), \
                     patch.object(module, 'get', side_effect=fetch):
                    result, observations = module.discover_one('us-co-county-08051', 'Fixture', home,
                                                               module.DownloadBudget(30_000_000))
                self.assertEqual(result['state'], 'directory_site_checked')
                self.assertLessEqual(result['pages_attempted'], module.MAX_DISCOVERY_PAGES)
                self.assertTrue(all(urlsplit(url).hostname == 'town.example.org' for url in calls))
                document = next(item for item in result['documents'] if item['url'].endswith('/council-minutes.pdf'))
                self.assertEqual(document['index_url'], home + 'meeting-archive')
                module.archive(root, observations)
                module.checked_document_candidate(root, document['url'], document)
            finally:
                db.close()

    def test_discovery_caps_attempts_even_when_every_deeper_page_fails(self):
        home = 'https://town.example.org/'
        body = b''.join(f'<a href="/agenda-{i}">Agenda {i}</a>'.encode() for i in range(100))
        calls = []
        def fetch(url, _hosts, **_kwargs):
            calls.append(url)
            data = body if url == home else b''
            meta = receipt(url, data)
            meta.update(content_type='text/html', status=200 if url == home else 404)
            return [(meta, data)]
        robots = receipt(home + 'robots.txt', b'')
        with patch.object(module, 'get_robots', return_value=(None, [(robots, b'')])), \
             patch.object(module, 'get', side_effect=fetch):
            result, _ = module.discover_one('us-co-county-08051', 'Fixture', home, module.DownloadBudget(30_000_000))
        self.assertLessEqual(len(calls), module.MAX_DISCOVERY_PAGES)
        self.assertLessEqual(result['pages_attempted'], module.MAX_DISCOVERY_PAGES)

    def test_discovery_shares_second_level_budget_across_sections(self):
        home = 'https://town.example.org/'
        pages = {home: b''.join(f'<a href="/meeting-section-{i}">Meetings {i}</a>'.encode()
                                for i in range(5))}
        for i in range(5):
            pages[home + f'meeting-section-{i}'] = b''.join(
                f'<a href="/meeting-archive-{i}-{j}">Minutes {j}</a>'.encode()
                for j in range(6))
            for j in range(6):
                pages[home + f'meeting-archive-{i}-{j}'] = (
                    f'<a href="/documents/{i}-{j}-council-minutes.pdf">Minutes</a>'.encode())
        calls = []
        def fetch(url, _hosts, **_kwargs):
            calls.append(url)
            data = pages.get(url, b'')
            meta = receipt(url, data)
            meta.update(content_type='text/html', status=200 if url in pages else 404)
            return [(meta, data)]
        robots = receipt(home + 'robots.txt', b'')
        with patch.object(module, 'get_robots', return_value=(None, [(robots, b'')])), \
             patch.object(module, 'get', side_effect=fetch):
            result, _ = module.discover_one('us-co-county-08051', 'Fixture', home, module.DownloadBudget(30_000_000))
        self.assertEqual({item['url'].split('/')[-1] for item in result['documents']},
                         {f'{i}-0-council-minutes.pdf' for i in range(5)})
        self.assertLessEqual(len(calls), module.MAX_DISCOVERY_PAGES)

    def test_word_extraction_rejects_entities_and_ignores_deleted_text(self):
        xml = (b'<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
               b'<w:body><w:p><w:r><w:t>Flock Safety</w:t></w:r><w:del><w:r><w:t> deleted</w:t></w:r></w:del>'
               b'</w:p></w:body></w:document>')
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'meeting.docx'
            path.write_bytes(word_fixture(xml))
            self.assertEqual(docx_module.extract(path), b'Flock Safety\n')
            deleted = (b'<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
                       b'<w:body><w:del><w:p><w:r><w:t>Flock Safety</w:t></w:r></w:p></w:del>'
                       b'<w:p><w:r><w:t>Budget hearing</w:t></w:r></w:p></w:body></w:document>')
            path.write_bytes(word_fixture(deleted))
            self.assertEqual(docx_module.extract(path), b'Budget hearing\n')
            path.write_bytes(word_fixture(b'<!DOCTYPE x [<!ENTITY x "bad">]><x>&x;</x>'))
            with self.assertRaises(ValueError): docx_module.extract(path)
            path.write_bytes(word_fixture(b'<x>' + b'A' * 2_000_000 + b'</x>'))
            with self.assertRaises(ValueError): docx_module.extract(path)

    def test_word_screening_binds_source_and_runs_without_network_in_sandbox(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref, index = 'us-co-county-08051', 'https://example.org/agenda'
                url = 'https://example.org/AgendaCenter/ViewFile/Minutes/_09152026-1206'
                page, document = b'<a href="/AgendaCenter/ViewFile/Minutes/_09152026-1206">Minutes</a>', b'PK\x03\x04fixture'
                page_obs, document_obs = receipt(index, page), receipt(url, document)
                module.archive(root, [(page_obs, page), (document_obs, document)])
                candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0, 'label': 'Minutes'}
                response = SimpleNamespace(returncode=0, stdout=b'Flock Safety discussion\n', stderr=b'')
                with patch.object(module.shutil, 'which', side_effect=lambda name, **_: '/usr/bin/' + name), \
                     patch.object(module.subprocess, 'run', return_value=response) as run:
                    result = module.extract_saved_docx(root, ref, candidate, document_obs)
                self.assertEqual((result['state'], result['document_format']), ('matched', 'docx'))
                self.assertTrue(result['docx_deleted_text_excluded'])
                self.assertIn('--unshare-net', run.call_args.args[0])
                self.assertEqual(result['finding']['citations'][0]['quote'], 'Flock Safety discussion')
                mixed = SimpleNamespace(returncode=0, stdout=b'The Flock art exhibit\nFlock cameras discussion\n', stderr=b'')
                with patch.object(module.shutil, 'which', side_effect=lambda name, **_: '/usr/bin/' + name), \
                     patch.object(module.subprocess, 'run', return_value=mixed):
                    stronger = module.extract_saved_docx(root, ref, candidate, document_obs)
                self.assertEqual(stronger['finding']['citations'][0]['quote'], 'Flock cameras discussion')
                old_result = {key: value for key, value in result.items() if key != 'docx_deleted_text_excluded'}
                with db:
                    db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (ref, 'https://example.org/', module.utcnow(), '{}'))
                    db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,attempts,result_json,updated_at)
                                  VALUES (?,?,?,?,?,?,?)''', (ref, url, json.dumps(candidate), 'matched', 0, json.dumps(old_result), module.utcnow()))
                output = root / 'public.json'
                module.export_report(root, db, output)
                self.assertEqual(json.loads(output.read_text())['records'], [])
                with patch.object(module, 'extract_saved_docx', return_value=result) as extract:
                    module.docx_saved_documents(root, db, 1, 1, revalidate=True)
                self.assertEqual(extract.call_args.args[3]['sha256'], module.sha(document))
                self.assertEqual(db.execute('SELECT count(*) FROM docx_history').fetchone()[0], 1)
                module.export_report(root, db, output)
                self.assertEqual(json.loads(output.read_text())['records'][0]['document_format'], 'docx')
            finally:
                db.close()

    def test_public_excerpt_does_not_repeat_private_contact_context(self):
        title = module.reference_title('Resident Jane Doe discussed Flock at 123 Main Street', ['Flock'])
        self.assertNotIn('Jane Doe', title)
        self.assertNotIn('123 Main', title)
        self.assertIn('Flock', title)
        self.assertNotIn('123 Main', module.public_text('Contract at 123 Main Street'))
        self.assertNotIn('jane@example.org', module.public_text('Email jane@example.org'))
        self.assertNotIn('Example', module.reference_title('Example v. Flock Group, Inc.', ['Flock Safety']))
        lines = ['Citizen Comments', 'Jane Example suggested facial recognition.',
                 'Approval of the previous minutes', 'A camera policy was considered.']
        self.assertTrue(module.public_comment_context(lines, 2))
        self.assertFalse(module.public_comment_context(lines, 4))
        title = module.reference_title(lines[1], ['Facial recognition'], module.public_comment_context(lines, 2))
        self.assertNotIn('Jane Example', title)
        self.assertIn('Facial recognition', title)

    def test_document_signal_labels_must_occur_at_the_cited_line(self):
        ids = ['vendor.flock-safety', 'vendor.axon']
        self.assertEqual(module.labels_at_quote(ids, 'Flock Safety contract discussion'), ['Flock Safety'])
        self.assertEqual(module.labels_at_quote(ids, 'Axon body cameras'), ['Axon'])
        self.assertEqual(module.labels_at_quote(ids, 'A flock of birds flew overhead'), [])
        self.assertEqual(module.labels_at_quote(['vendor.soundthinking-shotspotter'], 'ShotSpotter project'), ['ShotSpotter'])
        self.assertEqual(module.labels_at_quote(['vendor.motorola-solutions'], 'Motorola Solutions radio equipment'), [])
        self.assertEqual(module.labels_at_quote(['vendor.motorola-solutions'], 'Motorola Solutions license plate cameras'), ['Motorola Solutions'])

    def test_document_highlights_show_distinct_formal_passages_and_redact_public_comments(self):
        lines = ['RESOLUTION DISCONTINUING SERVICES WITH FLOCK GROUP, INC.'] + [''] * 23
        lines += ['Flock Safety was discussed.'] + [''] * 23
        lines += ['Public Comments', 'Resident Jane Doe opposed the Flock contract at 123 Main Street.']
        found = module.document_highlights(lines, 25, ['vendor.flock-safety'])
        self.assertEqual(len(found), 2)
        self.assertEqual(found[0]['locator'], 'extracted line 1')
        self.assertIn('DISCONTINUING', found[0]['title'])
        self.assertEqual(found[1]['locator'], 'extracted line 50')
        self.assertNotIn('Jane Doe', found[1]['title'])
        self.assertNotIn('123 Main', found[1]['title'])
        self.assertEqual(found[1]['title'], 'Possible Flock (identity not established) reference in a public record.')

    def test_packet_roster_attaches_only_same_line_present_officials_in_the_same_meeting(self):
        lines = ['TOWN COUNCIL', 'MINUTES', 'TOWN COUNCIL', 'ROLL CALL', 'Present',
                 'Mayor Pro Tem Don Post (Online)', 'Councilmember Lisa Buesgens', 'Absent',
                 'Councilmember Pat Wrong', 'Town Staff Present'] + [''] * 25
        lines += ['Mayor Pro Tem Post discussed Flock cameras.', 'Councilmember Buesgens moved an unrelated item.']
        number = len(lines) - 1
        found = module.officials_near_reference(lines, number)
        self.assertEqual([item['name'] for item in found], ['Don Post'])
        self.assertEqual(found[0]['individual_vote'], 'not_established')
        self.assertEqual(found[0]['role_in_source'], 'Mayor Pro Tem')
        absent = lines.copy()
        absent[number - 1] = 'Councilmember Wrong discussed Flock cameras.'
        self.assertEqual(module.officials_near_reference(absent, number), [])
        crossed = lines.copy()
        crossed[number - 3] = 'ADJOURNMENT'
        self.assertEqual(module.officials_near_reference(crossed, number), [])
        ambiguous = lines.copy()
        ambiguous.insert(6, 'Councilmember Dana Post')
        self.assertEqual(module.officials_near_reference(ambiguous, number + 1), [])

    def test_ambiguous_flock_name_does_not_become_a_vendor_or_an_artwork_claim(self):
        rule = ['vendor.flock-safety']
        self.assertEqual(module.labels_at_quote(rule, '3. The Flock 20 feet $65,000'), ['Flock (identity not established)'])
        self.assertEqual(module.labels_at_quote(rule, 'Trustee discussed Flock cameras'), ['Flock Safety'])
        self.assertEqual(module.labels_at_quote(rule, 'Written notice; Flock Group, Inc.'), ['Flock Safety'])
        art = ['WTC Roundabout Submissions', 'Artist - Jeff Reed', '3. The Flock 20 feet $65,000']
        self.assertTrue(module.non_surveillance_flock_context(art, 3, ['Flock (identity not established)']))
        self.assertFalse(module.non_surveillance_flock_context(['privacy legislation and FLOCK Bill'], 1,
                                                              ['Flock (identity not established)']))
        self.assertFalse(module.non_surveillance_flock_context(['Artist discussed Flock cameras'], 1, ['Flock Safety']))
        mixed = ['Municipal flag artist', 'Unrelated agenda item', 'Another topic', 'Another topic',
                 'Another topic', 'Resident spoke about flock', '208 cameras on the road']
        self.assertFalse(module.non_surveillance_flock_context(mixed, 6, ['Flock (identity not established)']))

    def test_robots_404_is_cached_within_a_single_bounded_run(self):
        cache = {}
        with patch.object(module, 'get_robots', return_value=(None, [('receipt', b'')])) as fetch:
            self.assertEqual(module.cached_robots('example.org', cache), (None, [('receipt', b'')]))
            self.assertEqual(module.cached_robots('example.org', cache), (None, []))
            self.assertEqual(fetch.call_count, 1)

    def test_document_robot_denial_keeps_its_observation(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                index = 'https://example.org/agenda'
                url = 'https://example.org/AgendaCenter/ViewFile/Minutes/_09152026-1206'
                body = b'<a href="/AgendaCenter/ViewFile/Minutes/_09152026-1206">Minutes</a>'
                module.archive(root, [(receipt(index, body), body)])
                candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(body), 'anchor_index': 0}
                observed = [('denial receipt', b'forbidden')]
                with patch.object(module, 'cached_robots', side_effect=module.RobotsDenied(403, observed)):
                    result, observations = module.screen_one(root, 'us-co-county-08051', candidate, '/bin/true',
                                                             threading.Lock(), module.DownloadBudget(2_000_000), False, {})
                self.assertEqual(result['state'], 'error')
                self.assertEqual(observations, observed)
            finally:
                db.close()

    def test_fresh_word_document_is_screened_on_initial_and_refresh_fetch(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                index = 'https://example.org/agenda'
                url = 'https://example.org/AgendaCenter/ViewFile/Minutes/_09152026-1206'
                page = b'<a href="/AgendaCenter/ViewFile/Minutes/_09152026-1206">Minutes</a>'
                document = b'PK\x03\x04word-document'
                module.archive(root, [(receipt(index, page), page)])
                candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0}
                obs = receipt(url, document)
                with patch.object(module, 'cached_robots', return_value=(None, [])), \
                     patch.object(module, 'get', return_value=[(obs, document)]), \
                     patch.object(module, 'extract_saved_docx', return_value={'state': 'no_match', 'document_format': 'docx'}) as extract:
                    result, _ = module.screen_one(root, 'us-co-county-08051', candidate, '/bin/true',
                                                  threading.Lock(), module.DownloadBudget(2_000_000), False, {})
                self.assertEqual(result['document_format'], 'docx')
                self.assertEqual(extract.call_args.args[3]['sha256'], module.sha(document))
                self.assertEqual((root / 'content' / module.sha(document)).read_bytes(), document)
            finally:
                db.close()

    def test_saved_ocr_reuses_archived_bytes_and_never_fetches(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref, index = 'us-co-county-08051', 'https://example.org/agenda'
                url = 'https://example.org/AgendaCenter/ViewFile/Minutes/_09152026-1206'
                page = b'<a href="/AgendaCenter/ViewFile/Minutes/_09152026-1206">Minutes</a>'
                pdf = b'%PDF-a-saved-scan'
                page_obs, pdf_obs = receipt(index, page), receipt(url, pdf)
                module.archive(root, [(page_obs, page), (pdf_obs, pdf)])
                candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0}
                db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,attempts,result_json,updated_at)
                              VALUES (?,?,?,?,?,?,?)''', (ref, url, json.dumps(candidate), 'error', 1,
                              json.dumps({'error': 'scanned_pdf_ocr_disabled'}), '2026-11-11T00:00:00+00:00'))
                db.commit()
                with patch.object(module, 'get', side_effect=AssertionError('network should not be used')):
                    with patch.object(module, 'extract_captured_pdf', return_value={'state': 'no_match'}) as extract:
                        module.ocr_saved_documents(root, db, '/bin/true', 1, 1)
                args = extract.call_args.args
                self.assertEqual(args[4]['sha256'], module.sha(pdf))
                self.assertIs(args[5], True)
                self.assertEqual(db.execute('SELECT state FROM document_candidates').fetchone()[0], 'no_match')
                self.assertEqual(db.execute('SELECT status FROM ocr_attempts').fetchone()[0], 'ok')
            finally:
                db.close()

    def test_previous_ocr_limit_is_retried_once_without_hot_looping(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref, index = 'us-co-county-08051', 'https://example.org/agenda'
                url = 'https://example.org/AgendaCenter/ViewFile/Minutes/_09152026-1206'
                page = b'<a href="/AgendaCenter/ViewFile/Minutes/_09152026-1206">Minutes</a>'
                pdf = b'%PDF-a-saved-scan'
                module.archive(root, [(receipt(index, page), page), (receipt(url, pdf), pdf)])
                candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0}
                db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,attempts,result_json,updated_at)
                              VALUES (?,?,?,?,?,?,?)''', (ref, url, json.dumps(candidate), 'error', 1,
                              json.dumps({'error': 'scanned_pdf_ocr_disabled'}), module.utcnow()))
                db.execute('INSERT INTO ocr_attempts VALUES (?,?,?,?)',
                           (ref, url, module.utcnow(), '(ocr_page_limit): OCR is limited to 5 pages'))
                db.commit()
                with patch.object(module, 'get', side_effect=AssertionError('network should not be used')), \
                     patch.object(module, 'extract_captured_pdf', side_effect=ValueError('(ocr_page_limit): OCR is limited to 20 pages')) as extract:
                    module.ocr_saved_documents(root, db, '/bin/true', 1, 1, retry_previous_limit=True)
                    module.ocr_saved_documents(root, db, '/bin/true', 1, 1, retry_previous_limit=True)
                self.assertEqual(extract.call_count, 1)
                self.assertEqual(db.execute('SELECT state FROM document_candidates').fetchone()[0], 'error')
                self.assertIn('20 pages', db.execute('SELECT status FROM ocr_attempts').fetchone()[0])
            finally:
                db.close()

    def test_ocr_worker_without_corrected_page_mapping_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            url = 'https://example.org/agenda.pdf'
            pdf = b'%PDF-scan'
            observed = receipt(url, pdf)
            output = {'state': 'no_match', 'document_sha256': module.sha(pdf),
                      'extraction_status': 'complete_with_ocr', 'ocr_page_set_validated': True}
            execution = SimpleNamespace(returncode=0, stderr=b'', stdout=json.dumps(output).encode())
            with patch.object(module, 'source_blob', return_value=pdf), patch.object(module.subprocess, 'run', return_value=execution):
                with self.assertRaisesRegex(ValueError, 'per-page text mapping'):
                    module.extract_captured_pdf(root, 'us-co-county-08051', {'url': url}, '/bin/true', observed, True)
                output['ocr_page_text_mapping_version'] = 2
                execution.stdout = json.dumps(output).encode()
                self.assertEqual(module.extract_captured_pdf(root, 'us-co-county-08051', {'url': url}, '/bin/true', observed, True)['state'], 'no_match')

    def test_older_ocr_no_match_is_not_counted_until_page_set_is_revalidated(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref, index = 'us-co-county-08051', 'https://example.org/agenda'
                url = 'https://example.org/AgendaCenter/ViewFile/Minutes/_09152026-1206'
                page, pdf = b'<a href="/AgendaCenter/ViewFile/Minutes/_09152026-1206">Minutes</a>', b'%PDF-scan'
                page_obs, pdf_obs = receipt(index, page), receipt(url, pdf)
                module.archive(root, [(page_obs, page), (pdf_obs, pdf)])
                candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0}
                old = {'state': 'no_match', 'extraction_status': 'complete_with_ocr', 'ocr_page_set_validated': True, 'receipt': pdf_obs}
                newer_pdf = b'%PDF-a-newer-version'
                newer_obs = receipt(url, newer_pdf)
                newer_obs['retrieved_at'] = '2026-11-11T00:05:00+00:00'
                module.archive(root, [(newer_obs, newer_pdf)])
                with db:
                    db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (ref, index, module.utcnow(),
                               json.dumps({'jurisdiction_ref': ref, 'state': 'directory_site_checked'})))
                    db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,attempts,result_json,updated_at)
                                  VALUES (?,?,?,?,?,?,?)''', (ref, url, json.dumps(candidate), 'no_match', 0, json.dumps(old), module.utcnow()))
                output = root / 'public.json'
                module.export_report(root, db, output)
                self.assertEqual(json.loads(output.read_text())['coverage']['document_screening'], {'error': 1})
                self.assertEqual(json.loads(output.read_text())['coverage']['ocr_revalidation_pending'], 1)
                db.execute("UPDATE document_candidates SET updated_at='2020-01-01T00:00:00+00:00',last_attempt_at=NULL")
                db.commit()
                with patch.object(module, 'screen_one') as refresh:
                    module.screen_documents(root, db, '/bin/true', 1, 1, 1, refresh_existing=True)
                refresh.assert_not_called()
                result = {'state': 'no_match', 'extraction_status': 'complete_with_ocr', 'ocr_page_set_validated': True,
                          'ocr_page_text_mapping_version': 2}
                with patch.object(module, 'extract_captured_pdf', return_value=result) as extract, patch.object(module, 'get', side_effect=AssertionError('no network')):
                    module.ocr_saved_documents(root, db, '/bin/true', 1, 1, revalidate=True)
                self.assertEqual(extract.call_args.args[4]['sha256'], module.sha(pdf))
                module.export_report(root, db, output)
                self.assertEqual(json.loads(output.read_text())['coverage']['document_screening'], {'no_match': 1})
                self.assertEqual(json.loads(output.read_text())['coverage']['ocr_revalidation_pending'], 0)
                self.assertEqual(db.execute('SELECT count(*) FROM ocr_history').fetchone()[0], 1)
                old['state'] = 'matched'
                db.execute('UPDATE document_candidates SET state=?,result_json=?', ('matched', json.dumps(old)))
                db.commit()
                with patch.object(module, 'extract_captured_pdf', side_effect=ValueError('OCR failed')):
                    module.ocr_saved_documents(root, db, '/bin/true', 1, 1, revalidate=True)
                self.assertEqual(db.execute('SELECT state FROM document_candidates').fetchone()[0], 'error')
                self.assertEqual(db.execute('SELECT count(*) FROM ocr_history').fetchone()[0], 2)
            finally:
                db.close()

    def test_named_context_requires_role_presence_and_nearby_unique_reference(self):
        lines = ['GUNNISON COUNTY BOARD OF COUNTY COMMISSIONERS', 'Elizabeth Smith, Vice-Chairperson',
                 'Jonathan Houck, Commissioner', 'A public agenda', 'Flock Group, Inc',
                 'Moved by Commissioner Smith, seconded by Commissioner Houck', 'Motion carried unanimously.']
        result = module.officials_near_reference(lines, 5)
        self.assertEqual({person['name'] for person in result}, {'Elizabeth Smith', 'Jonathan Houck'})
        self.assertTrue(all(person['individual_vote'] == 'not_established' for person in result))
        self.assertEqual(module.officials_near_reference([lines[0], '(ABSENT) Elizabeth Smith, Vice-Chairperson', *lines[2:]], 5)[0]['name'], 'Jonathan Houck')
        ambiguous = [lines[0], 'Elizabeth Smith, Commissioner', 'Alice Smith, Commissioner', 'Flock', 'Commissioner Smith spoke']
        self.assertEqual(module.officials_near_reference(ambiguous, 4), [])

    def test_portrait_nickname_needs_explicit_official_alias(self):
        self.assertTrue(module.portrait_name_matches('Laura Puckett Daniels', 'Profile picture of Laura Puckett Daniels'))
        self.assertTrue(module.portrait_name_matches('Alex Example', 'Councilmember Alex Example'))
        self.assertFalse(module.portrait_name_matches('Alex Example', 'Councilmember Other Example'))
        self.assertFalse(module.portrait_name_matches('Elizabeth Smith', 'Profile picture of Liz Smith'))
        self.assertTrue(module.portrait_name_matches('Elizabeth Smith', 'Profile picture of Liz Smith', 'Elizabeth (Liz) Smith, Commissioner'))
        self.assertFalse(module.portrait_name_matches('Elizabeth Smith', 'Profile picture of Liz Jones', 'Elizabeth (Liz) Smith, Commissioner'))

    def test_portrait_navigation_is_bounded_and_stays_on_the_official_host(self):
        page = 'https://town.example.org/'
        body = (b'<a href="https://outside.example/council">Council</a>'
                b'<a href="/news">News</a>' + b''.join(
                    f'<a href="/council-{i}">Council {i}</a>'.encode() for i in range(7)))
        self.assertEqual(module.portrait_navigation(body, page),
                         [page + f'council-{i}' for i in range(4)])
        foreign_base = b'<base href="https://outside.example/"><a href="council">Council</a>'
        self.assertEqual(module.portrait_navigation(foreign_base, page), [])

    def test_portrait_collection_requires_named_unique_same_host_source_and_original_bytes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref, name = 'us-co-county-08051', 'Alex Example'
                page = 'https://town.example.org/council'
                image_url = 'https://town.example.org/portraits/alex.jpg'
                body = b'<img alt="Councilmember Alex Example" src="/portraits/alex.jpg">'
                jpeg = b'\xff\xd8\xfforiginal'
                module.archive(root, [(receipt(page, body), body)])
                with db:
                    db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (ref, 'https://town.example.org/', module.utcnow(), '{}'))
                people = {name}
                with patch.object(module, 'get', return_value=[(receipt(image_url, jpeg), jpeg)]) as fetch:
                    count = module.collect_labeled_portraits(root, db, ref, people, 'town.example.org', None,
                                                              module.DownloadBudget(10_000), page, body)
                self.assertEqual(count, 1)
                self.assertEqual(people, set())
                fetch.assert_called_once()
                result = module.checked_portrait(root, db, module.observation_keys(root), ref, name, root / 'assets')
                self.assertEqual(result['sha256'], module.sha(jpeg))
                self.assertEqual((root / 'assets' / (module.sha(jpeg) + '.jpg')).read_bytes(), jpeg)
                for unsafe in [
                    body + body,
                    b'<base href="https://outside.example/"><img alt="Councilmember Alex Example" src="alex.jpg">',
                    b'<img alt="Councilmember Alex Example" src="https://outside.example/alex.jpg">',
                    b'<img alt="Councilmember Other Example" src="/portraits/alex.jpg">',
                ]:
                    with patch.object(module, 'get') as rejected:
                        self.assertEqual(module.collect_labeled_portraits(root, db, ref, {name}, 'town.example.org',
                                                                         None, module.DownloadBudget(10_000), page, unsafe), 0)
                    rejected.assert_not_called()
            finally:
                db.close()

    def test_budget_portrait_requires_source_receipt_and_exact_portal_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            proof = json.loads((ROOT / 'fixtures/atlas/portrait-sources/arapahoe-2022-budget-proof.json').read_text())
            receipt_key = (proof['source_url'], proof['source_sha256'])
            vote = {'name': 'Carrie Warren-Gully', 'person_id': 513}
            with patch.object(module, 'source_blob', return_value=b'%PDF-fixture'):
                portrait = module.arapahoe_budget_portrait(root, {receipt_key}, 'us-co-county-08005', 'arapahoe', vote, root / 'assets')
                self.assertEqual(portrait['source_kind'], 'embedded_pdf')
                self.assertEqual(portrait['source_document_sha256'], proof['source_sha256'])
                self.assertEqual(portrait['embedded_image_index'], 6)
                self.assertIsNone(module.arapahoe_budget_portrait(root, {receipt_key}, 'us-co-county-08005', 'wrong', vote, root / 'assets'))
                self.assertIsNone(module.arapahoe_budget_portrait(root, {receipt_key}, 'us-co-county-08005', 'arapahoe', {**vote, 'person_id': 999}, root / 'assets'))
                with self.assertRaises(ValueError):
                    module.arapahoe_budget_portrait(root, set(), 'us-co-county-08005', 'arapahoe', vote, root / 'assets')

    def test_littleton_portrait_requires_source_receipt_and_exact_portal_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            proof = json.loads((ROOT / 'fixtures/atlas/portrait-sources/littleton-2023-report-proof.json').read_text())
            key = (proof['source_url'], proof['source_sha256'])
            vote = {'name': 'Kyle Schlachter', 'person_id': 333}
            with patch.object(module, 'source_blob', return_value=b'%PDF-fixture'):
                result = module.littleton_report_portrait(root, {key}, 'us-co-place-0845255', 'littletongov', vote, root / 'assets')
                self.assertEqual((result['source_page'], result['embedded_image_index']), (3, 4))
                self.assertEqual(result['image_transform'], 'lossless_png_from_jpeg2000')
                self.assertEqual(result['embedded_original_sha256'], proof['portraits'][0]['original_sha256'])
                self.assertIsNone(module.littleton_report_portrait(root, {key}, 'us-co-place-0845255', 'wrong', vote, root / 'assets'))
                self.assertIsNone(module.littleton_report_portrait(root, {key}, 'us-co-place-0845255', 'littletongov', {**vote, 'person_id': 999}, root / 'assets'))
                with self.assertRaises(ValueError):
                    module.littleton_report_portrait(root, set(), 'us-co-place-0845255', 'littletongov', vote, root / 'assets')

    def test_littleton_2024_portraits_require_the_later_city_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            proof = json.loads((ROOT / 'fixtures/atlas/portrait-sources/littleton-2024-report-proof.json').read_text())
            key = (proof['source_url'], proof['source_sha256'])
            vote = {'name': 'Andrea Peters', 'person_id': 503}
            with patch.object(module, 'source_blob', return_value=b'%PDF-fixture'):
                result = module.littleton_report_portrait(root, {key}, 'us-co-place-0845255', 'littletongov', vote, root / 'assets')
                self.assertEqual((result['source_page'], result['embedded_image_index']), (3, 2))
                self.assertTrue(result['src'].endswith('.jpg'))
                self.assertNotIn('image_transform', result)
                self.assertIsNone(module.littleton_report_portrait(root, {key}, 'us-co-place-0845255', 'littletongov', {**vote, 'name': 'Robert Reichardt'}, root / 'assets'))
                with self.assertRaises(ValueError):
                    module.littleton_report_portrait(root, set(), 'us-co-place-0845255', 'littletongov', vote, root / 'assets')

    def test_commerce_portrait_requires_exact_voter_and_archived_city_report(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            proof = json.loads((ROOT / 'fixtures/atlas/portrait-sources/commerce-city-2022-report-proof.json').read_text())
            key = (proof['source_url'], proof['source_sha256'])
            vote = {'name': 'Benjamin Huseman', 'person_id': 402}
            with patch.object(module, 'source_blob', return_value=b'%PDF-fixture'):
                result = module.commerce_report_portrait(root, {key}, 'us-co-place-0816495', 'commerce', vote, root / 'assets')
                self.assertEqual((result['source_page'], result['embedded_image_index']), (2, 0))
                self.assertEqual(result['source_document_sha256'], proof['source_sha256'])
                self.assertIsNone(module.commerce_report_portrait(root, {key}, 'us-co-place-0816495', 'wrong', vote, root / 'assets'))
                self.assertIsNone(module.commerce_report_portrait(root, {key}, 'us-co-place-0816495', 'commerce', {**vote, 'person_id': 403}, root / 'assets'))
                with self.assertRaises(ValueError):
                    module.commerce_report_portrait(root, set(), 'us-co-place-0816495', 'commerce', vote, root / 'assets')

    def test_denver_budget_portrait_requires_source_receipt_and_exact_portal_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            proof = json.loads((ROOT / 'fixtures/atlas/portrait-sources/denver-2019-budget-proof.json').read_text())
            key = (proof['source_url'], proof['source_sha256'])
            vote = {'name': 'Paul López', 'person_id': 267}
            with patch.object(module, 'source_blob', return_value=b'%PDF-fixture'):
                result = module.denver_budget_portrait(root, {key}, 'us-co-place-0820000', 'denver', vote, root / 'assets')
                self.assertEqual((result['source_page'], result['embedded_image_index']), (6, 4))
                self.assertIn('Paul D. López', result['caption'])
                self.assertIsNone(module.denver_budget_portrait(root, {key}, 'us-co-place-0820000', 'other', vote, root / 'assets'))
                self.assertIsNone(module.denver_budget_portrait(root, {key}, 'us-co-place-0820000', 'denver', {**vote, 'person_id': 280}, root / 'assets'))
                with self.assertRaises(ValueError):
                    module.denver_budget_portrait(root, set(), 'us-co-place-0820000', 'denver', vote, root / 'assets')
            alias = ('<title>Meet Chris — Chris Herndon</title>'
                     '<p>Served on Denver City Council for 12 years, with four years as Council President and Vice President</p>'
                     '<p>For Councilman Christopher Herndon, his public service career started long before the Denver City Council.</p>').encode()
            alias_key = (proof['alias_source_url'], proof['alias_source_sha256'])
            chris = {'name': 'Chris Herndon', 'person_id': 282}
            with patch.object(module, 'source_blob', side_effect=lambda _root, value, *_args: alias if value == alias_key[1] else b'%PDF-fixture'):
                result = module.denver_budget_portrait(root, {key, alias_key}, 'us-co-place-0820000', 'denver', chris, root / 'assets')
                self.assertEqual(result['identity_source_url'], alias_key[0])
                with self.assertRaises(ValueError):
                    module.denver_budget_portrait(root, {key}, 'us-co-place-0820000', 'denver', chris, root / 'assets')

    def test_denver_2020_portrait_requires_source_and_exact_voter(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            proof = json.loads((ROOT / 'fixtures/atlas/portrait-sources/denver-2020-budget-proof.json').read_text())
            key = (proof['source_url'], proof['source_sha256'])
            vote = {'name': 'Candi CdeBaca', 'person_id': 472}
            with patch.object(module, 'source_blob', return_value=b'%PDF-fixture'):
                result = module.denver_2020_budget_portrait(root, {key}, 'us-co-place-0820000', 'denver', vote, root / 'assets')
                self.assertEqual((result['source_page'], result['embedded_image_index']), (6, 14))
                self.assertEqual(result['embedded_original_sha256'], proof['portrait']['original_sha256'])
                self.assertIsNone(module.denver_2020_budget_portrait(root, {key}, 'us-co-place-0820000', 'denver', {**vote, 'person_id': 999}, root / 'assets'))
                with self.assertRaises(ValueError):
                    module.denver_2020_budget_portrait(root, set(), 'us-co-place-0820000', 'denver', vote, root / 'assets')

    def test_commissioner_profile_image_requires_exact_heading_caption_and_official_host(self):
        source = (b'<h1>Commissioner Abe Laydon, District I</h1>'
                  b'<img alt="Commissioner Abe Laydon" srcset="https://www.douglasco.gov/a-small.jpg 480w, '
                  b'https://www.douglasco.gov/a-original.jpg 2048w">')
        self.assertEqual(module.official_profile_image(source, 'Abe Laydon'),
                         ('https://www.douglasco.gov/a-original.jpg', 'Commissioner Abe Laydon'))
        self.assertIsNone(module.official_profile_image(source, 'George Teal'))
        self.assertIsNone(module.official_profile_image(source.replace(b'Commissioner Abe Laydon, District I', b'Other Person'), 'Abe Laydon'))
        self.assertIsNone(module.official_profile_image(source.replace(b'www.douglasco.gov', b'evil.example'), 'Abe Laydon'))

    def test_douglas_gallery_requires_an_exact_name_and_image_in_one_card(self):
        image = 'https://www.douglasco.gov/wp-content/uploads/2026/06/Thomas.png'
        card = (f'<div class="gallery-item"><div><img data-lazy-srcset="{image} 2000w"></div>'
                '<div>Commissioner Lora Thomas &#8211; 2017</div></div>').encode()
        expected = (image, 'Commissioner Lora Thomas – 2017')
        self.assertEqual(module.douglas_gallery_image(card, 'Lora Thomas'), expected)
        self.assertIsNone(module.douglas_gallery_image(card, 'George Teal'))
        self.assertIsNone(module.douglas_gallery_image(card.replace(b'www.douglasco.gov', b'evil.example'), 'Lora Thomas'))
        self.assertIsNone(module.douglas_gallery_image(card + card, 'Lora Thomas'))
        self.assertIsNone(module.douglas_gallery_image(card.replace(b'Commissioner Lora Thomas', b'Commissioner Lora Thomas Junior'), 'Lora Thomas'))

    def test_arapahoe_fields_portrait_requires_exact_profile_and_explicit_policy(self):
        image_url = 'https://files.arapahoeco.gov/Board%20of%20County%20Commissioners/Rhonda%20Fields.png?t=202603181434200'
        body = (b'<base href="https://files.arapahoeco.gov/">'
                b'<h1>Commissioner Rhonda Fields</h1><div class="freeform-contact-us"><div class="freeform-contact-name">Rhonda Fields</div>'
                b'<div class="freeform-contact-title">Commissioner, District 5</div>'
                b'<img alt="image" src="Board%20of%20County%20Commissioners/Rhonda%20Fields.png?t=202603181434200"></div>')
        image = b'\x89PNG\r\n\x1a\nfixture'
        self.assertEqual(module.arapahoe_fields_image(body)[0], image_url)
        self.assertIsNone(module.arapahoe_fields_image(body.replace(b'Commissioner, District 5', b'Commissioner, District 4')))
        self.assertIsNone(module.arapahoe_fields_image(body.replace(b'Rhonda%20Fields.png', b'Other.png')))
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            denied = SimpleNamespace(can_fetch=lambda *_: False)
            def fetch(url, _hosts, **_kwargs):
                content = body if url == module.ARAPAHOE_FIELDS_PROFILE else image
                obs = receipt(url, content)
                obs['content_type'] = 'text/html' if url == module.ARAPAHOE_FIELDS_PROFILE else 'image/png'
                return [(obs, content)]
            try:
                with db:
                    db.execute('insert into discovery values (?,?,?,?)',
                               ('us-co-county-08005', 'https://www.arapahoeco.gov/', module.utcnow(), '{}'))
                with patch.object(module, 'get_robots', return_value=(denied, [])), \
                     patch.object(module, 'get', side_effect=fetch) as request, \
                     patch.dict(module.os.environ, {'PNUL_ALLOW_ARAPAHOE_PROFILE': '0'}):
                    self.assertEqual(module.collect_arapahoe_fields(root, db, {'Rhonda Fields'}, module.DownloadBudget(10_000_000)), 0)
                    request.assert_not_called()
                with patch.object(module, 'get_robots', return_value=(denied, [])), \
                     patch.object(module, 'get', side_effect=fetch), \
                     patch.dict(module.os.environ, {'PNUL_ALLOW_ARAPAHOE_PROFILE': '1'}):
                    self.assertEqual(module.collect_arapahoe_fields(root, db, {'Rhonda Fields'}, module.DownloadBudget(10_000_000)), 1)
                portrait = module.checked_portrait(root, db, module.observation_keys(root), 'us-co-county-08005', 'Rhonda Fields', root / 'assets')
                self.assertEqual(portrait['sha256'], module.sha(image))
                self.assertIsNone(module.checked_portrait(root, db, module.observation_keys(root), 'us-co-county-08005', 'Other Name', root / 'assets'))
            finally:
                db.close()

    def test_adams_portrait_requires_the_named_authority_vote_and_official_profile(self):
        image_url = 'https://adamscountyco.gov/wp-content/uploads/2025/05/image-23.png'
        body = (b'<h1>Commissioner Kathy Henson, District 2</h1>'
                b'<p>Commerce City Urban Renewal Authority</p>'
                b'<div data-component="card_contact"><img src="https://adamscountyco.gov/wp-content/uploads/2025/05/image-23.png">'
                b'<h3>Kathy Henson</h3></div>')
        image = b'RIFF1234WEBPfixture'
        self.assertEqual(module.adams_henson_image(body)[0], image_url)
        self.assertIsNone(module.adams_henson_image(body.replace(b'Commerce City Urban Renewal Authority', b'Other Agency')))
        self.assertIsNone(module.adams_henson_image(body.replace(b'<h3>Kathy Henson</h3>', b'<h3>Other Person</h3>')))
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            page_obs, image_obs = receipt(module.ADAMS_HENSON_PROFILE, body), receipt(image_url, image)
            page_obs['content_type'] = 'text/html'
            try:
                with db:
                    db.execute('insert into discovery values (?,?,?,?)',
                               ('us-co-county-08001', 'https://adamscountyco.gov/', module.utcnow(), '{}'))
                with patch.object(module, 'get_robots', return_value=(None, [])), \
                     patch.object(module, 'html_page', return_value=(body, [(page_obs, body)])), \
                     patch.object(module, 'get', return_value=[(image_obs, image)]):
                    self.assertEqual(module.collect_adams_henson(root, db, {'Kathy Henson'}, module.DownloadBudget(10_000_000)), 1)
                receipts = module.observation_keys(root)
                vote = {'name': 'Kathy Henson', 'person_id': 796}
                motion = {'body': 'Urban Renewal Authority'}
                result = module.authority_external_portrait(root, db, receipts, 'us-co-place-0816495', 'commerce', motion, vote, root / 'assets')
                self.assertEqual(result['sha256'], module.sha(image))
                self.assertIsNone(module.authority_external_portrait(root, db, receipts, 'us-co-place-0816495', 'commerce', {'body': 'City Council'}, vote, root / 'assets'))
                self.assertIsNone(module.authority_external_portrait(root, db, receipts, 'us-co-place-0816495', 'commerce', motion, {**vote, 'person_id': 1}, root / 'assets'))
            finally:
                db.close()

    def test_school_board_member_portrait_requires_the_exact_authority_and_portal_person(self):
        image_url = 'https://www.adams14.org/uploaded/faculty/James_Amador_Headshot_Adams_14_260x320.png'
        body = (b'<div class="fsConstituentProfile"><img src="/uploaded/faculty/James_Amador_Headshot_Adams_14_260x320.png">'
                b'<h3 class="fsFullName">James Amador</h3><div>Board of Education Treasurer</div>'
                b'<div>Commerce City Urban Renewal Authority (CCURA)</div></div>')
        image = b'\x89PNG\r\n\x1a\nfixture'
        self.assertEqual(module.adams14_amador_image(body)[0], image_url)
        self.assertIsNone(module.adams14_amador_image(body.replace(b'Urban Renewal Authority', b'Other Agency')))
        self.assertIsNone(module.adams14_amador_image(body.replace(b'James Amador</h3>', b'Other Person</h3>')))
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            page_obs, image_obs = receipt(module.ADAMS14_BOARD_PROFILE, body), receipt(image_url, image)
            page_obs['content_type'] = 'text/html'
            try:
                with db:
                    db.execute('insert into discovery values (?,?,?,?)',
                               ('us-co-school-0801950', 'https://www.adams14.org/', module.utcnow(), '{}'))
                with patch.object(module, 'get_robots', return_value=(None, [])), \
                     patch.object(module, 'html_page', return_value=(body, [(page_obs, body)])), \
                     patch.object(module, 'get', return_value=[(image_obs, image)]):
                    self.assertEqual(module.collect_adams14_amador(root, db, {'James Amador'}, module.DownloadBudget(10_000_000)), 1)
                receipts = module.observation_keys(root)
                vote = {'name': 'James Amador', 'person_id': 668}
                motion = {'body': 'Urban Renewal Authority'}
                portrait = module.authority_external_portrait(root, db, receipts, 'us-co-place-0816495', 'commerce', motion, vote, root / 'assets')
                self.assertEqual(portrait['sha256'], module.sha(image))
                self.assertIsNone(module.authority_external_portrait(root, db, receipts, 'us-co-place-0816495', 'commerce', {'body': 'City Council'}, vote, root / 'assets'))
                self.assertIsNone(module.authority_external_portrait(root, db, receipts, 'us-co-place-0816495', 'commerce', motion, {**vote, 'person_id': 669}, root / 'assets'))
            finally:
                db.close()

    def test_douglas_gallery_portrait_rechecks_source_card_and_original_bytes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            ref, name, home = 'us-co-county-08035', 'Lora Thomas', 'https://www.douglasco.gov/'
            image_url = 'https://www.douglasco.gov/wp-content/uploads/2026/06/Thomas.png'
            page = (f'<div class="gallery-item"><img srcset="{image_url} 2000w">'
                    '<p>Commissioner Lora Thomas &#8211; 2017</p></div>').encode()
            image = b'\x89PNG\r\n\x1a\nfixture'
            page_obs, image_obs = receipt(module.DOUGLAS_GALLERY, page), receipt(image_url, image)
            module.archive(root, [(page_obs, page), (image_obs, image)])
            info = {'kind': 'commissioner_gallery', 'source_url': module.DOUGLAS_GALLERY,
                    'source_sha256': module.sha(page), 'image_url': image_url, 'image_sha256': module.sha(image),
                    'caption': 'Commissioner Lora Thomas – 2017', 'extension': 'png', 'image_receipt': image_obs}
            try:
                with db:
                    db.execute('insert into discovery values (?,?,?,?)', (ref, home, module.utcnow(), '{}'))
                    db.execute('insert into official_portraits values (?,?,?)', (ref, name, json.dumps(info)))
                portrait = module.checked_portrait(root, db, module.observation_keys(root), ref, name, root / 'assets')
                self.assertEqual(portrait['sha256'], module.sha(image))
                info['caption'] = 'Commissioner George Teal – 2020'
                with db:
                    db.execute('update official_portraits set portrait_json=?', (json.dumps(info),))
                with self.assertRaises(ValueError):
                    module.checked_portrait(root, db, module.observation_keys(root), ref, name, root / 'assets')
            finally:
                db.close()

    def test_profile_portrait_is_bound_to_roster_profile_and_original_bytes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref, name = 'us-co-county-08035', 'Abe Laydon'
                board_url = 'https://www.douglasco.gov/board-county-commissioners/'
                profile_url = board_url + 'commissioner-abe-laydon-district-i/'
                image_url = 'https://www.douglasco.gov/original.jpg'
                board = f'<a href="{profile_url}">Commissioner Abe Laydon, District I</a>'.encode()
                profile = (f'<h1>Commissioner Abe Laydon, District I</h1><img alt="Commissioner Abe Laydon" '
                           f'srcset="{image_url} 2048w">').encode()
                image = b'\xff\xd8\xfforiginal-jpeg'
                observations = [(receipt(url, content), content) for url, content in
                                [(board_url, board), (profile_url, profile), (image_url, image)]]
                module.archive(root, observations)
                info = {'kind': 'commissioner_profile', 'board_url': board_url, 'board_sha256': module.sha(board),
                        'board_anchor': 0, 'source_url': profile_url, 'source_sha256': module.sha(profile),
                        'image_url': image_url, 'image_sha256': module.sha(image), 'caption': 'Commissioner Abe Laydon',
                        'extension': 'jpg', 'image_receipt': observations[-1][0]}
                with db:
                    db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (ref, 'https://www.douglasco.gov/', module.utcnow(), '{}'))
                    db.execute('INSERT INTO official_portraits VALUES (?,?,?)', (ref, name, json.dumps(info)))
                result = module.checked_portrait(root, db, module.observation_keys(root), ref, name, root / 'assets')
                self.assertEqual(result['sha256'], module.sha(image))
                self.assertEqual((root / 'assets' / (module.sha(image) + '.jpg')).read_bytes(), image)
                info['board_anchor'] = 1
                db.execute('UPDATE official_portraits SET portrait_json=?', (json.dumps(info),))
                with self.assertRaises(ValueError):
                    module.checked_portrait(root, db, module.observation_keys(root), ref, name, root / 'assets')
            finally:
                db.close()

    def test_recorded_votes_require_matching_matter_and_consent_scope(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                client, matter, event, action, block = 'example', 33, 11, 22, 44
                history = {'MatterHistoryId': action, 'MatterHistoryEventId': event,
                           'MatterHistoryActionName': 'approved', 'MatterHistoryPassedFlag': 1,
                           'MatterHistoryPassedFlagName': 'Pass', 'MatterHistoryConsent': 1,
                           'MatterHistoryActionDate': '2026-01-15T00:00:00', 'MatterHistoryActionBodyName': 'Council'}
                item = {'EventItemId': action, 'EventItemEventId': event, 'EventItemMatterId': matter,
                        'EventItemActionName': 'approved', 'EventItemPassedFlag': 1, 'EventItemConsent': 1}
                vote = {'VoteId': 8, 'VotePersonId': 9, 'VotePersonName': 'Alex Example',
                        'VoteValueName': 'Aye', 'VoteEventItemId': block}
                consent = {'EventItemId': block, 'EventItemEventId': event, 'EventItemMatterId': None,
                           'EventItemTitle': 'Approval of Consent Agenda'}

                def capture(kind, first, second, value):
                    url = module.action_url(client, kind, first, second)
                    body = json.dumps(value).encode()
                    obs = receipt(url, body)
                    module.archive(root, [(obs, body)])
                    return obs

                captures = {'history': capture('histories', matter, None, [history]), 'history_index': 0,
                            'item': capture('item', event, action, item),
                            'votes': capture('votes', action, None, [vote]),
                            'block': capture('item', event, block, consent)}
                result = module.materialize_action(root, module.observation_keys(root), client, matter, action, json.dumps(captures))
                self.assertEqual(result['vote_scope'], 'consent_block')
                self.assertEqual(result['votes'][0]['name'], 'Alex Example')
                self.assertEqual(result['votes'][0]['value'], 'Aye')
                self.assertEqual(result['votes'][0]['locator'], '$[0]')

                # A different meeting or a different item cannot supply an
                # individual vote for the matter under inspection.
                captures['block'] = capture('item', event, block, {**consent, 'EventItemEventId': 999})
                result = module.materialize_action(root, module.observation_keys(root), client, matter, action, json.dumps(captures))
                self.assertEqual(result['vote_scope'], 'not_established')
                self.assertEqual(result['votes'], [])
                captures['item'] = capture('item', event, action, {**item, 'EventItemMatterId': 999})
                with self.assertRaises(ValueError):
                    module.materialize_action(root, module.observation_keys(root), client, matter, action, json.dumps(captures))

                history.update(MatterHistoryConsent=0, MatterHistoryPassedFlag=0, MatterHistoryPassedFlagName='Fail')
                item.update(EventItemConsent=0, EventItemPassedFlag=0)
                vote.update(VoteEventItemId=action, VoteValueName='No')
                captures.update(history=capture('histories', matter, None, [history]),
                                item=capture('item', event, action, item),
                                votes=capture('votes', action, None, [vote]), block=None)
                result = module.materialize_action(root, module.observation_keys(root), client, matter, action, json.dumps(captures))
                self.assertEqual((result['outcome'], result['vote_scope'], result['votes'][0]['value']), ('Fail', 'item', 'No'))
            finally:
                db.close()

    def test_agenda_portal_requires_county_and_calendar_link_proofs(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                meeting, url = 123, f'https://{module.SUITE_HOST}/iip/elpaso/meeting/details/123'
                authority = f'<a href="{module.SUITE_LIST}">Calendar</a>'.encode()
                calendar = b'<a href="/iip/elpaso/meeting/list/2">2</a>'
                page_url = module.SUITE_LIST + '/2'
                page = b'<a href="/iip/elpaso/meeting/details/123">Details</a>'
                detail = b'<html><script>Axon</script><form>Palantir</form><main><p>Flock Safety discussion</p></main></html>'
                observations = [(receipt(source, body), body) for source, body in [
                    (module.SUITE_AUTHORITY, authority), (module.SUITE_LIST, calendar), (page_url, page), (url, detail)]]
                module.archive(root, observations)
                candidate = {'url': url, 'authority_sha256': module.sha(authority), 'authority_anchor': 0,
                             'root_sha256': module.sha(calendar), 'page_url': page_url, 'page_sha256': module.sha(page),
                             'page_anchor': 0, 'meeting_anchor': 0}
                result = {'receipt': observations[-1][0], 'candidate': candidate, 'node': 0}
                record = module.materialize_suite(root, module.observation_keys(root), meeting, json.dumps(candidate),
                                                  json.dumps(result), '2026-11-11T00:00:00+00:00', False)
                self.assertEqual(record['signals'], ['Flock'])
                self.assertEqual(record['locator'], 'HTML text node 1')
                self.assertNotIn('Axon', record['title'])
                candidate['meeting_anchor'] = 1
                result['candidate'] = candidate
                with self.assertRaises(ValueError):
                    module.materialize_suite(root, module.observation_keys(root), meeting, json.dumps(candidate),
                                             json.dumps(result), '2026-11-11T00:00:00+00:00', False)
            finally:
                db.close()

    def test_export_reopens_exact_raw_source_and_rejects_tampering(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                client, ref, page = 'denver', 'us-co-place-0820000', 0
                url = module.query_url(client, page, 100)
                source = [{'MatterId': 123, 'MatterFile': 'TEST-1', 'MatterTitle': 'Flock Safety cameras',
                           'MatterStatusName': 'Agenda Ready', 'MatterIntroDate': '2026-01-01'}]
                body = json.dumps(source).encode()
                obs = receipt(url, body)
                module.archive(root, [(obs, body)])
                with db:
                    db.execute('INSERT INTO scan_history VALUES (?,?,?,?)', (client, page, obs['sha256'], json.dumps(obs)))
                    db.execute('INSERT INTO leads VALUES (?,?,?,?,?,?,?,?,?)',
                        ('co-legistar-denver-123', ref, client, 123, page, 0, obs['sha256'],
                         json.dumps({'title': source[0]['MatterTitle']}), obs['retrieved_at']))
                output = root / 'public.json'
                module.export_report(root, db, output)
                report = json.loads(output.read_text())
                self.assertEqual(len(report['records']), 1)
                self.assertEqual(report['records'][0]['confidence'], 'unconfirmed_source_reference')
                self.assertIsNone(report['coverage']['lookback_period'])
                db.execute("UPDATE leads SET record_json=?", (json.dumps({'title': 'fabricated Flock claim'}),))
                db.commit()
                module.export_report(root, db, output)
                self.assertEqual(json.loads(output.read_text())['records'], [])
                self.assertEqual(json.loads(output.read_text())['coverage']['omitted_failed_provenance'], 1)
            finally:
                db.close()

    def test_preserved_index_and_document_line_are_rechecked(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref = 'us-co-county-08051'
                index = 'https://example.org/agenda'
                url = 'https://example.org/AgendaCenter/ViewFile/Minutes/_09152026-1206'
                page = b'<html><a href="/AgendaCenter/ViewFile/Minutes/_09152026-1206">Minutes</a></html>'
                pdf = b'%PDF-test fixture'
                page_obs, pdf_obs = receipt(index, page), receipt(url, pdf)
                module.archive(root, [(page_obs, page), (pdf_obs, pdf)])
                text = b'Flock Safety was discussed\n'
                (root / 'extracted').mkdir()
                (root / 'extracted' / module.sha(text)).write_bytes(text)
                candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0, 'label': 'Minutes'}
                result = {'state': 'matched', 'receipt': pdf_obs, 'document_sha256': module.sha(pdf),
                          'extracted_sha256': module.sha(text), 'evidence_id': 'evidence:test',
                          'finding': {'evidence_id': 'evidence:test', 'matched_rule_ids': ['vendor.flock-safety'],
                                      'citations': [{'evidence_id': 'evidence:test', 'locator': {'kind': 'line', 'start': 1}, 'quote': 'Flock Safety was discussed'}]}}
                with db:
                    db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (ref, 'https://example.org/', '2026-11-11T00:00:00+00:00', '{}'))
                    db.execute('INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,attempts,result_json,updated_at) VALUES (?,?,?,?,?,?,?)',
                               (ref, url, json.dumps(candidate), 'matched', 1, json.dumps(result), '2026-11-11T00:00:00+00:00'))
                output = root / 'public.json'
                module.export_report(root, db, output)
                self.assertEqual(len(json.loads(output.read_text())['records']), 1)
                result['extraction_status'] = 'complete_with_ocr'
                db.execute('UPDATE document_candidates SET result_json=?', (json.dumps(result),))
                db.commit()
                module.export_report(root, db, output)
                self.assertEqual(json.loads(output.read_text())['records'], [])
                result['ocr_page_set_validated'] = True
                db.execute('UPDATE document_candidates SET result_json=?', (json.dumps(result),))
                db.commit()
                module.export_report(root, db, output)
                self.assertEqual(json.loads(output.read_text())['records'], [])
                result['ocr_page_text_mapping_version'] = 2
                db.execute('UPDATE document_candidates SET result_json=?', (json.dumps(result),))
                db.commit()
                module.export_report(root, db, output)
                ocr_record = json.loads(output.read_text())['records'][0]
                self.assertEqual(ocr_record['title'], 'Possible Flock Safety reference in a public record.')
                self.assertEqual(ocr_record['people_context'], [])
                result['finding']['citations'][0]['quote'] = 'Flock was approved'
                db.execute('UPDATE document_candidates SET result_json=?', (json.dumps(result),))
                db.commit()
                module.export_report(root, db, output)
                self.assertEqual(json.loads(output.read_text())['records'], [])

                # A later discovery can replace the current index candidate;
                # the result must remain tied to the original captured anchor.
                result['finding']['citations'][0]['quote'] = 'Flock Safety was discussed'
                result['candidate'] = candidate
                newer_candidate = {**candidate, 'index_sha256': 'f' * 64}
                db.execute('UPDATE document_candidates SET candidate_json=?,result_json=?,updated_at=?,last_attempt_at=NULL',
                           (json.dumps(newer_candidate), json.dumps(result), '2020-01-01T00:00:00+00:00'))
                db.commit()
                module.export_report(root, db, output)
                self.assertEqual(len(json.loads(output.read_text())['records']), 1)

                with patch.object(module, 'screen_one', return_value=({'state': 'error', 'error': 'temporary failure'}, [])):
                    module.screen_documents(root, db, '/bin/true', 1, 1, 1, refresh_existing=True)
                state, error = db.execute('SELECT state,last_error FROM document_candidates').fetchone()
                self.assertEqual((state, error), ('matched', 'temporary failure'))
                module.export_report(root, db, output)
                self.assertEqual(json.loads(output.read_text())['coverage']['refresh_failures_preserving_prior_results'], 1)

                db.execute('UPDATE document_candidates SET last_attempt_at=NULL')
                db.commit()
                with patch.object(module, 'screen_one', return_value=({'state': 'no_match', 'document_sha256': module.sha(pdf)}, [])):
                    module.screen_documents(root, db, '/bin/true', 1, 1, 1, refresh_existing=True)
                module.export_report(root, db, output)
                archived = json.loads(output.read_text())['records']
                self.assertEqual(len(archived), 1)
                self.assertTrue(archived[0]['historical'])
                self.assertEqual(db.execute('SELECT count(*) FROM document_history').fetchone()[0], 1)
            finally:
                db.close()

    def test_public_comment_name_is_not_repeated_in_document_reference(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref, index = 'us-co-county-08051', 'https://example.org/agenda'
                url = 'https://example.org/AgendaCenter/ViewFile/Minutes/_09152026-1206'
                page = b'<a href="/AgendaCenter/ViewFile/Minutes/_09152026-1206">Minutes</a>'
                pdf = b'%PDF-comment'
                page_obs, pdf_obs = receipt(index, page), receipt(url, pdf)
                module.archive(root, [(page_obs, page), (pdf_obs, pdf)])
                text = b'Citizen Comments\nJane Example suggested facial recognition.\n'
                (root / 'extracted').mkdir()
                (root / 'extracted' / module.sha(text)).write_bytes(text)
                candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0}
                result = {'state': 'matched', 'receipt': pdf_obs, 'document_sha256': module.sha(pdf),
                          'extracted_sha256': module.sha(text), 'evidence_id': 'evidence:comment',
                          'finding': {'evidence_id': 'evidence:comment', 'matched_rule_ids': ['technology.facial-recognition'],
                                      'citations': [{'evidence_id': 'evidence:comment', 'locator': {'kind': 'line', 'start': 2},
                                                     'quote': 'Jane Example suggested facial recognition.'}]}}
                with db:
                    db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (ref, 'https://example.org/', module.utcnow(), '{}'))
                    db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at)
                                  VALUES (?,?,?,'matched',?,?)''', (ref, url, json.dumps(candidate), json.dumps(result), module.utcnow()))
                output = root / 'public.json'
                module.export_report(root, db, output)
                report = json.loads(output.read_text())
                self.assertEqual(len(report['records']), 1)
                self.assertNotIn('Jane Example', output.read_text())
                self.assertEqual(report['records'][0]['people_context'], [])
            finally:
                db.close()

    def test_school_document_results_do_not_publish_private_names_or_passages(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref, index = 'us-co-school-0801920', 'https://school.example.org/agenda'
                url = 'https://school.example.org/agenda.pdf'
                page = b'<a href="/agenda.pdf">Student Jane Example agenda</a>'
                pdf = b'%PDF-school'
                page_obs, pdf_obs = receipt(index, page), receipt(url, pdf)
                module.archive(root, [(page_obs, page), (pdf_obs, pdf)])
                text = b'Jane Example discussed Flock cameras.\n' + b'Other line\n' * 24 + b'Jane Example proposed a Flock camera resolution.\n'
                (root / 'extracted').mkdir()
                (root / 'extracted' / module.sha(text)).write_bytes(text)
                candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0,
                             'label': 'Student Jane Example agenda'}
                result = {'state': 'matched', 'receipt': pdf_obs, 'document_sha256': module.sha(pdf),
                          'extracted_sha256': module.sha(text), 'evidence_id': 'evidence:school',
                          'finding': {'evidence_id': 'evidence:school', 'matched_rule_ids': ['vendor.flock-safety'],
                                      'citations': [{'evidence_id': 'evidence:school', 'locator': {'kind': 'line', 'start': 1},
                                                     'quote': 'Jane Example discussed Flock cameras.'}]}}
                with db:
                    db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (ref, 'https://school.example.org/', module.utcnow(), '{}'))
                    db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at)
                                  VALUES (?,?,?,'matched',?,?)''', (ref, url, json.dumps(candidate), json.dumps(result), module.utcnow()))
                output = root / 'public.json'
                module.export_report(root, db, output)
                report = json.loads(output.read_text())
                self.assertEqual(len(report['records']), 1)
                self.assertNotIn('Jane Example', output.read_text())
                self.assertEqual(report['records'][0]['people_context'], [])
                self.assertNotIn('document_highlights', report['records'][0])
                self.assertEqual(report['records'][0]['file'], 'School district meeting document')
            finally:
                db.close()

    def test_special_district_document_uses_generic_public_reference(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            ref, name, home = 'dola-district-01006', 'Crestview Water & Sanitation District', 'https://district.example/'
            index, url = home + 'agenda', home + 'agenda.pdf'
            identity = b'<h1>Crestview Water &amp; Sanitation District</h1>'
            page = b'<a href="/agenda.pdf">Jane Example agenda</a>'
            pdf = b'%PDF-district'
            identity_obs, page_obs, pdf_obs = receipt(home, identity), receipt(index, page), receipt(url, pdf)
            identity_obs['content_type'] = 'text/html'
            module.archive(root, [(identity_obs, identity), (page_obs, page), (pdf_obs, pdf)])
            text = b'Jane Example discussed Flock cameras.\n'
            (root / 'extracted').mkdir()
            (root / 'extracted' / module.sha(text)).write_bytes(text)
            candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0,
                         'label': 'Jane Example agenda'}
            result = {'state': 'matched', 'receipt': pdf_obs, 'document_sha256': module.sha(pdf),
                      'extracted_sha256': module.sha(text), 'evidence_id': 'evidence:district',
                      'finding': {'evidence_id': 'evidence:district', 'matched_rule_ids': ['vendor.flock-safety'],
                                  'citations': [{'evidence_id': 'evidence:district', 'locator': {'kind': 'line', 'start': 1},
                                                 'quote': 'Jane Example discussed Flock cameras.'}]}}
            try:
                with db:
                    db.execute('insert into discovery values (?,?,?,?)',
                               (ref, home, module.utcnow(), json.dumps({'district_identity_receipt': identity_obs})))
                    db.execute('''insert into document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at)
                                  values (?,?,?,'matched',?,?)''', (ref, url, json.dumps(candidate), json.dumps(result), module.utcnow()))
                output = root / 'public.json'
                with patch.object(module, 'special_district_website_leads', return_value=[(ref, name, home)]):
                    module.export_report(root, db, output)
                report = json.loads(output.read_text())
                self.assertEqual(len(report['records']), 1)
                self.assertEqual(report['records'][0]['file'], 'Special district meeting document')
                self.assertEqual(report['records'][0]['people_context'], [])
                self.assertNotIn('Jane Example', output.read_text())
                self.assertNotIn('document_highlights', report['records'][0])
            finally:
                db.close()

    def test_school_matter_results_do_not_publish_raw_title_or_personal_fields(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                client, ref = 'school-fixture', 'us-co-school-0801920'
                url = module.query_url(client, 0, 100, module.LEGACY_TERM_VERSION)
                source = [{'MatterId': 123, 'MatterTitle': 'Flock cameras discussed by Jane Example',
                           'MatterFile': 'Student Jane Example', 'MatterStatusName': 'Jane Example'}]
                body = json.dumps(source).encode(); obs = receipt(url, body)
                module.archive(root, [(obs, body)])
                with db:
                    db.execute('insert into scan_history values (?,?,?,?)', (client, 0, obs['sha256'], json.dumps(obs)))
                    db.execute('insert into leads values (?,?,?,?,?,?,?,?,?)',
                               ('co-legistar-school-fixture-123', ref, client, 123, 0, 0, obs['sha256'],
                                json.dumps({'title': source[0]['MatterTitle']}), module.utcnow()))
                    db.execute('insert into matter_actions values (?,?,?,?,?)', (client, 123, 1, '{}', module.utcnow()))
                output = root / 'public.json'
                with patch.object(module, 'clients_from_store', return_value={client: ref}):
                    module.export_report(root, db, output)
                report = json.loads(output.read_text())
                self.assertEqual(len(report['records']), 1)
                self.assertNotIn('Jane Example', output.read_text())
                self.assertNotIn('actions', report['records'][0])
                self.assertEqual(report['records'][0]['file'], 'School district agenda item')
            finally:
                db.close()

    def test_artwork_called_flock_is_counted_but_not_published_as_surveillance(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            db = module.root_store(root)
            try:
                ref, index = 'us-co-county-08051', 'https://example.org/agenda'
                url = 'https://example.org/AgendaCenter/ViewFile/Minutes/_09152026-1206'
                page = b'<a href="/AgendaCenter/ViewFile/Minutes/_09152026-1206">Minutes</a>'
                pdf = b'%PDF-artwork'
                page_obs, pdf_obs = receipt(index, page), receipt(url, pdf)
                module.archive(root, [(page_obs, page), (pdf_obs, pdf)])
                text = b'Roundabout Submissions\nArtist - Jeff Reed\n3. The Flock 20 feet $65,000\n'
                (root / 'extracted').mkdir()
                (root / 'extracted' / module.sha(text)).write_bytes(text)
                candidate = {'url': url, 'index_url': index, 'index_sha256': module.sha(page), 'anchor_index': 0}
                result = {'state': 'matched', 'receipt': pdf_obs, 'document_sha256': module.sha(pdf),
                          'extracted_sha256': module.sha(text), 'evidence_id': 'evidence:art',
                          'finding': {'evidence_id': 'evidence:art', 'matched_rule_ids': ['vendor.flock-safety'],
                                      'citations': [{'evidence_id': 'evidence:art', 'locator': {'kind': 'line', 'start': 3},
                                                     'quote': '3. The Flock 20 feet $65,000'}]}}
                with db:
                    db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (ref, 'https://example.org/', module.utcnow(), '{}'))
                    db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at)
                                  VALUES (?,?,?,'matched',?,?)''', (ref, url, json.dumps(candidate), json.dumps(result), module.utcnow()))
                output = root / 'public.json'
                module.export_report(root, db, output)
                report = json.loads(output.read_text())
                self.assertEqual(report['records'], [])
                self.assertEqual(report['coverage']['non_surveillance_mentions_withheld'], 1)
                self.assertEqual(report['coverage']['omitted_failed_provenance'], 0)
            finally:
                db.close()

    def test_symlinked_preserved_blob_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'real'
            source.write_bytes(b'hello')
            link = root / 'link'
            link.symlink_to(source)
            with self.assertRaises(OSError): module.read_blob(link, 100)


if __name__ == '__main__':
    unittest.main()
