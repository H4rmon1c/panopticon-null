#!/usr/bin/env python3
import hashlib,importlib.util,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('yuma_portraits',ROOT/'scripts/verify-yuma-portraits.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
class Tests(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name)
  for x in ('content','observations'):(self.root/x).mkdir(mode=0o700)
 def receipt(self,url,digest,newline=False):
  item={'url':url,'status':200,'sha256':digest,'resolved_ips':['8.8.8.8'],'connected_ip':'8.8.8.8','retrieved_at':'2026-11-12T00:00:00+00:00'}
  raw=json.dumps(item,sort_keys=True,separators=(',',':')).encode()+(b'\n' if newline else b'');path=self.root/'observations'/(hashlib.sha256(raw).hexdigest()+'.json');path.write_bytes(raw);return path
 def fixture(self,name):
  row=m.ROWS[name];label=f'<div class="flex_cell {row["label"]}"><h2>{name}</h2></div>';photo=f'<div class="flex_cell {row["cell"]}"></div>'
  cells=label+photo if row['offset']==1 else photo+label
  page=(f'<link href="{m.CSS}">'+cells).encode();css=f'.flex_cell.{row["cell"]}{{background:url({row["url"]})}}'.encode()
  page_sha=hashlib.sha256(page).hexdigest();css_sha=hashlib.sha256(css).hexdigest()
  for sha,raw in ((page_sha,page),(css_sha,css),(row['sha'],(ROOT/'web/panopticon-terminal/assets/portraits'/row['asset']).read_bytes())):(self.root/'content'/sha).write_bytes(raw)
  self.receipt(m.PAGE,page_sha);self.receipt(m.CSS,css_sha,True);self.receipt(row['url'],row['sha'])
  return row,page,css,page_sha,css_sha
 def test_all_three_exact_bindings(self):
  for name in m.ROWS:
   with self.subTest(name=name):
    row,page,css,ps,cs=self.fixture(name)
    with patch.multiple(m,PAGE_SHA=ps,CSS_SHA=cs):m.verify(self.root,name)
 def test_name_adjacency_and_css_are_enforced(self):
  name='Adam Gates';row,page,css,_,_=self.fixture(name)
  for x,y in ((page.replace(b'Adam Gates',b'Other Person'),css),(page.replace(row['cell'].encode(),b'wrong'),css),(page,css.replace(row['url'].encode(),b'https://evil.example/image.jpg'))):
   with self.assertRaises(ValueError):m.association(x,y,name,row)
 def test_receipts_fail_closed(self):
  row,page,css,ps,cs=self.fixture('Mike Leerar')
  for f in (self.root/'observations').iterdir():f.rename(f.with_name('wrong.json'));break
  with patch.multiple(m,PAGE_SHA=ps,CSS_SHA=cs):
   with self.assertRaises(ValueError):m.verify(self.root,'Mike Leerar')
 def test_duplicate_css_background_fails(self):
  row,page,css,_,_=self.fixture('Scott Weaver')
  with self.assertRaises(ValueError):m.association(page,css+css,'Scott Weaver',row)
if __name__=='__main__':unittest.main()
