#!/usr/bin/env python3
import json
import unittest
from pathlib import Path
from unittest.mock import patch
import colorado_shared_districts as s
import colorado_screen_shared_leads as screen


class SharedDistrictTests(unittest.TestCase):
    def setUp(self):
        self.geo = [{'kind': 'district_provisional', 'id': 'dola-district-00001', 'code': '00001', 'name': 'Alpha Metro District'},
                    {'kind': 'district_provisional', 'id': 'dola-district-00002', 'code': '00002', 'name': 'Beta Metro District'}]
        self.rows = [{'lgid': '00001', 'name': 'Alpha Metro District', 'url': 'www.example.org/', 'lgstatus_i': '1.0'},
                     {'lgid': '00002', 'name': 'Beta Metro District', 'url': 'example.org', 'lgstatus_i': '1.0'}]

    def test_only_exact_distinct_shared_roots(self):
        def get(): return s.leads(json.dumps(self.rows).encode(), self.geo)
        self.assertEqual(len(get()), 1)
        self.rows[1]['url'] = 'example.org/path'
        self.assertEqual(get(), [])
        self.rows[1]['url'] = 'example.org'
        self.rows[1]['name'] = 'Different District'
        self.assertEqual(get(), [])
        self.rows[1]['name'] = 'Beta Metro District'
        self.rows[1]['lgid'] = '00001'
        with self.assertRaises(ValueError): get()

    def test_source_links_stay_unattributed_and_same_host(self):
        body = b'''<h1>Alpha Metro District</h1><h2>Beta Metro District</h2>
        <a href="/meeting.pdf">Minutes</a><a href="https://other.example/agenda.pdf">Agenda</a>'''
        meta = {'status': 200, 'content_type': 'text/html', 'retrieved_at': '2026-11-11T00:00:00Z'}
        entry = ('example.org', [('dola-district-00001', 'Alpha Metro District', 'example.org'),
                                 ('dola-district-00002', 'Beta Metro District', 'example.org')])
        with patch.object(s.r, 'get_robots', return_value=(None, [])), patch.object(s.r, 'get', return_value=[(meta, body)]):
            item, _ = s.inspect(entry, object())
        self.assertEqual(item['visible_names'], ['dola-district-00001', 'dola-district-00002'])
        self.assertEqual(len(item['documents']), 1)
        self.assertNotIn('jurisdiction_ref', item['documents'][0])
        self.assertNotIn('decision', item)

    def test_redirect_does_not_open_another_host(self):
        entry = ('example.org', [('dola-district-00001', 'Alpha Metro District', 'example.org'),
                                 ('dola-district-00002', 'Beta Metro District', 'example.org')])
        with patch.object(s.r, 'get_robots', return_value=(None, [])), patch.object(s.r, 'get', return_value=[({'status': 302}, b'')]):
            item, _ = s.inspect(entry, object())
        self.assertEqual(item['state'], 'unavailable')
        self.assertEqual(item['documents'], [])

    def test_screening_requires_directory_receipt_and_exact_anchor(self):
        body = b'<a href="/minutes.pdf">Minutes</a>'
        source = 'https://example.org/'
        digest = s.r.sha(body)
        group = [('dola-district-00001', 'Alpha Metro District', 'example.org'),
                 ('dola-district-00002', 'Beta Metro District', 'example.org')]
        item = {'state': 'site_checked', 'host': 'example.org', 'source_url': source,
                'sha256': digest, 'documents': [{'url': 'https://example.org/minutes.pdf',
                'anchor_index': 0, 'kind': 'minutes'}]}
        with patch.object(Path, 'read_bytes', return_value=b'{"jurisdictions": []}'), \
             patch.object(screen.shared, 'leads', return_value=[('example.org', group)]), \
             patch.object(screen.shared.d, 'pinned_source', return_value=b'[]'), \
             patch.object(screen.r, 'observation_keys', return_value={(source, digest)}), \
             patch.object(screen.r, 'source_blob', return_value=body):
            self.assertEqual(len(screen.validated_jobs(Path('/tmp'), {'results': [item]})), 1)
            item['documents'][0]['anchor_index'] = 3
            with self.assertRaises(ValueError): screen.validated_jobs(Path('/tmp'), {'results': [item]})
            item['documents'][0]['anchor_index'] = 0
            item['documents'][0]['url'] = 'https://attacker.example/minutes.pdf'
            with self.assertRaises(ValueError): screen.validated_jobs(Path('/tmp'), {'results': [item]})


if __name__ == '__main__':
    unittest.main()
