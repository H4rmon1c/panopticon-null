#!/usr/bin/env python3
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('context_research', ROOT / 'scripts/colorado-research.py')
r = importlib.util.module_from_spec(spec); spec.loader.exec_module(r)
c = r.official_context

class ContextTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name).resolve(); self.db = r.root_store(self.path); self.addCleanup(self.db.close)
        self.seed = dict(c.SEEDS['broomfield-flock']); self.name = 'broomfield-flock'
        self.body = ('<html><body><p>The City and County of Broomfield (CCOB) Police Department utilizes '
                     'Flock Safety Automated License Plate Readers (ALPRs) to support criminal investigations.</p></body></html>').encode()
        self.bind_context_hash(self.seed, self.body)
        seed_patch = patch.dict(c.SEEDS, {self.name: self.seed}); seed_patch.start(); self.addCleanup(seed_patch.stop)
        self.url = self.seed['url']; self.digest = r.sha(self.body)
        self.receipt = {'url': self.url, 'status': 200, 'sha256': self.digest, 'bytes': len(self.body),
                        'retrieved_at': '2026-11-11T12:00:00+00:00', 'content_type': 'text/html',
                        'resolved_ips': ['8.8.8.8'], 'connected_ip': '8.8.8.8'}
        r.archive(self.path, [(self.receipt, self.body)])
        with self.db:
            self.db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (self.seed['jurisdiction_ref'],
                'https://www.broomfield.org/', '2026-11-11T12:00:00+00:00',
                json.dumps({'jurisdiction_ref': self.seed['jurisdiction_ref'], 'state': 'directory_site_checked'})))
        self.receipts = r.observation_keys(self.path)
    @staticmethod
    def bind_context_hash(seed, body):
        parser = c.VisibleNodes(seed.get('tab_panel_id')); parser.feed(body.decode('utf8'))
        a = next(i for i, n in enumerate(parser.nodes) if seed['agency'] in n)
        b = next(i for i, n in enumerate(parser.nodes) if seed['claim'] in n)
        first, last = sorted((a, b))
        seed['context_sha256'] = r.sha('\n'.join(parser.nodes[first:last + 1]).encode())
    def result(self):
        return c.materialize(r, self.path, self.db, self.receipts, self.name,
                             json.dumps(self.receipt), '2026-11-11T12:00:00+00:00')
    def test_exact_source_and_jurisdiction_emit_only_context(self):
        record = self.result(); self.assertEqual(record['kind'], 'official_page_context')
        self.assertEqual(record['jurisdiction_ref'], self.seed['jurisdiction_ref'])
        self.assertNotIn('actions', record); self.assertNotIn('people_context', record)
        self.assertEqual(record['locator'], 'HTML text node 1')
    def test_bad_receipt_or_agency_website_rejected(self):
        self.receipt['url'] = 'https://evil.example/context'
        with self.assertRaises(ValueError): self.result()
        self.receipt['url'] = self.url
        with self.db: self.db.execute('UPDATE discovery SET source_url=?', ('https://evil.example/',))
        with self.assertRaises(ValueError): self.result()
    def test_hidden_and_wrong_agency_rejected(self):
        for body in (self.body.replace(b'<p>', b'<p hidden>'),
                     self.body.replace(b'<p>', b'<p style="display: none">'),
                     self.body.replace(b'<p>', b'<p class="d-none">'),
                     self.body.replace(b'Broomfield', b'Other City'),
                     self.body + self.body):
            with self.subTest(body=body):
                with self.assertRaises(ValueError): c.locate(r, self.seed, body)
        with self.assertRaises(ValueError):
            c.locate(r, self.seed, b'<div>' * 257 + self.body + b'</div>' * 257)
        with self.assertRaises(ValueError):
            c.locate(r, self.seed, self.body.replace(b'<p>', b'<p style="display:none" style="display:block">'))
    def test_changed_successful_source_withdraws_old_row(self):
        with self.db: self.db.execute('INSERT INTO official_context_pages(seed_id,receipt_json,updated_at) VALUES (?,?,?)',
                                      (self.name, json.dumps(self.receipt), self.receipt['retrieved_at']))
        changed = b'<html><body>unrelated government page</body></html>'
        changed_receipt = dict(self.receipt, sha256=r.sha(changed), bytes=len(changed))
        with patch.object(r, 'get_robots', return_value=(None, [])), \
             patch.object(r, 'get', return_value=[(changed_receipt, changed)]), \
             patch.object(r, 'discovery_hosts', return_value={'broomfield.org'}):
            c.capture(r, self.path, self.db)
        row = self.db.execute('SELECT receipt_json,updated_at,state FROM official_context_pages WHERE seed_id=?', (self.name,)).fetchone()
        self.assertEqual(row[2], 'withdrawn')
        tombstone = c.materialize(r, self.path, self.db, r.observation_keys(self.path), self.name, *row)
        self.assertEqual(tombstone['id'], 'co-official-context-broomfield-flock')
        self.assertEqual(tombstone['context_state'], 'withdrawn')
        self.assertEqual(tombstone['locator'], 'Source response (withdrawal)')
        self.assertNotIn('Flock', tombstone['title'])
        history = self.db.execute('SELECT source_sha256,event FROM official_context_history WHERE seed_id=?', (self.name,)).fetchone()
        self.assertEqual(history, (self.digest, 'withdrawn_after_source_change'))
        with self.db: self.db.execute('DELETE FROM official_context_history WHERE seed_id=?', (self.name,))
        with self.assertRaises(ValueError):
            c.materialize(r, self.path, self.db, r.observation_keys(self.path), self.name, *row)
    def test_unknown_seed_rejected(self):
        with self.assertRaises(ValueError):
            c.materialize(r, self.path, self.db, self.receipts, 'unknown', json.dumps(self.receipt), self.receipt['retrieved_at'])
    def test_new_fixed_agency_passages_reject_wrong_or_changed_text(self):
        for name in ('greeley-flock', 'boulder-flock'):
            seed = dict(c.SEEDS[name])
            statement = seed['combined_start'] + ' in this synthetic example.'
            body = ('<html><body><p>' + statement + '</p></body></html>').encode()
            self.bind_context_hash(seed, body)
            with self.subTest(name=name):
                self.assertEqual(c.locate(r, seed, body), 0)
                for changed in (body.replace(b'<p>', b'<p hidden>'),
                                body.replace(seed['combined_start'].encode(), b'Other Agency'),
                                body.replace(b'synthetic example', b'altered example'), body + body):
                    with self.assertRaises(ValueError): c.locate(r, seed, changed)
    def test_greeley_signal_does_not_infer_alpr_from_other_page_text(self):
        name = 'greeley-flock'; seed = dict(c.SEEDS[name])
        body = ('<p>' + seed['combined_start'] + ' in this synthetic example.</p>'
                '<p>These cameras do not read license plates.</p>').encode()
        self.bind_context_hash(seed, body)
        saved = dict(self.receipt, url=seed['url'], sha256=r.sha(body), bytes=len(body), content_type='text/html')
        r.archive(self.path, [(saved, body)])
        with self.db:
            self.db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (seed['jurisdiction_ref'],
                'https://greeleyco.gov/', self.receipt['retrieved_at'], '{}'))
        with patch.dict(c.SEEDS, {name: seed}):
            record = c.materialize(r, self.path, self.db, r.observation_keys(self.path), name,
                                   json.dumps(saved), self.receipt['retrieved_at'])
        self.assertEqual(record['signals'], ['Flock Safety'])
        self.assertEqual(record['title'], 'Greeley police describe their Flock Safety camera program.')
    def test_sterling_fixed_passage_identity_and_withdrawal(self):
        name = 'sterling-bwc'; seed = c.SEEDS[name]
        passage = ('The Sterling Police Department issues body worn cameras to all sworn members to enhance the mission '
                   'of the Department by accurately capturing contacts between members of the Department and the public. '
                   'Body worn cameras are activated any time an officer conducts enforcement activities such as traffic stops, '
                   'field interviews and contacts, dispatched calls for service or any time an officer would normally notify '
                   'dispatch of a law enforcement activity in which the officer is in contact with the public.')
        body = ('<html><body><p>' + passage + '</p></body></html>').encode()
        self.assertEqual(c.locate(r, seed, body), 0)
        for changed in (body.replace(b'<p>', b'<p hidden>'), body.replace(b'Sterling Police', b'Other Police'),
                        body.replace(b'all sworn members', b'some sworn members'),
                        body.replace(b'traffic stops', b'other incidents'), body + body):
            with self.assertRaises(ValueError): c.locate(r, seed, changed)
        split = ('<p>The Sterling Police Department runs traffic safety.</p><p>Other City Police Department issues '
                 'body worn cameras to all sworn members.</p>').encode()
        with self.assertRaises(ValueError): c.locate(r, seed, split)
        saved = dict(self.receipt, url=seed['url'], sha256=r.sha(body), bytes=len(body), content_type='text/html')
        r.archive(self.path, [(saved, body)])
        with self.db:
            self.db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (seed['jurisdiction_ref'],
                'https://www.sterlingcolo.com/', self.receipt['retrieved_at'], '{}'))
            self.db.execute('INSERT INTO official_context_pages(seed_id,receipt_json,updated_at) VALUES (?,?,?)',
                            (name, json.dumps(saved), saved['retrieved_at']))
        record = c.materialize(r, self.path, self.db, r.observation_keys(self.path), name, json.dumps(saved), saved['retrieved_at'])
        self.assertEqual(record['jurisdiction'], 'Sterling')
        self.assertEqual(record['signals'], ['Body-worn cameras'])
        self.assertNotIn('actions', record)
        changed = b'<html><body>Unrelated page</body></html>'
        changed_receipt = dict(saved, sha256=r.sha(changed), bytes=len(changed))
        with patch.dict(c.SEEDS, {name: seed}, clear=True), patch.object(r, 'get_robots', return_value=(None, [])), \
             patch.object(r, 'get', return_value=[(changed_receipt, changed)]):
            c.capture(r, self.path, self.db)
        row = self.db.execute('SELECT receipt_json,updated_at,state FROM official_context_pages WHERE seed_id=?', (name,)).fetchone()
        self.assertEqual(row[2], 'withdrawn')
        tombstone = c.materialize(r, self.path, self.db, r.observation_keys(self.path), name, *row)
        self.assertEqual(tombstone['id'], record['id'])
        self.assertEqual(tombstone['signals'], ['Source changed'])
    def test_only_exact_known_collapsed_tab_can_supply_context(self):
        seed = dict(c.SEEDS['westminster-axon']); panel = seed['tab_panel_id']; labelled = 'tabBtn' + panel[3:]
        body = (f'<div id="{panel}" class="tabbedWidget cpTabPanel" role="region" '
                f'aria-labelledby="{labelled}" style="display:none"><p>{seed["claim"]}</p>'
                f'<p>{seed["agency"]}</p></div>').encode()
        self.bind_context_hash(seed, body)
        self.assertGreaterEqual(c.locate(r, seed, body), 0)
        for changed in (body.replace(panel.encode(), b'other-panel'),
                        body.replace(labelled.encode(), b'other-label'),
                        body.replace(b'role="region"', b'role="presentation"'),
                        body.replace(b'class="tabbedWidget cpTabPanel"', b'class="other"').replace(b'style="display:none"', b''),
                        body.replace(b'display:none', b'visibility:hidden')):
            with self.assertRaises(ValueError): c.locate(r, seed, changed)
        separate = (f'<p>{seed["claim"]}</p><div id="{panel}" class="tabbedWidget cpTabPanel" '
                    f'role="region" aria-labelledby="{labelled}" style="display:none">'
                    f'<p>{seed["agency"]}</p></div>').encode()
        with self.assertRaises(ValueError): c.locate(r, seed, separate)
        inserted = body.replace(seed['claim'].encode(), b'Other City Police Department: ' + seed['claim'].encode())
        with self.assertRaises(ValueError): c.locate(r, seed, inserted)

    def test_rejects_unrelated_agency_and_cross_section_injection(self):
        agency = self.seed['agency']; claim = self.seed['claim']
        wrong_agency = (f'<p>{agency} runs traffic safety.</p><p>Other City {claim}.</p>').encode()
        with self.assertRaises(ValueError): c.locate(r, self.seed, wrong_agency)
        seed = dict(c.SEEDS['boulder-uas']); section = seed['section_id']
        valid = (f'<div id="{section}"><h2>{seed["agency"]}</h2><p>More detail.</p>'
                 f'<p>{seed["claim"]}</p></div>').encode()
        self.bind_context_hash(seed, valid)
        self.assertGreaterEqual(c.locate(r, seed, valid), 0)
        self.assertGreaterEqual(c.locate(r, seed, valid + b'<p>Unrelated page update.</p>'), 0)
        separate = (f'<div id="{section}"><h2>{seed["agency"]}</h2></div>'
                    f'<div><p>{seed["claim"]}</p></div>').encode()
        with self.assertRaises(ValueError): c.locate(r, seed, separate)
        other_heading = valid.replace(b'<p>More detail.</p>', b'<h2>Other Agency</h2>')
        with self.assertRaises(ValueError): c.locate(r, seed, other_heading)
        contradictory = valid.replace(b'<p>More detail.</p>', b'<p>We do not use drones. The following refers to Denver Police Department.</p>')
        with self.assertRaises(ValueError): c.locate(r, seed, contradictory)
        other_agency = valid.replace(seed['claim'].encode(), b'Other City: ' + seed['claim'].encode())
        with self.assertRaises(ValueError): c.locate(r, seed, other_agency)
        duplicated_scope = valid + f'<div id="{section}"></div>'.encode()
        with self.assertRaises(ValueError): c.locate(r, seed, duplicated_scope)
if __name__ == '__main__': unittest.main()
