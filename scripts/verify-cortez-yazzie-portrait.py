#!/usr/bin/env python3
"""Verify the preserved CRA scholarship profile and its original named portrait."""
import argparse,hashlib,importlib.util,json,os,struct
from html.parser import HTMLParser
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('lucero',ROOT/'scripts/verify-cortez-lucero-portrait.py');l=importlib.util.module_from_spec(spec);spec.loader.exec_module(l);p=l.p
PROFILE='https://www.cra-online.org/uncategorized/2022-cra-scholarship-program-recipients/'
PROFILE_SHA='519b1af4ef4894ada13a4c8b6d71b3ad73433917aacc57d5fa183cf14292ca6b'
IMAGE='https://www.cra-online.org/wp-content/uploads/2021/06/Arlina-Yazzie-Blog-Photo.png'
IMAGE_SHA='107ff6f3b9219f1052dcc64da05b51fd6f53a3e270efb8869dd2b6bd835c1ffb'
ASSET=ROOT/'web/panopticon-terminal/assets/portraits/cortez-arlina-yazzie-cra.png'
SIZE=1597599
class Profile(HTMLParser):
 def __init__(self):super().__init__(convert_charrefs=True);self.depth=0;self.active=False;self.sections=0;self.head=0;self.heading=[];self.text=[];self.images=[]
 def handle_starttag(self,tag,attrs):
  a=dict(attrs)
  if tag=='section':
   if self.active:self.depth+=1
   elif a.get('data-id')=='77bb280':self.sections+=1;self.active=True;self.depth=1
  if self.active and tag=='h3':self.head+=1
  if self.active and tag=='img':self.images.append(a)
 def handle_endtag(self,tag):
  if self.active and tag=='h3' and self.head:self.head-=1
  if self.active and tag=='section':
   self.depth-=1
   if not self.depth:self.active=False
 def handle_data(self,text):
  if self.active:self.text.append(text)
  if self.active and self.head:self.heading.append(text)

def bind_profile(raw):
 doc=Profile();doc.feed(raw.decode('utf8','strict'));heading=' '.join(''.join(doc.heading).split());body=' '.join(' '.join(doc.text).split())
 if doc.sections!=1 or heading!='Arlina Yazzie' or len(doc.images)!=1:raise ValueError('scholarship section identity is ambiguous')
 if 'Council Member for the City of Cortez' not in body:raise ValueError('profile does not identify Cortez role')
 img=doc.images[0]
 if img.get('alt')!='Arlina Yazzi headshot':raise ValueError('captured alt text differs')
 srcset=[x.strip().rsplit(' ',1) for x in img.get('srcset','').split(',')]
 if srcset.count([IMAGE,'1200w'])!=1:raise ValueError('original portrait URL is not in source section')

def png_info(image):
 if len(image)!=SIZE or image[:8]!=b'\x89PNG\r\n\x1a\n' or image[12:16]!=b'IHDR':raise ValueError('invalid PNG header or size')
 if struct.unpack('>II',image[16:24])!=(1200,1200):raise ValueError('portrait dimensions differ')

def receipts(obs,pairs):
 found=set()
 for name in os.listdir(obs):
  try:
   raw=p.read_at(obs,name,65536)
   if name!=hashlib.sha256(raw).hexdigest()+'.json':continue
   item=json.loads(raw);canonical=json.dumps(item,sort_keys=True,separators=(',',':')).encode()
   if raw not in (canonical,canonical+b'\n'):continue
   for url,digest in pairs:
    if p.r.receipt_valid(item,url,digest):found.add((url,digest))
  except (OSError,ValueError,TypeError):continue
 if found!=set(pairs):raise ValueError('profile or image receipt missing')

def verify(archive):
 root=p.open_absolute_dir(archive)
 try:
  st=os.fstat(root)
  if st.st_uid!=os.geteuid() or st.st_mode&0o077:raise ValueError('archive must be owner-only')
  content=p.open_dir_at(root,'content');obs=p.open_dir_at(root,'observations')
  try:
   profile=p.archive_blob(content,PROFILE_SHA,2*1024*1024);image=p.archive_blob(content,IMAGE_SHA,2*1024*1024)
   bind_profile(profile);png_info(image);receipts(obs,[(PROFILE,PROFILE_SHA),(IMAGE,IMAGE_SHA)])
   fd=os.open(ASSET,os.O_RDONLY|os.O_NOFOLLOW)
   with os.fdopen(fd,'rb') as f:
    if f.read(len(image)+1)!=image:raise ValueError('asset differs from preserved portrait')
  finally:os.close(obs);os.close(content)
 finally:os.close(root)

def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--archive',type=Path,required=True);a=ap.parse_args();verify(a.archive);print('Verified CRA profile, portrait and receipts')
if __name__=='__main__':main()
