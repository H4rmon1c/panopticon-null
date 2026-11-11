#!/usr/bin/env python3
import unittest
from pathlib import Path
from unittest.mock import patch
import colorado_cisc_2018_members as c


def fake_ocr():
    return ('2018 CISC Member Agencies ' + ' ; '.join(c.row_needle(n, name)
            for n, name in enumerate(c.NAMES, 1)) + ' Additional memberships pending Rev 11/18')


class CiscTests(unittest.TestCase):
    def test_all_rows_remain_private_dated_and_individually_located(self):
        result = c.parse_ocr(fake_ocr(), c.SOURCE_SHA256, 'a' * 64, '2026-11-11T00:00:00Z')
        self.assertEqual(len(result['rows']), 66)
        self.assertEqual(result['omitted_rows'], [])
        self.assertIn('does not establish current', result['notice'])
        self.assertEqual(result['rows'][0]['locator'], 'PDF page 1, left column, row 1')
        self.assertEqual(result['rows'][34]['locator'], 'PDF page 1, right column, row 35')
        self.assertTrue(all(x['state'] == 'ocr_candidate' and x['source_year'] == 2018 for x in result['rows']))
        self.assertTrue(all(x['transcription_method'] == 'manual_pretranscription_ocr_matched' for x in result['rows']))

    def test_missing_and_duplicate_ocr_rows_are_omitted(self):
        target = c.row_needle(10, c.NAMES[9])
        result = c.parse_ocr(fake_ocr().replace(target, 'unknown office'), c.SOURCE_SHA256, 'a' * 64, 'now')
        self.assertEqual(result['omitted_rows'], [{'row': 10, 'reason': 'OCR row missing or ambiguous'}])
        result = c.parse_ocr(fake_ocr() + ' ' + target, c.SOURCE_SHA256, 'a' * 64, 'now')
        self.assertEqual(result['omitted_rows'], [{'row': 10, 'reason': 'OCR row missing or ambiguous'}])

    def test_changed_source_or_missing_qualifier_fails_closed(self):
        with self.assertRaises(ValueError): c.parse_ocr(fake_ocr(), 'b' * 64, 'a' * 64, 'now')
        with self.assertRaises(ValueError): c.parse_ocr(fake_ocr().replace('Additional memberships pending', ''), c.SOURCE_SHA256, 'a' * 64, 'now')
        with self.assertRaises(ValueError): c.parse_ocr(fake_ocr(), c.SOURCE_SHA256, 'bad', 'now')

    def test_committee_anchor_must_be_unique_and_exact(self):
        body = b'<a href="/summary_attachments/20890/download">19HouseJud0129AttachD.pdf</a>'
        self.assertEqual(c.committee_anchor(body), 0)
        for bad in [body + body, body.replace(b'20890', b'12345'), body.replace(b'AttachD', b'AttachC')]:
            with self.assertRaises(ValueError): c.committee_anchor(bad)

    def test_disallowed_attachment_requires_explicit_narrow_override(self):
        body = b'<a href="/summary_attachments/20890/download">19HouseJud0129AttachD.pdf</a>'
        with patch.object(c.r, 'get_robots', return_value=(object(), [])), \
             patch.object(c.r, 'allowed', side_effect=lambda _rules, url: url == c.COMMITTEE), \
             patch.object(c.r, 'archive'), \
             patch.object(c.r, 'html_page', return_value=(body, [({'status': 200}, body)])), \
             patch.object(c.r, 'receipt_valid', return_value=True), \
             patch.object(c.r, 'get') as get:
            with self.assertRaisesRegex(ValueError, 'override'):
                c.capture(Path('/tmp/private'), Path('/tmp/pnull'), False)
            get.assert_not_called()

    def test_override_and_source_ocr_proofs_are_preserved(self):
        page = b'<a href="/summary_attachments/20890/download">19HouseJud0129AttachD.pdf</a>'
        pdf = b'%PDF-1.0 fixture'; pdfsha = c.r.sha(pdf)
        ocr = fake_ocr().encode(); ocrsha = c.r.sha(ocr)
        receipt = {'status': 200, 'retrieved_at': '2026-11-11T00:00:00Z'}
        parsed = {'extraction_status': 'complete_with_ocr', 'ocr_page_set_validated': True,
                  'ocr_page_text_mapping_version': 2, 'extracted_sha256': ocrsha}
        with patch.object(c, 'SOURCE_SHA256', pdfsha), \
             patch.object(c.r, 'get_robots', return_value=(object(), [])), \
             patch.object(c.r, 'allowed', side_effect=lambda _rules, url: url == c.COMMITTEE), \
             patch.object(c.r, 'archive'), \
             patch.object(c.r, 'html_page', return_value=(page, [({'status': 200}, page)])), \
             patch.object(c.r, 'receipt_valid', return_value=True), \
             patch.object(c.r, 'get', return_value=[(receipt, pdf)]), \
             patch.object(c.r, 'extract_captured_pdf', return_value=parsed), \
             patch.object(c.r, 'read_blob', return_value=ocr):
            result = c.capture(Path('/tmp/private'), Path('/tmp/pnull'), True)
        self.assertTrue(result['attachment_source']['robots_override_used'])
        self.assertEqual(result['committee_source']['anchor_index'], 0)
        self.assertEqual(len(result['rows']), 66)


if __name__ == '__main__':
    unittest.main()
