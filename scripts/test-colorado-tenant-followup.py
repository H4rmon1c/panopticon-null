#!/usr/bin/env python3
import unittest
from pathlib import Path
from unittest.mock import patch
import colorado_tenant_followup as f


class FollowupTests(unittest.TestCase):
    def test_navigation_requires_labeled_local_https_without_query_or_pdf(self):
        page = b'''<a href="/current">Current Documents</a>
        <a href="https://other.example/agenda">Agendas</a>
        <a href="http://example.org/meetings">Meetings</a>
        <a href="/documents?tenant=other">Documents</a>
        <a href="/agenda.pdf">Agenda</a><a href="/other">News</a>'''
        self.assertEqual(f.navigation('https://example.org/alpha', page), [(0, 'https://example.org/current')])

    def test_navigation_limits_and_deduplicates(self):
        page = b''.join(f'<a href="/documents{i}">Documents</a>'.encode() for i in range(8))
        self.assertEqual(len(f.navigation('https://example.org/alpha', page)), 3)

    def test_documents_retain_exact_source_and_anchor(self):
        page = b'''<h1>Alpha Metro District</h1><a href="/a.pdf">Agenda</a>
        <a href="https://other.example/b.pdf">Minutes</a><a href="/news.pdf">News</a>'''
        docs = f.direct_documents('https://example.org/index', page)
        self.assertEqual(len(docs), 1)
        self.assertEqual(docs[0]['url'], 'https://example.org/a.pdf')
        self.assertEqual(docs[0]['index_sha256'], f.r.sha(page))
        self.assertEqual(docs[0]['anchor_index'], 0)

    def test_capture_requires_pinned_parent_receipt(self):
        import json
        item = {'state': 'identity_heading_matched', 'jurisdiction_ref': 'dola-district-00001',
                'name': 'Alpha Metro District', 'url': 'https://example.org/alpha', 'source_sha256': 'a' * 64}
        with patch.object(f.r, 'read_blob', return_value=json.dumps({'results': [item]}).encode()), \
             patch.object(Path, 'read_bytes', return_value=b'{"jurisdictions": []}'), \
             patch.object(f.tenants, 'leads', return_value=[(item['jurisdiction_ref'], item['name'], item['url'])]), \
             patch.object(f.tenants.districts, 'pinned_source', return_value=b'[]'), \
             patch.object(f.r, 'observation_keys', return_value=set()):
            with self.assertRaisesRegex(ValueError, 'receipt'):
                f.collect(Path('/tmp'), Path('/tmp/pnull'))


if __name__ == '__main__':
    unittest.main()
