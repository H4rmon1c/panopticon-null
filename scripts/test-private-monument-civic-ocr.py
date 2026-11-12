#!/usr/bin/env python3
"""Offline source-chain substitution tests for the one-off Monument OCR adapter."""
import importlib.util,json,os,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('page',Path(__file__).with_name('private-research-page.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
def receipt(url,body,mime):return {'url':url,'status':200,'sha256':m.r.sha(body),'resolved_ips':['8.8.8.8'],'connected_ip':'8.8.8.8','retrieved_at':'2026-11-12T00:00:00+00:00','content_type':mime}
class Tests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name);(self.root/'content').mkdir(mode=0o700)
  self.info=dict(m.MONUMENT);self.bodies={}
  self.bodies['gov']=(''.join('<a href="/other">other</a>' for i in range(21))+f'<a href="{self.info["portal"]}">portal</a>').encode()
  self.bodies['portal']=b'<link rel="modulepreload" href="/assets/apis-WKEMuBZZ.js"><link rel="modulepreload" href="/assets/actions-CMJ3lQwM.js"><link rel="modulepreload" href="/assets/utils-C7X9g4QK.js">'
  for path in (*m.c.ASSETS,m.c.UTILITY_PATH):self.bodies[path]=('fixture:'+path).encode()
  event={'id':557,'eventName':'Town Council Regular Meeting','eventDate':'2026-10-19T18:30:00Z','categoryName':'Town Council','cpMediaOnly':False,'publishedFiles':[{}, {}, {'fileId':4894,'fileType':4,'type':'Minutes','streamUrl':None}]}
  self.event=event;self.bodies['list']=json.dumps({'items':[{}, {},event]},separators=(',',':')).encode();self.bodies['pdf']=b'%PDF-fixture'
  self.info['index_sha']=m.r.sha(self.bodies['list']);self.info['pdf_sha']=m.r.sha(self.bodies['pdf']);self.override=patch.object(m,'MONUMENT',self.info);self.override.start();self.addCleanup(self.override.stop)
  self.proofs=[]
  for key,body in self.bodies.items():
   url=self.info[key] if key in self.info else self.info['portal'].rstrip('/')+key;mime='application/pdf' if key=='pdf' else 'application/json' if key=='list' else 'text/html' if key in ('portal','gov') else 'application/javascript';rec=receipt(url,body,mime);raw=(json.dumps(rec,sort_keys=True,separators=(',',':'))+'\n').encode();self.proofs.append((key,m.r.sha(body),m.r.sha(raw),mime));m.r.archive(self.root,[(rec,body)])
  self.override_proofs=patch.object(m,'MONUMENT_PROOFS',self.proofs);self.override_proofs.start();self.addCleanup(self.override_proofs.stop)
  self.candidate={'source_kind':'civicclerk-monument-2026-v1','url':self.info['pdf'],'index_url':self.info['list'],'index_sha256':self.info['index_sha'],'event_index':2,'file_index':2}
  self.fd=m.open_absolute_dir(self.root);self.cf=m.open_dir_at(self.fd,'content');self.of=m.open_dir_at(self.fd,'observations');self.addCleanup(os.close,self.fd);self.addCleanup(os.close,self.cf);self.addCleanup(os.close,self.of)
 def check(self,candidate=None,url=None,digest=None,ref=None):return m.check_monument_candidate(self.cf,self.of,url or self.info['pdf'],digest or self.info['pdf_sha'],ref or self.info['ref'],candidate or self.candidate)
 def replace(self,key,body):
  i=next(i for i,x in enumerate(self.proofs) if x[0]==key);_,oldsha,oldreceipt,mime=self.proofs[i];url=self.info[key] if key in self.info else self.info['portal'].rstrip('/')+key;rec=receipt(url,body,mime);raw=(json.dumps(rec,sort_keys=True,separators=(',',':'))+'\n').encode();m.r.archive(self.root,[(rec,body)]);self.proofs[i]=(key,m.r.sha(body),m.r.sha(raw),mime)
  if key=='list':self.info['index_sha']=m.r.sha(body);self.candidate['index_sha256']=self.info['index_sha']
 def test_exact_chain(self):
  values=self.check();self.assertEqual(values[(self.info['pdf'],self.info['pdf_sha'])]['url'],self.info['pdf'])
 def test_wrong_tenant_file_reference_and_ref_rejected(self):
  for k,v in [('event_index',1),('file_index',1),('source_kind','other'),('url','https://evil.example/'),('index_url','https://evil.example/')]:
   with self.subTest(k=k),self.assertRaises(ValueError):self.check(candidate={**self.candidate,k:v})
  with self.assertRaises(ValueError):self.check(ref='us-co-place-0000000')
  with self.assertRaises(ValueError):self.check(digest='0'*64)
 def test_missing_and_corrupt_receipt_or_bytes_fail_closed(self):
  for key,sha,receipt_sha,mime in self.proofs:
   path=self.root/'observations'/(receipt_sha+'.json');raw=path.read_bytes();path.unlink()
   with self.subTest(key=key),self.assertRaises((ValueError,OSError)):self.check()
   path.write_bytes(raw)
  path=self.root/'content'/self.info['pdf_sha'];path.write_bytes(b'%PDF-tamper')
  with self.assertRaises(ValueError):self.check()
 def test_rebound_wrong_event_or_file_is_rejected(self):
  for k,v in [('id',558),('categoryName','Other'),('cpMediaOnly',True)]:
   event={**self.event,k:v};body=json.dumps({'items':[{}, {},event]}).encode();self.replace('list',body)
   with self.subTest(k=k),self.assertRaises(ValueError):self.check()
  event={**self.event,'publishedFiles':[{}, {}, {'fileId':4895,'fileType':4,'type':'Minutes','streamUrl':None}]};self.replace('list',json.dumps({'items':[{}, {},event]}).encode())
  with self.assertRaises(ValueError):self.check()
 def test_hidden_or_swapped_government_anchor_rejected(self):
  for target in (self.info['portal'],'https://evil.example/'):
   body=(''.join('<a href="/other">other</a>' for i in range(21))+f'<a href="{target}" hidden>portal</a>').encode();self.replace('gov',body)
   with self.assertRaises(ValueError):self.check()
if __name__=='__main__':unittest.main()
