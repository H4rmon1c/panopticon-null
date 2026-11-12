#!/usr/bin/env python3
import importlib.util,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('bv',Path(__file__).with_name('buenavista-motion-sidecar.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
def receipt(url,body):return {'url':url,'retrieved_at':'2026-11-11T00:00:00+00:00','status':200,'resolved_ips':['8.8.8.8'],'connected_ip':'8.8.8.8','sha256':m.r.sha(body),'bytes':len(body)}
class Tests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name);self.db=m.r.root_store(self.root);self.addCleanup(self.db.close)
  pdf=b'%PDF-fixture';index=(''.join(f'<a href="/other/{n}">Other</a>' for n in range(76))+f'<a href="{m.URL}">Minutes</a>').encode();current=index+b'<!-- updated -->'
  self.pages=['empty']*9;self.pages[1]=m.ROSTER;self.pages[3]=m.APPOINTMENT+' '+m.OATH;self.pages[5]=m.CONTEXT;self.pages[6]=m.SCOPE+' '+m.MOTION
  text=('\n'.join(self.pages)+'\n').encode();self.pdf=m.r.sha(pdf);self.index=m.r.sha(index);self.current=m.r.sha(current);self.text=m.r.sha(text)
  o=patch.multiple(m,PDF=self.pdf,INDEX_SHA=self.index,TEXT=self.text);o.start();self.addCleanup(o.stop)
  self.doc=receipt(m.URL,pdf);self.oldrec=receipt(m.INDEX,index);self.newrec=receipt(m.INDEX,current);m.r.archive(self.root,[(self.doc,pdf),(self.oldrec,index),(self.newrec,current)])
  (self.root/'extracted').mkdir();(self.root/'extracted'/self.text).write_bytes(text)
  self.old={'url':m.URL,'index_url':m.INDEX,'index_sha256':self.index,'anchor_index':76,'kind':'minutes','label':''};self.new={**self.old,'index_sha256':self.current};self.result={'candidate':self.old,'document_sha256':self.pdf,'extracted_sha256':self.text,'receipt':self.doc}
  with self.db:
   self.db.execute('INSERT INTO discovery VALUES (?,?,?,?)',(m.REF,'https://www.buenavistaco.gov/','2026-11-11T00:00:00+00:00','{}'))
   self.db.execute("INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,result_json,updated_at) VALUES (?,?,?,'matched',?,?)",(m.REF,m.URL,json.dumps(self.new),json.dumps(self.result),'2026-11-11T00:00:00+00:00'))
 def test_roll_call_scope_and_mayor_not_inferred(self):
  value=m.materialize(self.root,self.db);self.assertEqual(value['outcome'],'Carried; four AYES, two NYES');self.assertEqual(len(value['votes']),6);self.assertTrue(all(v['name']!='Libby Fay' for v in value['votes']));self.assertIn('NYES',value['notice']);self.assertIn('does not approve',value['notice']);self.assertEqual(value['source']['appointment_page'],4)
 def test_all_three_receipts_required(self):
  for rec in (self.doc,self.oldrec,self.newrec):
   raw=(json.dumps(rec,sort_keys=True,separators=(',',':'))+'\n').encode();path=self.root/'observations'/(m.r.sha(raw)+'.json');body=path.read_bytes();path.unlink()
   with self.assertRaises((OSError,ValueError)):m.materialize(self.root,self.db)
   path.write_bytes(body)
 def test_sturm_oath_and_exact_item_vote_required(self):
  for page,old,new in ((4,m.OATH,'oath not recorded'),(4,m.APPOINTMENT,'different appointee'),(7,m.SCOPE,'different context'),(7,m.MOTION,'different vote'),(2,m.ROSTER,'different roster')):
   pages=self.pages.copy();pages[page-1]=pages[page-1].replace(old,new);body=('\n'.join(pages)+'\n').encode();digest=m.r.sha(body);(self.root/'extracted'/digest).write_bytes(body);self.result['extracted_sha256']=digest
   with self.db:self.db.execute('UPDATE document_candidates SET result_json=?',(json.dumps(self.result),))
   with patch.object(m,'TEXT',digest),self.assertRaises(ValueError):m.materialize(self.root,self.db)
 def test_changed_source_record_withholds(self):
  row={'id':m.RECORD,'kind':'meeting_document','historical':False,'jurisdiction_ref':m.REF,'source_url':m.URL,'source_sha256':m.PDF};m.require_research_binding({'records':[row]})
  for key in row:
   with self.subTest(key=key),self.assertRaises(ValueError):m.require_research_binding({'records':[{**row,key:'changed'}]})
  with self.assertRaises(ValueError):m.require_research_binding({'records':[row,row]})
if __name__=='__main__':unittest.main()
