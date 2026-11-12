#!/usr/bin/env python3
"""Offline source-binding and hidden-text regressions for private HTML leads."""
import importlib.util,json,tempfile,unittest,subprocess,sys,time,signal
from unittest.mock import patch
from pathlib import Path
spec=importlib.util.spec_from_file_location('private_html',Path(__file__).with_name('private-html-meeting-screen.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
def receipt(url,body):return {'url':url,'status':200,'sha256':m.r.sha(body),'resolved_ips':['8.8.8.8'],'connected_ip':'8.8.8.8','retrieved_at':'2026-11-12T00:00:00+00:00','content_type':'text/html'}
class Test(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name);(self.root/'content').mkdir(mode=0o700)
  self.url='https://www.cityofcripplecreek.com/government/meeting/regular-city-council-meeting-15';self.index='https://www.cityofcripplecreek.com/meetings'
  self.html=b'<html><body><p>Flock Safety cameras discussed</p><div hidden>Axon</div><del>Palantir Technologies</del></body></html>'
  self.page=f'<a href="{self.url}">Meeting</a>'.encode();self.doc_sha=m.r.sha(self.html);self.idx_sha=m.r.sha(self.page)
  m.r.archive(self.root,[(receipt(self.url,self.html),self.html),(receipt(self.index,self.page),self.page)])
  self.discovery=[{'jurisdiction_ref':'us-co-place-0818530','state':'directory_site_checked','website_lead':'http://www.cityofcripplecreek.com/'}]
  self.entry={'jurisdiction_ref':'us-co-place-0818530','state':'screened','source_sha256':self.doc_sha,'source_anchor':{'url':self.url,'index_url':self.index,'index_sha256':self.idx_sha,'anchor_index':0}}
 def analyze(self,entry=None,discovery=None):return m.analyze(self.root,discovery or self.discovery,entry or self.entry,m.receipts(self.root))
 def test_source_chain_and_hidden_deletion(self):
  out=self.analyze();self.assertEqual(out['index_anchor'],0);self.assertEqual(out['sha256'],self.doc_sha);self.assertEqual(len(out['matches']),1);self.assertEqual(out['matches'][0]['signals'],['Flock Safety'])
 def test_excludes_inline_hidden_markup_and_deleted_text(self):
  p=m.CandidateText();p.feed('<p>shown</p><script>Axon</script><div hidden>Axon <b>Flock</b></div><div aria-hidden="true">Axon</div><span style="display: none">Axon</span><span style="visibility:hidden">Axon</span><del>Axon</del><s>Flock</s><p>visible</p>');self.assertEqual(p.nodes,['shown','visible'])
 def test_self_closing_hidden_container_does_not_reveal_descendants(self):
  p=m.CandidateText();p.feed('<div hidden/>Axon <b>Flock Safety</b></div><p>visible</p>');self.assertEqual(p.nodes,['visible'])
 def test_attribute_without_value_does_not_crash(self):
  p=m.CandidateText();p.feed('<p aria-hidden style>visible</p>');self.assertEqual(p.nodes,['visible'])
 def test_deep_hostile_html_is_bounded(self):
  malicious='<div>'*6000+'</not-present>'*6000
  for cls in (m.CandidateText,m.IndexLinks):
   with self.subTest(parser=cls.__name__),self.assertRaises(ValueError):cls().feed(malicious)
 def test_malformed_style_comments_are_bounded_and_conservatively_hidden(self):
  self.assertTrue(m.inline_hidden('/*a'*100000));self.assertTrue(m.inline_hidden('display/**/:none'));self.assertFalse(m.inline_hidden('color:red'))
 def test_receipt_and_match_budgets_fail_closed(self):
  with patch.object(m,'MAX_RECEIPTS',1),self.assertRaises(ValueError):m.receipts(self.root)
  with patch.object(m,'MAX_RECEIPT_BYTES',1),self.assertRaises(OverflowError):m.receipts(self.root)
  with patch.object(m,'MAX_MATCHES',0),self.assertRaises(ValueError):self.analyze()
 def test_receipt_iteration_stops_without_eager_directory_enumeration(self):
  path=next((self.root/'observations').iterdir());seen=[]
  class Entry:
   def __init__(self,path):self.path=str(path)
  class Lazy:
   def __enter__(self):return self
   def __exit__(self,*args):pass
   def __iter__(self):
    for i in range(1000000):
     seen.append(i);yield Entry(path)
  with patch.object(m,'MAX_RECEIPTS',1),patch.object(m.os,'scandir',return_value=Lazy()),self.assertRaises(ValueError):m.receipts(self.root)
  self.assertEqual(len(seen),2)
 def test_real_alarm_during_output_cleans_partial_directory(self):
  out=self.root/'timeout-output';old=signal.signal(signal.SIGALRM,m.deadline_handler)
  try:
   signal.setitimer(signal.ITIMER_REAL,0.03)
   with patch.object(m.os,'fsync',side_effect=lambda fd:time.sleep(1)),self.assertRaises(m.DeadlineExpired):
    m.atomic_output(out,b'{}\n',time.monotonic()+0.03)
  finally:
   signal.setitimer(signal.ITIMER_REAL,0);signal.signal(signal.SIGALRM,old)
  self.assertFalse(out.exists())
 def test_timeout_after_rename_removes_completed_looking_file(self):
  out=self.root/'commit-timeout';real_replace=m.os.replace
  def interrupted(src,dst):real_replace(src,dst);raise m.DeadlineExpired()
  with patch.object(m.os,'replace',side_effect=interrupted),self.assertRaises(m.DeadlineExpired):m.atomic_output(out,b'{}\n')
  self.assertFalse(out.exists())
 def test_repeated_large_discovery_fails_during_bounded_encoding(self):
  shared={'source':'z'*10000};payload={'results':[{'discovery':shared}]*100}
  with patch.object(m,'MAX_OUTPUT',20000),self.assertRaises(ValueError):m.bounded_json(payload)
  self.assertEqual(json.loads(m.bounded_json({'ok':True})),{'ok':True})
 def test_inline_css_hidden_and_hidden_anchor_text(self):
  p=m.CandidateText();p.feed('<p style="display/**/:none">Axon</p><p style="opacity:0">Axon</p><p style="visibility:collapse">Axon</p><p>visible</p>');self.assertEqual(p.nodes,['visible'])
  page=f'<a href="{self.url}"><span hidden>Meeting</span></a>'.encode();sha=m.r.sha(page);m.r.archive(self.root,[(receipt(self.index,page),page)])
  entry=json.loads(json.dumps(self.entry));entry['source_anchor']['index_sha256']=sha
  with self.assertRaises(ValueError):self.analyze(entry)
 def test_duplicate_hidden_attributes_and_ambiguous_href_fail_closed(self):
  text=m.CandidateText();text.feed('<p style="display:none" style="color:red">Axon</p><p aria-hidden="true" aria-hidden="false">Flock</p><p>visible</p>');self.assertEqual(text.nodes,['visible'])
  page=f'<a href="{self.url}" href="https://evil.example/">Meeting</a>'.encode();sha=m.r.sha(page);m.r.archive(self.root,[(receipt(self.index,page),page)])
  entry=json.loads(json.dumps(self.entry));entry['source_anchor']['index_sha256']=sha
  with self.assertRaises(ValueError):self.analyze(entry)
 def test_swapped_identity_and_attacker_url_rejected(self):
  for field,value in [('url','https://evil.example/meeting'),('url',self.url+'?token=abc'),('index_url','https://evil.example/index'),('anchor_index',1)]:
   bad=json.loads(json.dumps(self.entry));bad['source_anchor'][field]=value
   with self.subTest(field=field),self.assertRaises(ValueError):self.analyze(bad)
  bad=json.loads(json.dumps(self.entry));bad['jurisdiction_ref']='us-co-place-0807850'
  with self.assertRaises(ValueError):self.analyze(bad)
 def test_hidden_index_anchor_does_not_establish_source_link(self):
  page=f'<div hidden><a href="{self.url}">Meeting</a></div>'.encode();sha=m.r.sha(page);m.r.archive(self.root,[(receipt(self.index,page),page)])
  entry=json.loads(json.dumps(self.entry));entry['source_anchor']['index_sha256']=sha
  with self.assertRaises(ValueError):self.analyze(entry)
 def test_self_closing_hidden_index_container_fails_closed(self):
  page=f'<div hidden/><a href="{self.url}">Meeting</a></div>'.encode();sha=m.r.sha(page);m.r.archive(self.root,[(receipt(self.index,page),page)])
  entry=json.loads(json.dumps(self.entry));entry['source_anchor']['index_sha256']=sha
  with self.assertRaises(ValueError):self.analyze(entry)
 def test_receipt_reference_is_the_html_receipt_that_passed_validation(self):
  for url,body in ((self.index,self.page),(self.url,self.html)):
   html_hash=m.r.sha((json.dumps(receipt(url,body),sort_keys=True,separators=(',',':'))+'\n').encode())
   for n in range(1000):
    nonhtml={**receipt(url,body),'content_type':'text/plain','variant':n}
    raw=(json.dumps(nonhtml,sort_keys=True,separators=(',',':'))+'\n').encode()
    if m.r.sha(raw)<html_hash:break
   else:self.fail('could not create earlier non-HTML receipt')
   m.r.archive(self.root,[(nonhtml,body)])
  out=self.analyze()
  for k in ('index_receipt_sha256','receipt_sha256'):
   saved=json.loads((self.root/'observations'/(out[k]+'.json')).read_text());self.assertIn('html',saved['content_type'])
 def test_missing_receipt_and_changed_bytes_rejected(self):
  for path in (self.root/'observations').iterdir():
   if json.loads(path.read_bytes())['url']==self.url:path.unlink();break
  with self.assertRaises(ValueError):self.analyze()
  m.r.archive(self.root,[(receipt(self.url,self.html),self.html)])
  (self.root/'content'/self.doc_sha).write_bytes(b'<p>tampered</p>')
  with self.assertRaises(ValueError):self.analyze()
 def test_redirected_official_domain_requires_preserved_redirect_chain(self):
  origin='https://www.townofholly.com/';home='https://www.townofhollyco.gov/';idx=home+'records/board-minutes';url=idx+'/october-7-2026'
  index=f'<a href="{url}">minutes</a>'.encode();body=b'<p>Flock Safety</p>';home_body=b'<h1>Town of Holly</h1>';redirect_body=b''
  redirect={**receipt(origin,redirect_body),'status':301,'redirect_target':home};final=receipt(home,home_body)
  m.r.archive(self.root,[(redirect,redirect_body),(final,home_body),(receipt(idx,index),index),(receipt(url,body),body)])
  discovery=[{'jurisdiction_ref':'us-co-place-0837215','state':'directory_site_checked','website_lead':'http://www.townofholly.com/','canonical_redirects':[redirect],'canonical_url':home,'canonical_receipt':final}]
  entry={'jurisdiction_ref':'us-co-place-0837215','state':'screened','source_sha256':m.r.sha(body),'source_anchor':{'url':url,'index_url':idx,'index_sha256':m.r.sha(index),'anchor_index':0}}
  self.assertEqual(self.analyze(entry,discovery)['url'],url)
  discovery[0]['canonical_redirects']=[{**redirect,'redirect_target':'https://evil.example/'}]
  with self.assertRaises(ValueError):self.analyze(entry,discovery)
 def test_output_is_fresh_private_and_not_consumed_by_research_export(self):
  (self.root/'private-results.json').write_text(json.dumps({'discovery':self.discovery}));(self.root/'private-html-screen-results.json').write_text(json.dumps({'results':[self.entry]}))
  cmd=[sys.executable,str(Path(__file__).with_name('private-html-meeting-screen.py')),'--archive',str(self.root),'--output-name','result']
  run=subprocess.run(cmd,capture_output=True,text=True);self.assertEqual(run.returncode,0,run.stderr)
  out=self.root/'result';self.assertEqual((out.stat().st_mode&0o777),0o700);self.assertEqual(((out/'results.json').stat().st_mode&0o777),0o600);self.assertFalse((out/'research.sqlite3').exists())
  again=subprocess.run(cmd,capture_output=True,text=True);self.assertNotEqual(again.returncode,0)
  db=m.r.root_store(out)
  try:
   report=out/'report.json';m.r.export_report(out,db,report);self.assertEqual(json.loads(report.read_text())['records'],[])
  finally:db.close()
 def test_duplicate_or_changed_discovery_rejected(self):
  with self.assertRaises(ValueError):self.analyze(discovery=self.discovery*2)
  with self.assertRaises(ValueError):self.analyze(discovery=[{**self.discovery[0],'website_lead':'https://evil.example'}])
if __name__=='__main__':unittest.main()
