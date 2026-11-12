#!/usr/bin/env python3
import importlib.util,json,sys,tempfile,unittest
from datetime import date
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('civic',Path(__file__).with_name('private-civicclerk-discovery.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
def receipt(url,body,kind='text/html',status=200):return {'url':url,'status':status,'sha256':m.r.sha(body),'resolved_ips':['8.8.8.8'],'connected_ip':'8.8.8.8','retrieved_at':'2026-11-12T00:00:00+00:00','content_type':kind}
class Tests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name);(self.root/'content').mkdir(mode=0o700)
  self.ref='us-co-place-0824950';self.index='https://www.erieco.gov/Calendar.aspx?EID=5350';self.portal='https://erieco.portal.civicclerk.com/event/1224/overview'
  self.page=f'<a href="{self.portal}">Agenda</a>'.encode();self.shell=b'<link rel="modulepreload" href="/assets/apis-test.js"><link rel="modulepreload" href="/assets/actions-test.js"><link rel="modulepreload" href="/assets/utils-C7X9g4QK.js">'
  self.assets={'/assets/apis-test.js':b'api fixture','/assets/actions-test.js':b'action fixture'};self.override=patch.object(m,'ASSETS',{k:m.r.sha(v) for k,v in self.assets.items()});self.override.start();self.addCleanup(self.override.stop)
  captures=[(receipt(self.index,self.page),self.page),(receipt(self.portal,self.shell),self.shell)]+[(receipt('https://erieco.portal.civicclerk.com'+k,v,'application/javascript'),v) for k,v in self.assets.items()];m.r.archive(self.root,captures)
  self.entry={'jurisdiction_ref':self.ref,'state':'html_screened','portal_url':self.portal,'portal_sha256':m.r.sha(self.shell),'source_anchor':{'url':self.portal,'index_url':self.index,'index_sha256':m.r.sha(self.page),'anchor_index':0},'directory':{'jurisdiction_ref':self.ref,'state':'directory_site_checked','website_lead':'http://www.erieco.gov/index.cfm'}}
  self.event={'id':12,'eventDate':'2020-01-01T12:00:00Z','eventName':'Council meeting','categoryName':'Council','cpMediaOnly':False,'publishedFiles':[{'fileId':33,'fileType':4,'type':'Minutes','streamUrl':None,'url':'https://evil.example/?token=secret'}]}
 def bind(self,entry=None):return m.bind_source(self.root,entry or self.entry,m.h.receipts(self.root))
 def test_source_chain_and_pinned_assets(self):
  bound=self.bind();self.assertEqual(bound['tenant'],'erieco');self.assertEqual(len(bound['capture_proofs']),4)
 def test_wrong_tenant_or_index_or_missing_receipt_fails(self):
  for changes in ({'portal_url':'https://evil.portal.civicclerk.com/'},{'jurisdiction_ref':'us-co-place-0826600'}):
   with self.subTest(changes=changes),self.assertRaises(ValueError):self.bind({**self.entry,**changes})
  bad=json.loads(json.dumps(self.entry));bad['source_anchor']['anchor_index']=5
  with self.assertRaises(ValueError):self.bind(bad)
  for f in (self.root/'observations').iterdir():
   if json.loads(f.read_text())['url']==self.portal:f.unlink();break
  with self.assertRaises(ValueError):self.bind()
 def test_hidden_anchor_and_modified_asset_fail(self):
  page=f'<div hidden><a href="{self.portal}">Agenda</a></div>'.encode();m.r.archive(self.root,[(receipt(self.index,page),page)])
  bad=json.loads(json.dumps(self.entry));bad['source_anchor']['index_sha256']=m.r.sha(page)
  with self.assertRaises(ValueError):self.bind(bad)
  (self.root/'content'/m.r.sha(self.assets['/assets/apis-test.js'])).write_bytes(b'changed')
  with self.assertRaises(ValueError):self.bind()
 def test_multiple_source_seeds_are_explicit_and_conflicting_tenants_withheld(self):
  with patch.object(m,'bind_source',side_effect=[{'tenant':'erieco'},{'tenant':'erieco'}]):
   selected,errors=m.bind_group(self.root,self.ref,[{},{}],{});self.assertEqual(selected['additional_same_tenant_sources_skipped'],1);self.assertEqual(errors,[])
  with patch.object(m,'bind_source',side_effect=[{'tenant':'erieco'},{'tenant':'other'}]):
   selected,errors=m.bind_group(self.root,self.ref,[{},{}],{});self.assertIsNone(selected);self.assertIn('conflicting',errors[0]['reason'])
 def test_list_is_bounded_past_only_and_excludes_urls(self):
  obj={'hasMore':True,'items':[self.event]};v=m.summarize(json.dumps(obj).encode(),date(2020,1,2));self.assertTrue(v['has_more']);self.assertEqual(v['events'][0]['published_files'][0]['json_path'],'$.items[0].publishedFiles[0]');self.assertNotIn('secret',json.dumps(v));self.assertNotIn('evil.example',json.dumps(v))
  for invalid in ({'hasMore':True,'items':[self.event]*101},{'hasMore':True,'items':[self.event,self.event]},{'hasMore':'true','items':[]},{'hasMore':True,'items':[{**self.event,'eventDate':'2020-01-02T00:00:00Z'}]},{'hasMore':True,'items':[{**self.event,'publishedFiles':self.event['publishedFiles']*2}]}):
   with self.subTest(invalid=invalid),self.assertRaises(ValueError):m.summarize(json.dumps(invalid).encode(),date(2020,1,2))
 def test_duplicate_json_keys_and_wrong_file_type_fail_closed(self):
  with self.assertRaises(ValueError):m.summarize(b'{"hasMore":false,"hasMore":true,"items":[]}',date(2020,1,2))
  bad={**self.event,'publishedFiles':[{'fileId':33,'fileType':1,'type':'Minutes'}]}
  v=m.summarize(json.dumps({'hasMore':False,'items':[bad]}).encode(),date(2020,1,2));self.assertFalse(v['events'][0]['published_files'][0]['file_endpoint_eligible'])
 def test_optional_pdf_fetch_bounded_and_403_is_not_retried(self):
  bound=self.bind();row={'metadata':m.summarize(json.dumps({'hasMore':False,'items':[self.event]}).encode(),date(2020,1,2))};asset=b'utility fixture';out=self.root/'files';out.mkdir();(out/'content').mkdir();calls=[]
  def get(url,hosts,**kw):
   calls.append((url,kw));body=asset if url.endswith(m.UTILITY_PATH) else b'forbidden';status=200 if body==asset else 403
   return [(receipt(url,body,'application/javascript' if status==200 else 'text/html',status),body)]
  with patch.object(m,'UTILITY_SHA',m.r.sha(asset)),patch.object(m.r,'get_robots',return_value=(None,[])),patch.object(m.r,'get',side_effect=get):probe=m.capture_file(out,bound,row,None,'erieco.api.civicclerk.com',m.r.DownloadBudget(10000000),False)
  self.assertEqual(probe['state'],'http_error');self.assertEqual(probe['status'],403);self.assertEqual(len(calls),2);self.assertTrue(all(call[1]['redirects']==0 for call in calls));self.assertEqual(calls[1][1]['maximum'],m.MAX_PDF)
 def test_optional_published_pdf_is_saved_but_not_screened(self):
  bound=self.bind();row={'metadata':m.summarize(json.dumps({'hasMore':False,'items':[self.event]}).encode(),date(2020,1,2))};asset=b'utility fixture';pdf=b'%PDF-fixture';out=self.root/'files';out.mkdir();(out/'content').mkdir();calls=[]
  def get(url,hosts,**kw):
   calls.append((url,kw));body=asset if url.endswith(m.UTILITY_PATH) else pdf
   return [(receipt(url,body,'application/javascript' if body==asset else 'application/pdf'),body)]
  with patch.object(m,'UTILITY_SHA',m.r.sha(asset)),patch.object(m.r,'get_robots',return_value=(None,[])),patch.object(m.r,'get',side_effect=get):probe=m.capture_file(out,bound,row,None,'erieco.api.civicclerk.com',m.r.DownloadBudget(10000000),False)
  self.assertEqual(probe['state'],'pdf_captured_unscreened');self.assertEqual(probe['sha256'],m.r.sha(pdf));self.assertEqual((out/'content'/m.r.sha(pdf)).read_bytes(),pdf);self.assertEqual(len(calls),2);self.assertFalse((out/'research.sqlite3').exists())
 def test_utility_must_be_source_linked_before_any_request(self):
  bound=self.bind();bound['capture_proofs']=[(u,d,p,(b'<html></html>' if u==self.portal else b)) for u,d,p,b in bound['capture_proofs']];row={'metadata':m.summarize(json.dumps({'hasMore':False,'items':[self.event]}).encode(),date(2020,1,2))};out=self.root/'files';out.mkdir();(out/'content').mkdir()
  with patch.object(m.r,'get') as get:
   result=m.capture_file(out,bound,row,None,'erieco.api.civicclerk.com',m.r.DownloadBudget(10000000),False)
  self.assertEqual(result['state'],'error');get.assert_not_called()
 def test_missing_or_malformed_file_metadata_is_not_a_negative(self):
  for value in (False,0,{},''):
   with self.subTest(value=value),self.assertRaises(ValueError):m.summarize(json.dumps({'hasMore':False,'items':[{**self.event,'publishedFiles':value}]}).encode(),date(2020,1,2))
  for value in (None,'absent'):
   event=dict(self.event)
   if value=='absent':event.pop('publishedFiles')
   else:event['publishedFiles']=value
   meta=m.summarize(json.dumps({'hasMore':False,'items':[event]}).encode(),date(2020,1,2));self.assertEqual(meta['events'][0]['published_files_state'],'not_provided')
   out=self.root/'empty';out.mkdir(exist_ok=True);self.assertEqual(m.capture_file(out,{}, {'metadata':meta},None,'erieco.api.civicclerk.com',m.r.DownloadBudget(1000000),False)['state'],'file_metadata_incomplete')
  for field in ('cpMediaOnly','fileType','streamUrl'):
   event=json.loads(json.dumps(self.event))
   if field=='cpMediaOnly':event.pop(field)
   else:event['publishedFiles'][0].pop(field)
   meta=m.summarize(json.dumps({'hasMore':False,'items':[event]}).encode(),date(2020,1,2));self.assertIsNone(meta['events'][0]['published_files'][0]['file_endpoint_eligible'])
   self.assertEqual(m.capture_file(self.root,{}, {'metadata':meta},None,'erieco.api.civicclerk.com',m.r.DownloadBudget(1000000),False)['state'],'file_metadata_incomplete')
  event={**self.event,'publishedFiles':[]};meta=m.summarize(json.dumps({'hasMore':False,'items':[event]}).encode(),date(2020,1,2));self.assertEqual(m.capture_file(self.root,{}, {'metadata':meta},None,'erieco.api.civicclerk.com',m.r.DownloadBudget(1000000),False)['state'],'no_supported_file_in_bounded_list')
 def test_api_host_and_path_are_strictly_derived(self):
  host,url=m.list_url('erieco',date(2020,1,2));self.assertEqual(host,'erieco.api.civicclerk.com');self.assertIn('cursorId=0',url)
  for bad in ('evil.example','erieco@evil','/other','erieco:443'):
   with self.subTest(bad=bad),self.assertRaises(ValueError):m.list_url(bad,date(2020,1,2))
 def run_adapter(self,response_status=200,denied=False,override=False,name='out',fetch=False):
  (self.root/'private-probe-results-v3.json').write_text(json.dumps({'results':[self.entry]}));calls=[]
  class Rules:
   def can_fetch(self,*args):return not denied
  def get(url,hosts,**kw):
   calls.append((url,hosts,kw));data=json.dumps({'hasMore':False,'items':[self.event]}).encode();status=403 if url.endswith(m.UTILITY_PATH) else response_status;return [(receipt(url,data,'application/json',status),data)]
  argv=['adapter','--source-archive',str(self.root),'--output-parent',str(self.root),'--output-name',name,'--before','2020-01-02','--limit','1','--jurisdiction',self.ref]+(['--ignore-robots'] if override else [])+(['--fetch-files'] if fetch else [])
  with patch.object(sys,'argv',argv),patch.object(m.r,'get_robots',return_value=(Rules(),[])),patch.object(m.r,'get',side_effect=get):m.main(None)
  result=json.loads((self.root/name/'report/results.json').read_text());return result,calls
 def test_private_output_and_exact_403_stop(self):
  value,calls=self.run_adapter(response_status=403);row=value['results'][0];self.assertEqual(row['state'],'error');self.assertEqual(row['status'],403);self.assertEqual(len(calls),1);self.assertEqual(calls[0][2]['redirects'],0);self.assertFalse((self.root/'out/research.sqlite3').exists());self.assertEqual((self.root/'out').stat().st_mode&0o777,0o700)
 def test_utility_failure_keeps_valid_list_and_stops(self):
  v,calls=self.run_adapter(fetch=True,name='file-fails');row=v['results'][0];self.assertEqual(row['state'],'list_captured');self.assertEqual(row['file_probe']['state'],'error');self.assertEqual(len(calls),2)
 def test_private_report_is_not_consumed_by_research_export(self):
  self.run_adapter(name='private');out=self.root/'private';self.assertFalse((out/'research.sqlite3').exists());db=m.r.root_store(out)
  try:
   report=out/'research-export.json';m.r.export_report(out,db,report);self.assertEqual(json.loads(report.read_text())['records'],[])
  finally:db.close()
 def test_seed_name_cannot_escape_private_archive(self):
  argv=['adapter','--source-archive',str(self.root),'--output-parent',str(self.root),'--output-name','unsafe','--before','2020-01-02','--seed-name','../escape.json','--jurisdiction',self.ref]
  with patch.object(sys,'argv',argv),patch.object(m.r,'get') as get,self.assertRaises(SystemExit):m.main(None)
  get.assert_not_called();self.assertFalse((self.root/'unsafe').exists())
 def test_explicit_selection_is_bounded_and_unique(self):
  base=['adapter','--source-archive',str(self.root),'--output-parent',str(self.root),'--output-name','selection','--before','2020-01-02']
  for extra in ([],['--jurisdiction',self.ref,'--jurisdiction',self.ref],['--jurisdiction','us-co-place-0000001','--jurisdiction','us-co-place-0000002','--jurisdiction','us-co-place-0000003','--jurisdiction','us-co-place-0000004']):
   with patch.object(sys,'argv',base+extra),patch.object(m.r,'get') as get,self.assertRaises(SystemExit):m.main(None)
   get.assert_not_called()
  self.assertFalse((self.root/'selection').exists())
 def test_missing_explicit_seed_is_reported_without_a_request(self):
  (self.root/'private-probe-results-v3.json').write_text(json.dumps({'results':[self.entry]}))
  argv=['adapter','--source-archive',str(self.root),'--output-parent',str(self.root),'--output-name','missing','--before','2020-01-02','--jurisdiction','us-co-place-0000001']
  with patch.object(sys,'argv',argv),patch.object(m.r,'get') as get:m.main(None)
  result=json.loads((self.root/'missing/report/results.json').read_text());self.assertEqual(result['results'],[]);self.assertEqual(result['errors'][0]['reason'],'requested source seed unavailable');get.assert_not_called()
 def test_robot_override_is_explicit_and_recorded(self):
  v,calls=self.run_adapter(denied=True,name='blocked');self.assertEqual(len(calls),0);self.assertEqual(v['results'][0]['state'],'error')
  v,calls=self.run_adapter(denied=True,override=True,name='override');self.assertEqual(len(calls),1);self.assertTrue(v['results'][0]['robots_override']);self.assertEqual(v['results'][0]['state'],'list_captured')
if __name__=='__main__':unittest.main()
