#!/usr/bin/env python3
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('keenesburg', Path(__file__).with_name('keenesburg-motion-sidecar.py'))
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)

def receipt(url, body, status=200, target=None):
    value = {'url': url, 'retrieved_at': '2026-11-11T00:00:00+00:00', 'status': status,
             'resolved_ips': ['8.8.8.8'], 'connected_ip': '8.8.8.8', 'content_type': 'application/pdf',
             'sha256': m.r.sha(body), 'bytes': len(body)}
    if target: value['redirect_target'] = target
    return value

class Tests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); self.db = m.r.root_store(self.root); self.addCleanup(self.db.close)
        pdf = b'%PDF-test'; index = (''.join(f'<a href="/x/{n}">Other</a>' for n in range(220)) + f'<a href="{m.LINKED}">Regular Meeting</a>').encode()
        text = ('Monday, April 4, 2022 ' + m.PAGE1 + '\n6. New Business a. RESOLUTION NO. 2022-19 ' + m.MOTION19 + ' b. RESOLUTION NO. 2022-20 ' + m.MOTION20 + '\npage3\npage4\npage5\n').encode()
        pdf_sha, index_sha, text_sha = (m.r.sha(x) for x in (pdf, index, text)); self.pdf_sha=pdf_sha; self.index_sha=index_sha; self.text_sha=text_sha
        override = patch.multiple(m, PDF=pdf_sha, INDEX_SHA=index_sha, TEXT=text_sha); override.start(); self.addCleanup(override.stop)
        rb, ib = b'redirect1', b'redirect2'; target_indexed = m.URL + '?' + m.LINKED.split('?')[1]
        self.r1 = receipt(m.ORIGIN, rb, 302, m.URL); self.r2 = receipt(m.LINKED, ib, 302, target_indexed); self.doc = receipt(m.URL, pdf); self.indexed_doc = receipt(m.INDEXED_PDF, pdf)
        m.r.archive(self.root, [(receipt(m.INDEX, index), index), (self.doc, pdf), (self.indexed_doc, pdf), (self.r1, rb), (self.r2, ib)])
        (self.root/'extracted').mkdir(); (self.root/'extracted'/text_sha).write_bytes(text)
        self.candidate = {'url': m.URL, 'index_url': m.INDEX, 'index_sha256': index_sha, 'anchor_index': 220,
            'kind': 'minutes', 'label': 'Regular Meeting', 'linked_url': m.LINKED, 'origin_url': m.ORIGIN,
            'capture_adjustment': 'timestamp_omitted', 'redirect_receipt': self.r1, 'indexed_redirect_receipt': self.r2}
        self.result = {'candidate': self.candidate, 'document_sha256': pdf_sha, 'extracted_sha256': text_sha, 'receipt': self.doc}
        with self.db:
            self.db.execute('INSERT INTO discovery VALUES (?,?,?,?)', (m.REF, 'https://www.townofkeenesburg.com/', '2026-11-11T00:00:00+00:00', '{}'))
            self.db.execute("INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at) VALUES (?,?,?,'matched',?,?)", (m.REF, m.URL, json.dumps(self.candidate), json.dumps(self.result), '2026-11-11T00:00:00+00:00'))
    def test_votes_and_distinct_motion(self):
        value=m.materialize(self.root,self.db)
        self.assertEqual(len(value['votes']),7); self.assertTrue(all(x['position']=='Yes' for x in value['votes']))
        self.assertIn('2022-19',value['motion']); self.assertIn('2022-20',value['separate_motion'])
        self.assertEqual((value['source']['attendance_page'],value['source']['motion_page']), (1,2))
    def test_both_redirect_receipts_required(self):
        for rec in (self.r1,self.r2,self.indexed_doc):
            raw=(json.dumps(rec,sort_keys=True,separators=(',',':'))+'\n').encode(); path=self.root/'observations'/(m.r.sha(raw)+'.json'); original=path.read_bytes();path.unlink()
            with self.assertRaises((ValueError,OSError)): m.materialize(self.root,self.db)
            path.write_bytes(original)
    def test_changed_vote_or_item_order_fails_even_with_changed_text_digest(self):
        text=('Monday, April 4, 2022 '+m.PAGE1+'\n6. New Business a. RESOLUTION NO. 2022-19 '+m.MOTION19.replace('Erger voted yes','Erger voted no')+' b. RESOLUTION NO. 2022-20 '+m.MOTION20+'\npage3\npage4\npage5\n').encode();digest=m.r.sha(text); (self.root/'extracted'/digest).write_bytes(text)
        self.result['extracted_sha256']=digest
        with self.db: self.db.execute('UPDATE document_candidates SET result_json=?',(json.dumps(self.result),))
        with patch.object(m,'TEXT',digest), self.assertRaises(ValueError): m.materialize(self.root,self.db)
    def test_public_research_binding(self):
        row={'id':m.RECORD,'kind':'meeting_document','historical':False,'jurisdiction_ref':m.REF,'source_url':m.URL,'source_sha256':m.PDF,'indexed_source_url':m.LINKED,'source_index_url':m.INDEX,'capture_adjustment':'timestamp_omitted'}
        m.require_research_binding({'records':[row]})
        for key in row:
            with self.subTest(key=key), self.assertRaises(ValueError): m.require_research_binding({'records':[{**row,key:'altered'}]})
        with self.assertRaises(ValueError): m.require_research_binding({'records':[row,row]})

if __name__=='__main__': unittest.main()
