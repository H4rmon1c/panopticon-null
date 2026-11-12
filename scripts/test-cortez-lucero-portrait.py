#!/usr/bin/env python3
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('verifier', ROOT/'scripts/verify-cortez-lucero-portrait.py')
v = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)


class PortraitTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root/'content').mkdir(mode=0o700)
        (self.root/'observations').mkdir(mode=0o700)
        self.image = v.ASSET.read_bytes()
        self.pdf = b'%PDF-1.4\n' + v.HEADER + self.image + v.TRAILER + b'%%EOF\n'
        self.pdf_sha = hashlib.sha256(self.pdf).hexdigest()
        (self.root/'content'/self.pdf_sha).write_bytes(self.pdf)
        (self.root/'content'/v.IMAGE_SHA).write_bytes(self.image)
        self.receipt = {'url':v.URL,'status':200,'sha256':self.pdf_sha,
                        'resolved_ips':['8.8.8.8'],'connected_ip':'8.8.8.8',
                        'retrieved_at':'2026-11-12T00:00:00+00:00'}
        self.write_receipt()
        self.mock = patch.object(v, 'PDF_SHA', self.pdf_sha)
        self.mock.start()

    def tearDown(self):
        self.mock.stop()
        self.temp.cleanup()

    def write_receipt(self):
        raw = json.dumps(self.receipt, sort_keys=True, separators=(',', ':')).encode()
        self.receipt_path = self.root/'observations'/(hashlib.sha256(raw).hexdigest()+'.json')
        self.receipt_path.write_bytes(raw)

    def test_source_and_original_asset_are_verified(self):
        v.verify(self.root)

    def test_canonical_newline_receipt_is_accepted(self):
        raw = self.receipt_path.read_bytes()+b'\n'
        self.receipt_path.unlink()
        (self.receipt_path.parent/(hashlib.sha256(raw).hexdigest()+'.json')).write_bytes(raw)
        v.verify(self.root)

    def test_missing_or_mismatched_receipt_fails(self):
        self.receipt['url'] = 'https://example.org/not-the-source'
        self.receipt_path.unlink()
        self.write_receipt()
        with self.assertRaises(ValueError): v.verify(self.root)

    def test_receipt_filename_and_bytes_must_match(self):
        self.receipt_path.rename(self.receipt_path.parent/'arbitrary.json')
        with self.assertRaises(ValueError): v.verify(self.root)
        (self.receipt_path.parent/'arbitrary.json').unlink()
        self.write_receipt()
        with self.receipt_path.open('ab') as f: f.write(b' ')
        with self.assertRaises(ValueError): v.verify(self.root)

    def test_changed_pdf_fails(self):
        (self.root/'content'/self.pdf_sha).write_bytes(self.pdf+b'x')
        with self.assertRaises(ValueError): v.verify(self.root)

    def test_wrong_asset_fails(self):
        asset = self.root/'other.jpg'
        asset.write_bytes(self.image[:-1]+b'X')
        with self.assertRaises(ValueError): v.verify(self.root, asset)

    def test_duplicate_or_wrong_object_fails(self):
        for pdf in (self.pdf + v.HEADER, self.pdf.replace(v.HEADER, b'8 0 obj\n', 1)):
            with patch.object(v, 'PDF_SHA', hashlib.sha256(pdf).hexdigest()):
                with self.assertRaises(ValueError): v.check_embedded(pdf, self.image)

    def test_archive_symlink_is_refused(self):
        link = self.root.parent/(self.root.name+'-link')
        link.symlink_to(self.root)
        try:
            with self.assertRaises(OSError): v.verify(link)
        finally:
            link.unlink()


if __name__ == '__main__': unittest.main()
