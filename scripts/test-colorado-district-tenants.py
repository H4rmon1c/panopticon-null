#!/usr/bin/env python3
import json
import unittest
from pathlib import Path
from unittest.mock import patch
import colorado_district_tenants as t
import colorado_screen_tenant_leads as screen


class TenantTests(unittest.TestCase):
    def test_directory_join_and_shared_exact_paths_fail_closed(self):
        geo = [{'kind': 'district_provisional', 'id': 'dola-district-00001', 'code': '00001', 'name': 'Alpha Metro District'},
               {'kind': 'district_provisional', 'id': 'dola-district-00002', 'code': '00002', 'name': 'Beta Metro District'}]
        rows = [{'lgid': '00001', 'name': 'Alpha Metro District', 'url': 'www.example.org/alpha', 'lgstatus_i': '1.0'},
                {'lgid': '00002', 'name': 'Beta Metro District', 'url': 'example.org/alpha/', 'lgstatus_i': '1.0'}]
        self.assertEqual(t.leads(json.dumps(rows).encode(), geo), [])
        rows[1]['url'] = 'example.org/beta'
        self.assertEqual(len(t.leads(json.dumps(rows).encode(), geo)), 2)
        for bad in ['example.org/', 'example.org/../alpha', 'example.org/%2falpha', 'example.org/alpha?tenant=1',
                    'sites.google.com/site/alpha', 'example.org/alpha#other']:
            rows[0]['url'] = bad
            self.assertEqual(len(t.leads(json.dumps(rows).encode(), geo)), 1)
        rows[0]['url'] = 'example.org/alpha'
        rows[0]['name'] = 'Wrong Metro District'
        self.assertEqual(len(t.leads(json.dumps(rows).encode(), geo)), 1)

    def test_identity_requires_one_visible_exact_heading(self):
        name = 'Alpha Metro District'
        self.assertTrue(t.heading_matches(b'<h1>Alpha Metro District</h1>', name))
        for body in (b'<p>Alpha Metro District</p>', b'<script><h1>Alpha Metro District</h1></script>',
                     b'<h1>Alpha Metro District Number 1</h1>',
                     b'<h1>Alpha Metro District</h1><h2>Alpha Metro District</h2>'):
            self.assertFalse(t.heading_matches(body, name))

    def test_direct_candidates_remain_private_and_same_origin(self):
        page = b'<h1>Alpha Metro District</h1><a href="/docs/minutes.pdf">Minutes</a><a href="https://other.org/agenda.pdf">Agenda</a>'
        meta = {'url': 'https://example.org/alpha', 'status': 200, 'content_type': 'text/html', 'retrieved_at': '2026-11-11T00:00:00Z'}
        with patch.object(t.research, 'get_robots', return_value=(None, [])), patch.object(t.research, 'get', return_value=[(meta, page)]):
            result, _ = t.inspect(('dola-district-00001', 'Alpha Metro District', meta['url']), object())
        self.assertEqual(result['state'], 'identity_heading_matched')
        self.assertEqual([x['url'] for x in result['documents']], ['https://example.org/docs/minutes.pdf'])
        self.assertNotIn('vote', result)

    def test_redirect_does_not_cross_tenant_boundary(self):
        meta = {'status': 302, 'content_type': 'text/html'}
        with patch.object(t.research, 'get_robots', return_value=(None, [])), patch.object(t.research, 'get', return_value=[(meta, b'')]):
            result, _ = t.inspect(('dola-district-00001', 'Alpha Metro District', 'https://example.org/alpha'), object())
        self.assertEqual(result['state'], 'unavailable')
        self.assertEqual(result['documents'], [])

    def test_screening_rechecks_identity_receipt_and_anchor(self):
        body = b'<h1>Alpha Metro District</h1><a href="/docs/minutes.pdf">Minutes</a>'
        url = 'https://example.org/alpha'
        digest = t.research.sha(body)
        item = {'state': 'identity_heading_matched', 'jurisdiction_ref': 'dola-district-00001',
                'name': 'Alpha Metro District', 'url': url, 'source_sha256': digest,
                'documents': [{'url': 'https://example.org/docs/minutes.pdf', 'anchor_index': 0, 'kind': 'minutes'}]}
        geo = {'jurisdictions': []}
        with patch.object(Path, 'read_bytes', return_value=json.dumps(geo).encode()), \
             patch.object(screen.tenants, 'leads', return_value=[(item['jurisdiction_ref'], item['name'], url)]), \
             patch.object(screen.tenants.districts, 'pinned_source', return_value=b'[]'), \
             patch.object(screen.r, 'observation_keys', return_value={(url, digest)}), \
             patch.object(screen.r, 'source_blob', return_value=body):
            self.assertEqual(len(screen.validated_jobs(Path('/tmp'), {'results': [item]})), 1)
            item['documents'][0]['anchor_index'] = 2
            with self.assertRaises(ValueError):
                screen.validated_jobs(Path('/tmp'), {'results': [item]})
            item['documents'][0]['anchor_index'] = 0
            item['documents'][0]['url'] = 'https://attacker.example/minutes.pdf'
            with self.assertRaises(ValueError):
                screen.validated_jobs(Path('/tmp'), {'results': [item]})


if __name__ == '__main__':
    unittest.main()
