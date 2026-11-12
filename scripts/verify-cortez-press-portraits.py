#!/usr/bin/env python3
"""Verify privately captured, exact newspaper image-caption provenance."""
import argparse
import hashlib
import html
from html.parser import HTMLParser
import importlib.util
import json
import os
from pathlib import Path
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('lucero', ROOT/'scripts/verify-cortez-lucero-portrait.py')
lucero = importlib.util.module_from_spec(spec); spec.loader.exec_module(lucero)
p = lucero.p
ROWS = {
 'Amy Huckins': {
  'article':'https://www.the-journal.com/amy-huckins-steps-down-from-city-council-position/',
  'article_sha':'1872b65fee7d9d3021f69d67711e316d444ed7d9e9ef684dfb34bbb23945b844',
  'image_sha':'ed7d705204895df827cdc9c112da49195bc3cf1b13661209ec730f71aa7671ad',
  'asset':'cortez-amy-huckins-2021.jpg','bytes':157541,'dimensions':(800,1055),
  'caption':'Huckins','credit':'The Journal file','kind':'figure',
 },
 'Rachel Medina': {
  'article':'https://www.the-journal.com/articles/meet-rachel-medina-cortezs-new-mayor/',
  'article_sha':'6791b8870819e60f944980b83501e7e89de2741dcaea003673c708cb816fd7af',
  'image_sha':'1ac177816727bf52291eacff0dedf04b141f675e651720da12bcd47a6b207391',
  'asset':'cortez-rachel-medina-2022.jpg','bytes':48057,'dimensions':(719,562),
  'caption':'Rachel Medina is the new mayor of Cortez. (Courtesy photo)','credit':'','kind':'figure',
 },
 'Robert Dobry': {
  'article':'https://www.the-journal.com/articles/news/cortez-city-council-appoints-new-member/',
  'article_sha':'45734998967ac3f40f5c4da881aee4afbf5df2c26bf76db3d4d186b20fd50f77',
  'image_sha':'324f0c80bb0e9828175f5706af1fb851f6c4fa8f1d74c4b9f8d1f02447bd280f',
  'asset':'cortez-robert-dobry-2020.jpg','bytes':45717,'dimensions':(534,852),
  'caption':'Rob Dobry','credit':'The Journal file','kind':'jsonld',
 },
}

class Article(HTMLParser):
 def __init__(self):
  super().__init__(convert_charrefs=True); self.figure=None; self.figures=[]; self.in_caption=0; self.caption=[]; self.scripts=[]; self.script=None
 def handle_starttag(self,tag,attrs):
  attrs=dict(attrs)
  if tag=='figure' and self.figure is None: self.figure={'images':[]};self.caption=[]
  elif tag=='img' and self.figure is not None:self.figure['images'].append(attrs)
  elif tag=='figcaption' and self.figure is not None:self.in_caption+=1
  if tag=='script' and attrs.get('type')=='application/ld+json':self.script=[]
 def handle_endtag(self,tag):
  if tag=='figcaption' and self.in_caption:self.in_caption-=1
  if tag=='figure' and self.figure is not None:
   self.figure['caption']=' '.join(' '.join(self.caption).split());self.figures.append(self.figure);self.figure=None
  if tag=='script' and self.script is not None:self.scripts.append(''.join(self.script));self.script=None
 def handle_data(self,data):
  if self.in_caption:self.caption.append(data)
  if self.script is not None:self.script.append(data)


def source_image(raw,row):
 doc=Article();doc.feed(raw.decode('utf-8','strict'));urls=[]
 if row['kind']=='figure':
  expected=' '.join((row['caption']+' '+row['credit']).split())
  for figure in doc.figures:
   if figure['caption']==expected:
    for image in figure['images']:
     if image.get('alt')==row['caption']:urls.append(image.get('src',''))
 else:
  for script in doc.scripts:
   try: tree=json.loads(script)
   except ValueError:continue
   for item in tree.get('@graph',[]):
    if item.get('@type')=='ImageObject' and item.get('caption')==row['caption'] and item.get('creditText')==row['credit']:
     if item.get('url')==item.get('contentUrl'):urls.append(html.unescape(item['url']))
 if len(urls)!=1:raise ValueError('caption/image binding missing or ambiguous')
 value=urlsplit(urls[0])
 if (value.scheme!='https' or value.hostname!='imengine.public.prod.dur.navigacloud.com' or value.port not in (None,443)
     or value.username or value.password or value.fragment or len(urls[0])>4000):raise ValueError('image source outside expected provider')
 return urls[0]


def receipts(obs,pairs):
 found=set()
 for name in os.listdir(obs):
  try:
   raw=p.read_at(obs,name,65536)
   if name!=hashlib.sha256(raw).hexdigest()+'.json':continue
   item=json.loads(raw); canonical=json.dumps(item,sort_keys=True,separators=(',',':')).encode()
   if raw not in (canonical,canonical+b'\n'):continue
   for url,digest in pairs:
    if p.r.receipt_valid(item,url,digest):found.add((url,digest))
  except (OSError,ValueError,TypeError):continue
 if found!=set(pairs):raise ValueError('canonical article or image receipt missing')


def verify(archive,name,asset_dir=None):
 row=ROWS[name];root=p.open_absolute_dir(archive)
 try:
  st=os.fstat(root)
  if st.st_uid!=os.geteuid() or st.st_mode&0o077:raise ValueError('archive must be owner-only')
  content=p.open_dir_at(root,'content');obs=p.open_dir_at(root,'observations')
  try:
   article=p.archive_blob(content,row['article_sha'],2*1024*1024); image=p.archive_blob(content,row['image_sha'],2*1024*1024)
   url=source_image(article,row);receipts(obs,[(row['article'],row['article_sha']),(url,row['image_sha'])])
   if len(image)!=row['bytes'] or lucero.portraits.jpeg_dimensions(image)!=row['dimensions']:raise ValueError('image dimensions or length differ')
   path=(asset_dir or ROOT/'web/panopticon-terminal/assets/portraits')/row['asset']
   fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
   with os.fdopen(fd,'rb') as f:
    if f.read(len(image)+1)!=image:raise ValueError('asset differs from captured image')
  finally:os.close(obs);os.close(content)
 finally:os.close(root)


def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--archive',type=Path,required=True);parser.add_argument('--person',choices=ROWS,required=True)
 args=parser.parse_args();verify(args.archive,args.person);print('Verified newspaper article, caption, credit, image and receipts')
if __name__=='__main__':main()
