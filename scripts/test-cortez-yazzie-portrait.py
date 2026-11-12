#!/usr/bin/env python3
import hashlib,importlib.util,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[1];sp=importlib.util.spec_from_file_location('yazzie',ROOT/'scripts/verify-cortez-yazzie-portrait.py');m=importlib.util.module_from_spec(sp);sp.loader.exec_module(m)
class Tests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name)
  for n in ('content','observations'):(self.root/n).mkdir(mode=0o700)
  self.profile=(f'<section data-id="77bb280"><img alt="Arlina Yazzi headshot" srcset="{m.IMAGE} 1200w"><h3>Arlina Yazzie</h3><p>Council Member for the City of Cortez</p></section>').encode()
  self.sha=hashlib.sha256(self.profile).hexdigest();(self.root/'content'/self.sha).write_bytes(self.profile)
  (self.root/'content'/m.IMAGE_SHA).write_bytes(m.ASSET.read_bytes())
  self.receipt(m.PROFILE,self.sha);self.receipt(m.IMAGE,m.IMAGE_SHA,True)
  self.mock=patch.object(m,'PROFILE_SHA',self.sha);self.mock.start();self.addCleanup(self.mock.stop)
 def receipt(self,url,sha,newline=False):
  x={'url':url,'status':200,'sha256':sha,'resolved_ips':['8.8.8.8'],'connected_ip':'8.8.8.8','retrieved_at':'2026-11-12T00:00:00+00:00'};raw=json.dumps(x,sort_keys=True,separators=(',',':')).encode()+(b'\n' if newline else b'');(self.root/'observations'/(hashlib.sha256(raw).hexdigest()+'.json')).write_bytes(raw)
 def test_matching_section_and_receipts(self):m.verify(self.root)
 def test_missing_wrong_or_duplicated_section_fails(self):
  for raw in (self.profile.replace(b'Arlina Yazzie',b'Someone Else'),self.profile.replace(b'data-id="77bb280"',b'data-id="other"'),self.profile+self.profile,self.profile.replace(b'Council Member for the City of Cortez',b'Other')):
   with self.assertRaises(ValueError):m.bind_profile(raw)
 def test_changed_image_or_alt_rejected(self):
  for raw in (self.profile.replace(m.IMAGE.encode(),b'https://example.org/other.png'),self.profile.replace(b'Arlina Yazzi headshot',b'Other')):
   with self.assertRaises(ValueError):m.bind_profile(raw)
 def test_missing_receipt_fails(self):
  for f in (self.root/'observations').iterdir():f.unlink();break
  with self.assertRaises(ValueError):m.verify(self.root)
 def test_mutated_archived_image_fails(self):
  path=self.root/'content'/m.IMAGE_SHA;raw=bytearray(path.read_bytes());raw[-1]^=1;path.write_bytes(raw)
  with self.assertRaises(ValueError):m.verify(self.root)
 def test_png_dimensions_enforced(self):
  raw=bytearray(m.ASSET.read_bytes());raw[19]^=1
  with self.assertRaises(ValueError):m.png_info(raw)
if __name__=='__main__':unittest.main()
