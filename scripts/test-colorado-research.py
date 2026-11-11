#!/usr/bin/env python3
"""Offline source-bound discovery and preliminary-export regressions."""
import importlib.util
import io
import json
from pathlib import Path
import sqlite3
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
                output = root / 'public.json'
                module.export_report(root, db, output)
                attempt = json.loads(output.read_text())['coverage']['search_attempts'][0]
                self.assertEqual((attempt['documents_screened'], attempt['documents_failed'], attempt['documents_queued']), (2, 1, 1))
                self.assertIsNone(attempt['lookback_period'])
            finally:
                db.close()

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

    def test_candidate_document_scope(self):
        index = 'https://city.example.org/agenda'
        good = 'https://city.example.org/AgendaCenter/ViewFile/Minutes/_09152026-1206'
        self.assertEqual(module.candidate_kind(good, index, ''), ('minutes', '2026-09-15'))
        self.assertEqual(module.candidate_kind('https://city.example.org/uploads/council-minutes.pdf', index, 'Minutes')[0], 'minutes')
        self.assertEqual(module.candidate_kind('https://city.example.org/uploads/council-minutes.docx', index, 'Minutes')[0], 'minutes')
        for bad in ['http://city.example.org/uploads/agenda.pdf', 'https://evil.example/uploads/agenda.pdf',
                    'https://city.example.org/uploads/agenda.pdf?token=x', 'https://city.example.org/uploads/%2e%2e/agenda.pdf',
                    'https://city.example.org/uploads/unrelated.pdf']:
            self.assertIsNone(module.candidate_kind(bad, index, ''))

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

    def test_document_signal_labels_must_occur_at_the_cited_line(self):
        ids = ['vendor.flock-safety', 'vendor.axon']
        self.assertEqual(module.labels_at_quote(ids, 'Flock Safety contract discussion'), ['Flock Safety'])
        self.assertEqual(module.labels_at_quote(ids, 'Axon body cameras'), ['Axon'])
        self.assertEqual(module.labels_at_quote(ids, 'A flock of birds flew overhead'), [])
        self.assertEqual(module.labels_at_quote(['vendor.soundthinking-shotspotter'], 'ShotSpotter project'), ['ShotSpotter'])

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
                old = {'state': 'no_match', 'extraction_status': 'complete_with_ocr', 'receipt': pdf_obs}
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
                result = {'state': 'no_match', 'extraction_status': 'complete_with_ocr', 'ocr_page_set_validated': True}
                with patch.object(module, 'extract_captured_pdf', return_value=result) as extract, patch.object(module, 'get', side_effect=AssertionError('no network')):
                    module.ocr_saved_documents(root, db, '/bin/true', 1, 1, revalidate=True)
                self.assertEqual(extract.call_args.args[4]['sha256'], module.sha(pdf))
                module.export_report(root, db, output)
                self.assertEqual(json.loads(output.read_text())['coverage']['document_screening'], {'no_match': 1})
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
        self.assertFalse(module.portrait_name_matches('Elizabeth Smith', 'Profile picture of Liz Smith'))
        self.assertTrue(module.portrait_name_matches('Elizabeth Smith', 'Profile picture of Liz Smith', 'Elizabeth (Liz) Smith, Commissioner'))
        self.assertFalse(module.portrait_name_matches('Elizabeth Smith', 'Profile picture of Liz Jones', 'Elizabeth (Liz) Smith, Commissioner'))

    def test_commissioner_profile_image_requires_exact_heading_caption_and_official_host(self):
        source = (b'<h1>Commissioner Abe Laydon, District I</h1>'
                  b'<img alt="Commissioner Abe Laydon" srcset="https://www.douglasco.gov/a-small.jpg 480w, '
                  b'https://www.douglasco.gov/a-original.jpg 2048w">')
        self.assertEqual(module.official_profile_image(source, 'Abe Laydon'),
                         ('https://www.douglasco.gov/a-original.jpg', 'Commissioner Abe Laydon'))
        self.assertIsNone(module.official_profile_image(source, 'George Teal'))
        self.assertIsNone(module.official_profile_image(source.replace(b'Commissioner Abe Laydon, District I', b'Other Person'), 'Abe Laydon'))
        self.assertIsNone(module.official_profile_image(source.replace(b'www.douglasco.gov', b'evil.example'), 'Abe Laydon'))

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
                self.assertEqual(len(json.loads(output.read_text())['records']), 1)
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
