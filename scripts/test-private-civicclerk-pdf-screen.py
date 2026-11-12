#!/usr/bin/env python3
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import time
import subprocess
import unittest
from unittest.mock import patch

HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('screen',HERE/'private-civicclerk-pdf-screen.py'); m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
sha=lambda b:hashlib.sha256(b).hexdigest()

class ScreenTest(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.base=Path(self.tmp.name);self.source=self.base/'source';self.capture=self.base/'capture'
  for root in (self.source,self.capture):
   root.mkdir();(root/'content').mkdir();(root/'observations').mkdir()
  self.ref='us-co-place-0856860';self.tenant='pagosaspringsco';self.portal='https://pagosaspringsco.portal.civicclerk.com/'
  self.host,self.listurl=m.c.list_url(self.tenant,m.date.fromisoformat('2026-11-11'))
  event={'id':1419,'eventName':'Town Council','categoryName':'Council','eventDate':'2026-10-20T18:30:00Z','cpMediaOnly':False,'publishedFiles':[{'fileId':6629,'type':'Minutes','fileType':4,'streamUrl':None}]}
  body=json.dumps({'items':[event],'hasMore':False}).encode();pdf=b'%PDF-1.4 synthetic'
  utility='https://'+self.tenant+'.portal.civicclerk.com'+m.c.UTILITY_PATH
  self.lsha,self.lreceipt=self.store(self.listurl,body,'application/json');self.psha,self.preceipt=self.store('https://'+self.host+'/v1/Meetings/GetMeetingFileStream(fileId=6629,plainText=false)',pdf,'application/pdf')
  # The source-bound utility is static; provide it through a patched proof read.
  self.ureceipt='a'*64
  self.row={'jurisdiction_ref':self.ref,'tenant':self.tenant,'portal_url':self.portal,'state':'list_captured','list_url':self.listurl,'sha256':self.lsha,'receipt_sha256':self.lreceipt,'file_probe':{'state':'pdf_captured_unscreened','event_id':1419,'event_json_path':'$.items[0]','file_id':6629,'file_type':'Minutes','file_json_path':'$.items[0].publishedFiles[0]','url':'https://'+self.host+'/v1/Meetings/GetMeetingFileStream(fileId=6629,plainText=false)','sha256':self.psha,'receipt_sha256':self.preceipt,'utility_url':utility,'utility_sha256':m.c.UTILITY_SHA,'utility_receipt_sha256':self.ureceipt}}
  (self.source/'private-probe-results-v3.json').write_text(json.dumps({'results':[{'jurisdiction_ref':self.ref,'state':'html_screened'}]}))
  self.save_report()
  shell=b'<link rel="modulepreload" href="/assets/utils-C7X9g4QK.js">'
  bound={'tenant':self.tenant,'portal_url':self.portal,'source_anchor':{'index_url':'https://example.com/index','index_sha256':'b'*64,'anchor_index':1},'capture_proofs':[(self.portal,sha(shell),('c'*64,{}),shell)]}
  self.bind=patch.object(m.c,'bind_group',return_value=(bound,[]));self.bind.start();self.addCleanup(self.bind.stop)
  original=m.proof
  def proof(root,lookup,url,digest,receipt,mime):
   if url==utility:
    if digest!=m.c.UTILITY_SHA or receipt!=self.ureceipt:raise ValueError('utility mismatch')
    return {},b'utility'
   return original(root,lookup,url,digest,receipt,mime)
  self.patcher=patch.object(m,'proof',side_effect=proof);self.patcher.start();self.addCleanup(self.patcher.stop)
 def store(self,url,body,mime):
  digest=sha(body);(self.capture/'content'/digest).write_bytes(body)
  receipt={'url':url,'sha256':digest,'status':200,'resolved_ips':['8.8.8.8'],'connected_ip':'8.8.8.8','retrieved_at':'2026-11-11T12:00:00+00:00','content_type':mime}
  raw=(json.dumps(receipt,sort_keys=True,separators=(',',':'))+'\n').encode();rd=sha(raw);(self.capture/'observations'/(rd+'.json')).write_bytes(raw)
  return digest,rd
 def save_report(self):
  (self.capture/'results.json').write_text(json.dumps({'schema_version':'pnull-private-civicclerk-v1','cutoff':'2026-11-11','results':[self.row]}))
 def check(self):return m.validate(self.source,self.capture,self.ref)
 def test_matching_source_chain(self):
  result,pdf=self.check();self.assertEqual(result['file_id'],6629);self.assertEqual(pdf,b'%PDF-1.4 synthetic');self.assertEqual(result['source_binding']['proofs'][0]['receipt_sha256'],'c'*64);self.assertEqual(result['utility_binding']['receipt_sha256'],self.ureceipt)
 def test_tenant_event_file_url_and_locator_swaps_fail(self):
  for key,value in [('tenant','other'),('list_url','https://other.invalid/'),('file_probe.event_id',7),('file_probe.file_id',7),('file_probe.url','https://evil.invalid/'),('file_probe.event_json_path','$.items[1]'),('file_probe.file_json_path','$.items[0].publishedFiles[1]'),('file_probe.sha256','0'*64),('file_probe.utility_receipt_sha256','0'*64)]:
   with self.subTest(key=key):
    original=copy.deepcopy(self.row);parent=self.row
    parts=key.split('.')
    for part in parts[:-1]:parent=parent[part]
    parent[parts[-1]]=value;self.save_report()
    with self.assertRaises((ValueError,KeyError)):self.check()
    self.row=original
 def test_missing_or_malformed_receipt_and_redirect_fail(self):
  path=self.capture/'observations'/(self.preceipt+'.json');raw=path.read_bytes()
  for replacement in (None,b'{bad',json.dumps({'status':302}).encode()):
   if replacement is None:path.unlink(missing_ok=True)
   else:path.write_bytes(replacement)
   with self.assertRaises(ValueError):self.check()
   path.write_bytes(raw)
 def test_malformed_reports_fail(self):
  for obj in ([],{}, {'schema_version':'pnull-private-civicclerk-v1','results':[None]}):
   (self.capture/'results.json').write_text(json.dumps(obj))
   with self.assertRaises((ValueError,KeyError,TypeError)):self.check()
 def test_snapshot_rejects_symlink_and_tamper(self):
  root=self.base/'archive';root.mkdir(mode=0o700);(root/'content').mkdir();(root/'observations').mkdir();(root/'private-probe-results-v3.json').write_text('{}')
  (root/'content'/('a'*64)).symlink_to('/etc/passwd');fd=m.open_dir(root)
  try:
   with self.assertRaises(OSError):m.snapshot(fd,self.base/'copy')
  finally:os.close(fd)
 def test_pinned_parent_survives_rename(self):
  old=self.base/'old';old.mkdir(mode=0o700);fd=m.open_dir(old);old.rename(self.base/'moved');old.mkdir(mode=0o700)
  try:
   m.c.h.atomic_output(Path(f'/proc/self/fd/{fd}')/'result',b'{}')
   self.assertTrue((self.base/'moved/result/results.json').is_file());self.assertFalse((old/'result').exists())
  finally:os.close(fd)


@unittest.skipUnless(os.environ.get('PNUL_TEST_PDF_BINARY'), 'set PNUL_TEST_PDF_BINARY for real scoped integration')
class RealScopeTest(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name)
  (self.root/'content').mkdir();raw=(HERE.parent/'fixtures/pdf/mixed-native-image.pdf').read_bytes();digest=sha(raw);(self.root/'content'/digest).write_bytes(raw)
  self.p={'pdf_sha256':digest,'pdf_url':'https://example.com/document.pdf','jurisdiction_ref':'us-co-place-0851800','retrieved_at':'2026-11-11T12:00:00+00:00'}
 def test_real_scoped_synthetic_image_pdf_remains_unverified(self):
  result,text=m.screen(Path(os.environ['PNUL_TEST_PDF_BINARY']),self.root,self.p)
  self.assertEqual(result['state'],'unverified_no_keyword_in_extracted_text');self.assertEqual(result['document_coverage'],'unverified');self.assertNotIn('extraction_status',result);self.assertEqual(result['native_text_status'],'extracted');self.assertTrue(text)
 def test_scope_terminates_sleeping_descendant(self):
  fake=self.root/'fake';fake.write_text('#!/bin/sh\nexec /usr/bin/sleep 61.234\n');fake.chmod(0o755);start=time.monotonic()
  result,text=m.screen(fake,self.root,self.p,runtime=3)
  self.assertEqual(result['state'],'incomplete');self.assertIsNone(text);elapsed=time.monotonic()-start;self.assertGreaterEqual(elapsed,2);self.assertLess(elapsed,12)
  remaining=[]
  for q in Path('/proc').iterdir():
   if q.name.isdigit():
    try:
     if (q/'cmdline').read_bytes().split(b'\0')==[b'/usr/bin/sleep',b'61.234',b'']:remaining.append(q.name)
    except OSError:pass
  self.assertEqual(remaining,[])
 def test_direct_unbounded_extractor_fails_closed(self):
  cmd=[os.environ['PNUL_TEST_PDF_BINARY'],'--data-dir',str(self.root),'--rules',str(HERE.parent/'rules/surveillance.yml'),'source','screen-research-pdf','--bounded-private','--digest',self.p['pdf_sha256'],'--source-url',self.p['pdf_url'],'--jurisdiction-ref',self.p['jurisdiction_ref'],'--retrieved-at',self.p['retrieved_at']]
  result=subprocess.run(cmd,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=10)
  self.assertNotEqual(result.returncode,0);self.assertFalse((self.root/'extracted').exists())

if __name__=='__main__':unittest.main()
