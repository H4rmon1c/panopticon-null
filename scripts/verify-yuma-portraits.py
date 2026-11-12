#!/usr/bin/env python3
"""Verify Yuma official page/stylesheet/photo associations from private receipts."""
import argparse, hashlib, importlib.util, json, os, re
from html.parser import HTMLParser
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('lucero',ROOT/'scripts/verify-cortez-lucero-portrait.py');l=importlib.util.module_from_spec(spec);spec.loader.exec_module(l);p=l.p
PAGE='https://yumacounty.net/commissioners/'
PAGE_SHA='2252c4e8e80d1360579835ed809f3d518b6b282cfff66177fb275c6afc0f109e'
CSS='https://yumacounty.net/wp-content/uploads/dynamic_avia/avia_posts_css/post-626.css?ver=ver-1794328682'
CSS_SHA='6bb0df295b0da195ca7c15f0cfb212f7de51397403ea6478365dde0c3694fc57'
ROWS={
 'Scott Weaver':{'label':'av-lxq7d6-d4b888fccaa322982ed0b70867ce2048','cell':'av-l92mfm-e7f7bd5b889263b6d5647762f960749a','offset':1,'url':'https://yumacounty.net/wp-content/uploads/2021/01/Scott-Weaver.jpg','sha':'ca38dda41f1b06cf181598f9f1a19885519f477bc4fede820981cc74a0c48c85','bytes':65774,'asset':'yuma-scott-weaver.jpg'},
 'Adam Gates':{'label':'av-kp7vq2-8eada780f0570803dac6a810bec559c5','cell':'av-kv6mfm-a527c515f85f8d6d7a7d69fe9c8d2966','offset':-1,'url':'https://yumacounty.net/wp-content/uploads/2023/01/Adam-Gates.jpg','sha':'c38f1a54ba33598461b04221531ab5280b1a71b7486d69a49d8c9810f0a67d26','bytes':91411,'asset':'yuma-adam-gates.jpg'},
 'Mike Leerar':{'label':'av-jyfnj6-e7afd53234990d684182d081f23ed0eb','cell':'av-kze2g45g-aef5535b6440bb5c5e4360c3353b1c09','offset':-1,'url':'https://yumacounty.net/wp-content/uploads/2025/01/mike-leerar.jpg','sha':'145320cd515685389dc4f01c879aa3b50053d2149fba36a9f85251cceb796514','bytes':78354,'asset':'yuma-mike-leerar.jpg'},
}
class Page(HTMLParser):
 def __init__(self):super().__init__(convert_charrefs=True);self.depth=0;self.active=None;self.heading=0;self.cells=[];self.links=[]
 def handle_starttag(self,tag,attrs):
  a=dict(attrs)
  if tag=='link' and a.get('href'):self.links.append(a['href'])
  if tag=='div':
   self.depth+=1
   classes=a.get('class','').split()
   if 'flex_cell' in classes:
    if self.active is not None:raise ValueError('nested cell')
    self.active={'depth':self.depth,'classes':classes,'heading':[]}
  if tag=='h2' and self.active is not None:self.heading+=1
 def handle_endtag(self,tag):
  if tag=='h2' and self.heading:self.heading-=1
  if tag=='div':
   if self.active is not None and self.active['depth']==self.depth:
    self.active['heading']=' '.join(''.join(self.active['heading']).split());self.cells.append(self.active);self.active=None
   self.depth-=1
 def handle_data(self,data):
  if self.heading and self.active is not None:self.active['heading'].append(data)

def association(page,css,name,row):
 doc=Page();doc.feed(page.decode('utf8','strict'))
 if doc.links.count(CSS)!=1:raise ValueError('county page stylesheet differs')
 matches=[i for i,cell in enumerate(doc.cells) if cell['heading']==name and row['label'] in cell['classes']]
 if len(matches)!=1:raise ValueError('county name cell differs')
 adjacent=matches[0]+row['offset']
 if not 0<=adjacent<len(doc.cells) or row['cell'] not in doc.cells[adjacent]['classes']:raise ValueError('portrait cell adjacency differs')
 css=css.decode('utf8','strict');pattern=r'\.flex_cell\.'+re.escape(row['cell'])+r'\{([^{}]*)\}'
 rules=re.findall(pattern,css)
 urls=[url for rule in rules for url in re.findall(r'background\s*:\s*url\(([^)]*)\)',rule)]
 if urls!=[row['url']]:raise ValueError('county stylesheet portrait differs')

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
 if found!=set(pairs):raise ValueError('page, stylesheet, or portrait receipt missing')

def verify(archive,name):
 row=ROWS[name];root=p.open_absolute_dir(archive)
 try:
  st=os.fstat(root)
  if st.st_uid!=os.geteuid() or st.st_mode&0o077:raise ValueError('archive must be owner-only')
  content=p.open_dir_at(root,'content');obs=p.open_dir_at(root,'observations')
  try:
   page=p.archive_blob(content,PAGE_SHA,2*1024*1024);css=p.archive_blob(content,CSS_SHA,2*1024*1024);image=p.archive_blob(content,row['sha'],2*1024*1024)
   association(page,css,name,row);receipts(obs,[(PAGE,PAGE_SHA),(CSS,CSS_SHA),(row['url'],row['sha'])])
   if len(image)!=row['bytes'] or l.portraits.jpeg_dimensions(image)!=(300,300):raise ValueError('portrait dimensions differ')
   fd=os.open(ROOT/'web/panopticon-terminal/assets/portraits'/row['asset'],os.O_RDONLY|os.O_NOFOLLOW)
   with os.fdopen(fd,'rb') as f:
    if f.read(len(image)+1)!=image:raise ValueError('asset differs from captured bytes')
  finally:os.close(obs);os.close(content)
 finally:os.close(root)

def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--archive',type=Path,required=True);ap.add_argument('--person',choices=ROWS,required=True);a=ap.parse_args();verify(a.archive,a.person);print('Verified county name, linked CSS, portrait and receipts')
if __name__=='__main__':main()
