#!/usr/bin/env python3
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
spec=importlib.util.spec_from_file_location('county_recon',Path(__file__).with_name('colorado_county_root_recon.py'))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
class Tests(unittest.TestCase):
    def test_only_reached_unscreened_counties(self):
        g=[{'id':'us-co-county-08001','kind':'county'},{'id':'us-co-county-08003','kind':'county'}, {'id':'us-co-place-0800010','kind':'municipality'}]
        def attempt(ref,**values):return {'jurisdiction_ref':ref,'website_check':'site_checked','documents_screened':0,'portal_pages_checked':0,'agenda_meeting_pages_screened':0,**values}
        rows=[attempt(g[0]['id']),attempt(g[1]['id'],documents_screened=1),attempt(g[2]['id'])]
        self.assertEqual(m.unscreened({'coverage':{'search_attempts':rows}},g),[g[0]['id']])
        rows[0]['website_check']='unavailable';self.assertEqual(m.unscreened({'coverage':{'search_attempts':rows}},g),[])
    def test_duplicate_and_invalid_attempts_fail_closed(self):
        g=[{'id':'us-co-county-08001','kind':'county'}];a={'jurisdiction_ref':g[0]['id'],'website_check':'site_checked'}
        for rows in ([a,a],[dict(a,documents_screened=-1)],[dict(a,documents_screened='0')],[None]):
            with self.assertRaises(ValueError):m.unscreened({'coverage':{'search_attempts':rows}},g)
    def test_does_not_adopt_denver_or_any_other_legislative_portal(self):
        item = {'jurisdiction_ref': 'us-co-county-08031', 'portals': [{'client': 'denver'}], 'documents': []}
        sanitized, excluded = m.private_result(item)
        self.assertEqual(sanitized['portals'], [])
        self.assertEqual(excluded, 1)
        self.assertEqual(item['portals'], [{'client': 'denver'}])
        with self.assertRaises(ValueError): m.private_result({'portals': 'denver'})
    def test_private_leads_are_not_exporter_database_rows(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / 'private'
            m.create_private_archive(root)
            with self.assertRaises(FileExistsError): m.create_private_archive(root)
            (root / 'county-root-recon.json').write_text(json.dumps({'results': [{
                'jurisdiction_ref': 'us-co-county-08051', 'unverified_discovery': {
                    'documents': [{'url': 'https://example.gov/other-city.pdf',
                                   'label': 'Other city meeting minutes discussing Flock'}]}}]}))
            self.assertFalse((root / 'research.sqlite3').exists())
            db = m.r.root_store(root)
            try:
                output = Path(temp) / 'export.json'
                m.r.export_report(root, db, output)
                self.assertEqual(json.loads(output.read_text())['records'], [])
                self.assertEqual(db.execute('SELECT count(*) FROM discovery').fetchone()[0], 0)
                self.assertEqual(db.execute('SELECT count(*) FROM document_candidates').fetchone()[0], 0)
            finally: db.close()
if __name__=='__main__':unittest.main()
