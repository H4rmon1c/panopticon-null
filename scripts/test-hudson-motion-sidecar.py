#!/usr/bin/env python3
import importlib.util,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('bv',Path(__file__).with_name('hudson-motion-sidecar.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
def receipt(url,body):return {'url':url,'retrieved_at':'2026-11-11T00:00:00+00:00','status':200,'resolved_ips':['8.8.8.8'],'connected_ip':'8.8.8.8','sha256':m.r.sha(body),'bytes':len(body)}
class Tests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name);self.db=m.r.root_store(self.root);self.addCleanup(self.db.close)
  pdf=b'%PDF-fixture';index=(''.join(f'<a href="/other/{n}">Other</a>' for n in range(107))+f'<a href="{m.URL}">Minutes</a>').encode();current=index+b'<!-- updated -->'
  self.pages=['empty']*5;self.pages[0]=m.ROSTER;self.pages[3]=m.SCOPE+' '+m.MOTION
  text=('\n'.join(self.pages)+'\n').encode();self.pdf=m.r.sha(pdf);self.index=m.r.sha(index);self.current=m.r.sha(current);self.text=m.r.sha(text)
  o=patch.multiple(m,PDF=self.pdf,INDEX_SHA=self.index,TEXT=self.text);o.start();self.addCleanup(o.stop)
  self.doc=receipt(m.URL,pdf);self.oldrec=receipt(m.INDEX,index);self.newrec=receipt(m.INDEX,current);m.r.archive(self.root,[(self.doc,pdf),(self.oldrec,index),(self.newrec,current)])
  (self.root/'extracted').mkdir();(self.root/'extracted'/self.text).write_bytes(text)
  self.old={'url':m.URL,'index_url':m.INDEX,'index_sha256':self.index,'anchor_index':107,'kind':'minutes','label':''};self.new={**self.old,'index_sha256':self.current};self.result={'candidate':self.old,'document_sha256':self.pdf,'extracted_sha256':self.text,'receipt':self.doc}
  with self.db:
   self.db.execute('INSERT INTO discovery VALUES (?,?,?,?)',(m.REF,'https://hudsonco.gov/','2026-11-11T00:00:00+00:00','{}'))
   self.db.execute("INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at) VALUES (?,?,?,'matched',?,?)",(m.REF,m.URL,json.dumps(self.new),json.dumps(self.result),'2026-11-11T00:00:00+00:00'))
 def test_roll_call_scope_and_unrecorded_attendee(self):
  value=m.materialize(self.root,self.db);self.assertEqual(value['outcome'],'Motion carried; six recorded ayes, no nays');self.assertEqual(len(value['votes']),6);self.assertTrue(all(v['name']!='Zach Reyes' for v in value['votes']));self.assertIn('not recorded',value['unrecorded_vote']);self.assertIn('nonrenewal',value['notice']);self.assertEqual(value['source']['motion_page'],4)
 def test_all_three_receipts_required(self):
  for rec in (self.doc,self.oldrec,self.newrec):
   raw=(json.dumps(rec,sort_keys=True,separators=(',',':'))+'\n').encode();path=self.root/'observations'/(m.r.sha(raw)+'.json');body=path.read_bytes();path.unlink()
   with self.assertRaises((OSError,ValueError)):m.materialize(self.root,self.db)
   path.write_bytes(body)
 def test_exact_roster_scope_and_item_vote_required(self):
  for page,old,new in ((4,m.SCOPE,'different context'),(4,m.MOTION,'different vote'),(1,m.ROSTER,'different roster')):
   pages=self.pages.copy();pages[page-1]=pages[page-1].replace(old,new);body=('\n'.join(pages)+'\n').encode();digest=m.r.sha(body);(self.root/'extracted'/digest).write_bytes(body);self.result['extracted_sha256']=digest
   with self.db:self.db.execute('UPDATE document_candidates SET result_json=?',(json.dumps(self.result),))
   with patch.object(m,'TEXT',digest),self.assertRaises(ValueError):m.materialize(self.root,self.db)
 def test_missing_document_and_current_candidate_withhold(self):
  path=self.root/'content'/self.pdf;body=path.read_bytes();path.unlink()
  with self.assertRaises((OSError,ValueError)):m.materialize(self.root,self.db)
  path.write_bytes(body)
  with self.db:self.db.execute("UPDATE document_candidates SET state='stale'")
  with self.assertRaises(ValueError):m.materialize(self.root,self.db)
 def test_current_index_link_must_remain_bound(self):
  bad=b'<a href="/elsewhere">Minutes</a>';digest=m.r.sha(bad);m.r.archive(self.root,[(receipt(m.INDEX,bad),bad)])
  self.new['index_sha256']=digest
  with self.db:self.db.execute('UPDATE document_candidates SET candidate_json=?',(json.dumps(self.new),))
  with self.assertRaises(ValueError):m.materialize(self.root,self.db)
 def test_changed_source_record_withholds(self):
  row={'id':m.RECORD,'kind':'meeting_document','historical':False,'jurisdiction_ref':m.REF,'source_url':m.URL,'source_sha256':m.PDF};m.require_research_binding({'records':[row]})
  for key in row:
   with self.subTest(key=key),self.assertRaises(ValueError):m.require_research_binding({'records':[{**row,key:'changed'}]})
  with self.assertRaises(ValueError):m.require_research_binding({'records':[row,row]})
if __name__=='__main__':unittest.main()
