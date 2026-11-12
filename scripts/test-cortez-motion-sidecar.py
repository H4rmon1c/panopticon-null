#!/usr/bin/env python3
"""Offline receipts, evidence identity and fail-closed motion regressions."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('garden_motion', Path(__file__).with_name('cortez-motion-sidecar.py'))
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)


def receipt(url, body):
    return {'url': url, 'retrieved_at': '2026-11-11T00:00:00+00:00', 'status': 200,
            'resolved_ips': ['8.8.8.8'], 'connected_ip': '8.8.8.8',
            'content_type': 'application/pdf', 'sha256': m.r.sha(body), 'bytes': len(body)}


class Tests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); self.db = m.r.root_store(self.root); self.addCleanup(self.db.close)
        pdf = b'%PDF-fixture'
        links = ''.join(f'<a href="/other/{n}">Other</a>' for n in range(69)) + f'<a href="{m.URL}">Minutes</a>'
        index = links.encode(); self.lines = [f'line {n}' for n in range(1, 97)]
        for n, line in m.LINES.items(): self.lines[n - 1] = line
        text = ('\n'.join(self.lines) + '\n').encode()
        self.pdf, self.index, self.text = m.r.sha(pdf), m.r.sha(index), m.r.sha(text)
        self.override = patch.multiple(m, PDF=self.pdf, INDEX_SHA=self.index, TEXT_SHA=self.text)
        self.override.start(); self.addCleanup(self.override.stop)
        self.pdf_receipt = receipt(m.URL, pdf)
        m.r.archive(self.root, [(receipt(m.INDEX, index), index), (self.pdf_receipt, pdf)])
        (self.root / 'extracted').mkdir(); (self.root / 'extracted' / self.text).write_bytes(text)
        self.candidate = {'url': m.URL, 'index_url': m.INDEX, 'index_sha256': self.index,
                          'anchor_index': 69, 'kind': 'minutes', 'label': 'Minutes'}
        self.result = {'document_sha256': self.pdf, 'extracted_sha256': self.text,
                       'receipt': self.pdf_receipt, 'candidate': self.candidate}
        with self.db:
            self.db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (m.REF, 'https://www.cortezco.gov/', '2026-11-11T00:00:00+00:00', '{}'))
            self.db.execute("INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at) VALUES (?,?,?,'matched',?,?)",
                            (m.REF, m.URL, json.dumps(self.candidate), json.dumps(self.result), '2026-11-11T00:00:00+00:00'))

    def save(self):
        with self.db: self.db.execute('UPDATE document_candidates SET result_json=?', (json.dumps(self.result),))

    def test_exact_sources_yield_only_item_vote(self):
        before = self.db.total_changes; value = m.materialize(self.root, self.db)
        self.assertEqual(self.db.total_changes, before)
        self.assertEqual(value['meeting_date'], '2021-03-23')
        self.assertEqual(len(value['votes']), 7)
        self.assertTrue(all(v['position'] == 'Yes' for v in value['votes']))
        self.assertEqual((value['source']['roster_page'], value['source']['vote_page']), (1, 2))
        self.assertEqual(value['source']['index_anchor'], 69)
        self.assertIn('separate from the consent', value['scope'])
        self.assertIn('not established', value['notice'])

    def test_research_snapshot_must_bind_the_exact_source(self):
        record = {'id': m.RECORD, 'kind': 'meeting_document', 'historical': False, 'jurisdiction_ref': m.REF, 'source_url': m.URL, 'source_sha256': m.PDF}
        m.require_research_binding({'records': [record]})
        for rows in ([], [record, record], [{**record, 'historical': True}], [{**record, 'kind': 'other'}], [{**record, 'source_sha256': '0' * 64}],
                     [{**record, 'jurisdiction_ref': 'other'}], [{**record, 'source_url': 'https://example.org'}]):
            with self.assertRaises(ValueError): m.require_research_binding({'records': rows})

    def test_forged_receipt_and_changed_source_are_withheld(self):
        self.result['receipt'] = {**self.pdf_receipt, 'retrieved_at': '2026-11-10T00:00:00+00:00'}
        self.save()
        with self.assertRaises((ValueError, OSError)): m.materialize(self.root, self.db)
        self.result['receipt'] = self.pdf_receipt; self.result['document_sha256'] = '0' * 64; self.save()
        with self.assertRaises(ValueError): m.materialize(self.root, self.db)
        self.result['document_sha256'] = self.pdf; self.result['candidate'] = {**self.candidate, 'anchor_index': 68}; self.save()
        with self.assertRaises(ValueError): m.materialize(self.root, self.db)

    def test_altered_vote_or_adjacent_motion_never_matches_even_with_new_text_digest(self):
        for n, value in [(6, 'Different council'), (75, 'different equipment'), (78, 'motion was rejected'), (81, 'Other'), (82, 'No'), (94, 'Other'), (95, 'No')]:
            with self.subTest(line=n):
                lines = self.lines.copy(); lines[n-1] = value; body = ('\n'.join(lines) + '\n').encode()
                digest = m.r.sha(body); (self.root / 'extracted' / digest).write_bytes(body)
                self.result['extracted_sha256'] = digest; self.save()
                with patch.object(m, 'TEXT_SHA', digest), self.assertRaises(ValueError):
                    m.materialize(self.root, self.db)


if __name__ == '__main__': unittest.main()
