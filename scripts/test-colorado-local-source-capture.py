import importlib.util
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location('local_capture', HERE / 'colorado-local-source-capture.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class LocalSourceCaptureTests(unittest.TestCase):
    def setUp(self):
        self.item = {'jurisdiction_ref': 'us-co-county-08017',
                     'url': 'https://kiowacountypress.net/content/record',
                     'source_type': 'third_party_sheriff_statement'}

    def test_manifest_rejects_untrusted_and_ambiguous_sources(self):
        for url in ('http://kiowacountypress.net/content/record',
                    'https://kiowacountypress.net.evil.example/content/record',
                    'https://user@kiowacountypress.net/content/record',
                    'https://kiowacountypress.net/content/record?token=abc',
                    'https://kiowacountypress.net:444/content/record'):
            with self.subTest(url=url), self.assertRaises(ValueError):
                module.validate_seeds({'schema_version': 1, 'sources': [{**self.item, 'url': url}]})
        with self.assertRaises(ValueError):
            module.validate_seeds({'schema_version': 1, 'sources': [self.item, self.item]})

    def test_capture_preserves_receipt_and_remains_private(self):
        html = b'<html><body>Flock LPR public meeting information. ' + b'Additional context. ' * 8 + b'</body></html>'
        receipt = {'status': 200, 'content_type': 'text/html', 'sha256': 'a' * 64,
                   'retrieved_at': '2026-11-11T00:00:00Z'}
        with patch.object(module.research, 'get_robots', return_value=(None, [])), \
             patch.object(module.research, 'get', return_value=[(receipt, html)]), \
             patch.object(module.research, 'archive') as archive:
            report = module.collect(Path('/tmp'), [self.item])
        self.assertEqual(report['state'], 'private_third_party_leads')
        self.assertEqual(report['sources'][0]['sha256'], receipt['sha256'])
        self.assertIn('Flock', report['sources'][0]['signals'])
        self.assertNotIn('Additional context', str(report))
        self.assertEqual(archive.call_count, 2)

    def test_failed_http_cannot_be_reported_as_capture(self):
        receipt = {'status': 403, 'content_type': 'text/html', 'sha256': 'a' * 64,
                   'retrieved_at': '2026-11-11T00:00:00Z'}
        with patch.object(module.research, 'get_robots', return_value=(None, [])), \
             patch.object(module.research, 'get', return_value=[(receipt, b'<html>denied</html>')]), \
             patch.object(module.research, 'archive'):
            report = module.collect(Path('/tmp'), [self.item])
        self.assertEqual(report['sources'][0]['status'], 'error')
        self.assertNotIn('sha256', report['sources'][0])


if __name__ == '__main__':
    unittest.main()
