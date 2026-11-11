#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parent))
spec = importlib.util.spec_from_file_location('eff_leads', Path(__file__).with_name('colorado-eff-private-leads.py'))
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)

class EFFTests(unittest.TestCase):
    def sample(self, state='CO', ref='AOS123456'):
        return (f'<table><tbody><tr data-ref="{ref}" class="desktop-only">'
                f'<td>Agency</td><td>Town</td><td>County</td><td>{state}</td><td>Technology</td><td>Vendor</td></tr>'
                '<tr class="desktop-only"><td><p class="links">Links: <a href="https://town.gov/a.pdf">Proof</a></p>'
                '<a href="/a/other">more info</a></td></tr><tr class="mobile-only"><td>duplicate</td></tr></tbody></table>').encode()
    def test_scoped_records_and_source_links(self):
        rows = m.parse_entries(self.sample()); self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]['links'], ['https://town.gov/a.pdf'])
        self.assertEqual(rows[0]['cells'][3], 'CO')
    def test_wrong_state_and_bad_id_rejected(self):
        with self.assertRaises(ValueError): m.parse_entries(self.sample(state='CA'))
        with self.assertRaises(ValueError): m.parse_entries(self.sample(ref='bad'))
    def test_allowlist_rejects_unsafe_sources(self):
        hosts = {'town.gov'}
        self.assertTrue(m.safe_official_url('https://town.gov/proof.pdf', hosts))
        for url in ('http://town.gov/a', 'https://town.gov.evil/a', 'https://u:p@town.gov/a',
                    'https://town.gov/a?token=x', 'https://town.gov/a#fragment', 'https://127.0.0.1/a'):
            self.assertFalse(m.safe_official_url(url, hosts))
        self.assertFalse(m.safe_official_url('https://other.gov/a', hosts))
        self.assertTrue(m.safe_official_url('https://other.gov/a', hosts, include_gov=True))
        self.assertFalse(m.safe_official_url('https://evilgov.example/a', hosts, include_gov=True))
    def test_reject_duplicate_records(self):
        with self.assertRaises(ValueError): m.parse_entries(self.sample() + self.sample())
if __name__ == '__main__': unittest.main()
