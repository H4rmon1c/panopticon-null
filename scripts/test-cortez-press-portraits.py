#!/usr/bin/env python3
import hashlib,importlib.util,json,os,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('press',ROOT/'scripts/verify-cortez-press-portraits.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
URL='https://imengine.public.prod.dur.navigacloud.com/?uuid=test'

class Verify(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name)
  for x in ('content','observations'): (self.root/x).mkdir(mode=0o700)
 def tearDown(self):self.tmp.cleanup()
 def receipt(self,url,digest,newline=False):
  item={'url':url,'status':200,'sha256':digest,'resolved_ips':['8.8.8.8'],'connected_ip':'8.8.8.8','retrieved_at':'2026-11-12T00:00:00+00:00'}
  raw=json.dumps(item,sort_keys=True,separators=(',',':')).encode()+(b'\n' if newline else b'')
  (self.root/'observations'/(hashlib.sha256(raw).hexdigest()+'.json')).write_bytes(raw)
 def fixture(self,name):
  row=dict(m.ROWS[name]);caption=row['caption'];credit=row['credit']
  if row['kind']=='figure':
   article=(f'<figure><img src="{URL}" alt="{caption}"><figcaption><span>{caption}</span><span>{credit}</span></figcaption></figure>').encode()
  else:
   item={'@type':'ImageObject','url':URL,'contentUrl':URL,'caption':caption,'creditText':credit}
   article=('<script type="application/ld+json">'+json.dumps({'@graph':[item]})+'</script>').encode()
  row['article_sha']=hashlib.sha256(article).hexdigest();(self.root/'content'/row['article_sha']).write_bytes(article)
  img=(ROOT/'web/panopticon-terminal/assets/portraits'/row['asset']).read_bytes();(self.root/'content'/row['image_sha']).write_bytes(img)
  self.receipt(row['article'],row['article_sha']);self.receipt(URL,row['image_sha'],True)
  return row,article
 def test_three_individual_sources(self):
  for name in m.ROWS:
   with self.subTest(name=name):
    row,article=self.fixture(name)
    with patch.dict(m.ROWS,{name:row}):m.verify(self.root,name)
 def test_wrong_caption_and_ambiguous_figure_rejected(self):
  row,article=self.fixture('Amy Huckins')
  with self.assertRaises(ValueError):m.source_image(article.replace(b'Huckins',b'Someone'),row)
  with self.assertRaises(ValueError):m.source_image(article+article,row)
 def test_noncanonical_and_wrong_receipt_fail(self):
  row,article=self.fixture('Rachel Medina')
  for p in (self.root/'observations').iterdir():p.rename(p.with_name('wrong-name.json'));break
  with patch.dict(m.ROWS,{'Rachel Medina':row}):
   with self.assertRaises(ValueError):m.verify(self.root,'Rachel Medina')
 def test_changed_asset_fails(self):
  name='Robert Dobry';row,article=self.fixture(name)
  assets=self.root/'assets';assets.mkdir();(assets/row['asset']).write_bytes(b'wrong')
  with patch.dict(m.ROWS,{name:row}):
   with self.assertRaises(ValueError):m.verify(self.root,name,assets)
 def test_host_scope_rejected(self):
  row,article=self.fixture('Amy Huckins')
  with self.assertRaises(ValueError):m.source_image(article.replace(b'imengine.public.prod.dur.navigacloud.com',b'bad.example'),row)
if __name__=='__main__':unittest.main()
