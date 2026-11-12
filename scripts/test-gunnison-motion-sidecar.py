#!/usr/bin/env python3
import importlib.util,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('gunnison',Path(__file__).with_name('gunnison-motion-sidecar.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
def receipt(url,body):return {'url':url,'retrieved_at':'2026-11-11T00:00:00+00:00','status':200,'resolved_ips':['8.8.8.8'],'connected_ip':'8.8.8.8','content_type':'application/pdf','sha256':m.r.sha(body),'bytes':len(body)}
class Tests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name);self.db=m.r.root_store(self.root);self.addCleanup(self.db.close)
  pdf=b'%PDF-fixture';index=(''.join(f'<a href="/other/{n}">Other</a>' for n in range(206))+f'<a href="{m.URL}">November 19</a>').encode();current=index+b'<!-- updated index -->'
  self.lines=[f'line {n}' for n in range(1,157)]
  for n,v in m.LINES.items():self.lines[n-1]=v
  text=('\n'.join(self.lines)+'\n').encode();self.pdf=m.r.sha(pdf);self.index=m.r.sha(index);self.current=m.r.sha(current);self.text=m.r.sha(text)
  self.override=patch.multiple(m,PDF=self.pdf,INDEX_SHA=self.index,TEXT=self.text);self.override.start();self.addCleanup(self.override.stop)
  self.doc=receipt(m.URL,pdf);self.indexrec=receipt(m.INDEX,index);self.currentrec=receipt(m.INDEX+'#',current)
  m.r.archive(self.root,[(self.doc,pdf),(self.indexrec,index),(self.currentrec,current)])
  (self.root/'extracted').mkdir();(self.root/'extracted'/self.text).write_bytes(text)
  self.old={'url':m.URL,'index_url':m.INDEX,'index_sha256':self.index,'anchor_index':206,'kind':'minutes','label':'November 19'}
  self.new={**self.old,'index_sha256':self.current,'index_url':m.INDEX+'#'};self.result={'candidate':self.old,'document_sha256':self.pdf,'extracted_sha256':self.text,'receipt':self.doc}
  with self.db:
   self.db.execute('INSERT INTO discovery VALUES (?,?,?,?)',(m.REF,'https://gunnisoncounty.org/','2026-11-11T00:00:00+00:00','{}'))
   self.db.execute("INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at) VALUES (?,?,?,'matched',?,?)",(m.REF,m.URL,json.dumps(self.new),json.dumps(self.result),'2026-11-11T00:00:00+00:00'))
 def test_motion_and_attendance_do_not_invent_votes(self):
  value=m.materialize(self.root,self.db);self.assertEqual(value['outcome'],'Motion passed unanimously');self.assertNotIn('votes',value)
  self.assertEqual(len(value['attendance']),3);self.assertTrue(all(x['individual_vote']=='Not individually recorded' for x in value['attendance']))
  self.assertEqual((value['source']['attendance_page'],value['source']['motion_page']),(1,3));self.assertNotEqual(value['source']['index_sha256'],value['source']['current_index_sha256'])
 def test_both_index_receipts_are_required(self):
  for rec in (self.doc,self.indexrec,self.currentrec):
   raw=(json.dumps(rec,sort_keys=True,separators=(',',':'))+'\n').encode();path=self.root/'observations'/(m.r.sha(raw)+'.json');data=path.read_bytes();path.unlink()
   with self.assertRaises((OSError,ValueError)):m.materialize(self.root,self.db)
   path.write_bytes(data)
 def test_changed_motion_or_attendance_fails_even_if_digest_changes(self):
  for line,new in ((8,'Different attendees'),(128,'No exceptions from consent agenda'),(153,'Commissioner approved unrelated matter'),(156,'The motion failed.')):
   lines=self.lines.copy();lines[line-1]=new;body=('\n'.join(lines)+'\n').encode();digest=m.r.sha(body);(self.root/'extracted'/digest).write_bytes(body);self.result['extracted_sha256']=digest
   with self.db:self.db.execute('UPDATE document_candidates SET result_json=?',(json.dumps(self.result),))
   with patch.object(m,'TEXT',digest),self.assertRaises(ValueError):m.materialize(self.root,self.db)
 def test_research_requires_current_exact_record(self):
  row={'id':m.RECORD,'kind':'meeting_document','historical':False,'jurisdiction_ref':m.REF,'source_url':m.URL,'source_sha256':m.PDF};m.require_research_binding({'records':[row]})
  for key in row:
   with self.subTest(key=key),self.assertRaises(ValueError):m.require_research_binding({'records':[{**row,key:'different'}]})
  with self.assertRaises(ValueError):m.require_research_binding({'records':[row,row]})
if __name__=='__main__':unittest.main()
