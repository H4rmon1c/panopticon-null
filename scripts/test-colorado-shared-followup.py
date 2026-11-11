#!/usr/bin/env python3
import json
from pathlib import Path
import unittest
from unittest.mock import patch
import colorado_shared_followup as deep


class SharedFollowupTests(unittest.TestCase):
    def setUp(self):
        self.root = Path('/tmp/private')
        self.parent_url = 'https://example.org/'
        self.child_url = 'https://example.org/documents'
        self.parent_body = b'<h1>Alpha Metro District</h1><a href="/documents">Documents</a>'
        self.child_body = b'<h1>Alpha Metro District</h1><a href="/minutes.pdf">Minutes</a>'
        self.pdigest = deep.r.sha(self.parent_body)
        self.cdigest = deep.r.sha(self.child_body)
        self.group = [('dola-district-00001', 'Alpha Metro District', 'example.org'),
                      ('dola-district-00002', 'Beta Metro District', 'example.org')]
        self.manifest = {'results': [{'state': 'site_checked', 'host': 'example.org',
            'source_url': self.parent_url, 'sha256': self.pdigest}]}
        self.parent = {'host': 'example.org', 'parent_url': self.parent_url, 'parent_sha256': self.pdigest,
                       'parent_anchor_index': 0, 'url': self.child_url}
        self.page = {**self.parent, 'sha256': self.cdigest, 'captured_at': '2026-11-11T00:00:00Z'}
        self.sources = {self.pdigest: self.parent_body, self.cdigest: self.child_body}
        self.receipts = {(self.parent_url, self.pdigest), (self.child_url, self.cdigest)}

    def with_sources(self):
        return patch.multiple(deep.r, observation_keys=lambda *_: self.receipts,
                              source_blob=lambda _root, digest, *_: self.sources[digest])

    def test_parent_requires_directory_identity_and_receipt(self):
        with patch.object(Path, 'read_bytes', return_value=b'{"jurisdictions": []}'), \
             patch.object(deep.shared, 'leads', return_value=[('example.org', self.group)]), \
             patch.object(deep.shared.d, 'pinned_source', return_value=b'[]'), self.with_sources():
            self.assertEqual(deep.parents(self.root, self.manifest), [self.parent])
            self.receipts.remove((self.parent_url, self.pdigest))
            with self.assertRaisesRegex(ValueError, 'receipt'):
                deep.parents(self.root, self.manifest)

    def test_secondary_requires_receipt_exact_heading_and_parent_anchor(self):
        with patch.object(Path, 'read_bytes', return_value=b'{"jurisdictions": []}'), \
             patch.object(deep.shared, 'leads', return_value=[('example.org', self.group)]), \
             patch.object(deep.shared.d, 'pinned_source', return_value=b'[]'), self.with_sources():
            jobs, pages = deep.jobs_for_pages(self.root, [self.page], [self.parent])
            self.assertEqual(len(jobs), 1)
            self.assertEqual(pages[0]['matching_directory_names'], [('dola-district-00001', 'Alpha Metro District')])
            self.receipts.remove((self.child_url, self.cdigest))
            with self.assertRaisesRegex(ValueError, 'receipt'):
                deep.jobs_for_pages(self.root, [self.page], [self.parent])
            self.receipts.add((self.child_url, self.cdigest))
            wrong_anchor = {**self.page, 'parent_anchor_index': 3}
            with self.assertRaises(ValueError):
                deep.jobs_for_pages(self.root, [wrong_anchor], [wrong_anchor])
            self.sources[self.cdigest] = b'<p>Alpha Metro District</p><a href="/minutes.pdf">Minutes</a>'
            self.assertEqual(deep.jobs_for_pages(self.root, [self.page], [self.parent]), ([], []))

    def test_altered_document_anchor_is_rejected_by_existing_check(self):
        candidate = {'url': 'https://example.org/unrelated.pdf', 'index_url': self.child_url,
                     'index_sha256': self.cdigest, 'anchor_index': 0}
        with self.with_sources():
            with self.assertRaises(ValueError):
                deep.r.checked_document_candidate(self.root, candidate['url'], candidate)

    def test_redirect_does_not_follow_or_accept_secondary(self):
        metadata = {'status': 302}
        with patch.object(deep.r, 'get_robots', return_value=(None, [])), \
             patch.object(deep.r, 'get', return_value=[(metadata, b'')]) as fetch:
            page, _ = deep.capture_page(self.root, self.parent, object())
        self.assertIsNone(page)
        self.assertEqual(fetch.call_args.kwargs['redirects'], 0)
        self.assertTrue(fetch.call_args.kwargs['stop_at_redirect'])

    def test_document_offsets_are_applied_after_stable_sort(self):
        pages = [self.parent, {**self.parent, 'url': 'https://example.org/z'}]
        unsorted = [
            {'host': 'example.org', 'candidate': {'url': 'https://example.org/z.pdf'}},
            {'host': 'example.org', 'candidate': {'url': 'https://example.org/a.pdf'}},
        ]
        def capture(_root, page, _budget):
            return ({**page, 'sha256': self.cdigest}, [])
        with patch.object(deep.r, 'read_blob', return_value=b'{}'), \
             patch.object(deep, 'parents', return_value=pages), \
             patch.object(deep, 'capture_page', side_effect=capture), \
             patch.object(deep, 'jobs_for_pages', return_value=(unsorted, [])), \
             patch.object(deep.r, 'archive'), \
             patch.object(deep.r, 'screen_one', return_value=({'state': 'no_match'}, [])) as screen:
            result = deep.run(self.root, Path('/tmp/pnull'), 0, 0, 1, 2, 1, 2)
        self.assertEqual(result['documents_available'], 2)
        self.assertEqual(result['document_candidates'], 1)
        self.assertEqual(screen.call_args.args[1], 'us-co-shared-site')
        self.assertEqual(screen.call_args.args[2]['url'], 'https://example.org/z.pdf')


if __name__ == '__main__':
    unittest.main()
