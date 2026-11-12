#!/usr/bin/env python3
"""Verify exact town-profile portrait evidence and stage original JPEG bytes privately."""
import argparse
import html.parser
import importlib.util
import json
import os
from pathlib import Path
from urllib.parse import urlsplit

ROOT=Path(__file__).resolve().parents[1]
SPEC=importlib.util.spec_from_file_location('private_page',ROOT/'scripts/private-research-page.py')
p=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(p)
PROOF=ROOT/'fixtures/atlas/portrait-sources/garden-city-2026.json'
MAX_IMAGE=2*1024*1024

class Profile(html.parser.HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True);self.article=0;self.h2=0;self.section=0
        self.names=[];self.roles=[];self.images=[];self.articles=0
    def handle_starttag(self,tag,attrs):
        a=dict(attrs)
        if tag=='article' and a.get('id')=='poc':self.article+=1;self.articles+=1
        elif self.article and tag=='article':self.article+=1
        if self.article and tag=='h2':self.h2+=1
        if self.article and tag=='section' and 'profile-fields' in a.get('class','').split():self.section+=1
        if self.article and tag=='img':self.images.append(a)
    def handle_endtag(self,tag):
        if self.article and tag=='h2':self.h2-=1
        if self.article and tag=='section' and self.section:self.section-=1
        if self.article and tag=='article':self.article-=1
    def handle_data(self,text):
        if self.h2:self.names.append(text)
        if self.section:self.roles.append(text)


def profile_image(raw,name,role):
    doc=Profile();doc.feed(raw.decode('utf8','strict'))
    if (doc.articles!=1 or ' '.join(''.join(doc.names).split())!=name
        or ' '.join(''.join(doc.roles).split())!='Position: '+role or len(doc.images)!=1):
        raise ValueError('official profile name, role or portrait differs')
    url=doc.images[0].get('src','');parsed=urlsplit(url)
    if (parsed.scheme!='https' or parsed.hostname!='streamline.imgix.net' or parsed.username or parsed.password
        or parsed.port not in (None,443) or parsed.fragment or len(url)>4000):
        raise ValueError('profile portrait URL is outside expected HTTPS provider')
    return url


def jpeg_dimensions(data):
    if not data.startswith(b'\xff\xd8') or len(data)>MAX_IMAGE:raise ValueError('invalid JPEG bytes')
    i=2
    while i<len(data):
        if data[i]!=0xff:raise ValueError('malformed JPEG marker')
        while i<len(data) and data[i]==0xff:i+=1
        if i>=len(data):break
        marker=data[i];i+=1
        if marker in (0x01,*range(0xd0,0xd9)):continue
        if marker in (0xda,0xd9) or i+2>len(data):break
        size=int.from_bytes(data[i:i+2],'big')
        if size<2 or i+size>len(data):raise ValueError('malformed JPEG segment')
        if marker in (0xc0,0xc1,0xc2,0xc3,0xc5,0xc6,0xc7,0xc9,0xca,0xcb,0xcd,0xce,0xcf):
            if size<8:raise ValueError('short JPEG frame')
            height=int.from_bytes(data[i+3:i+5],'big');width=int.from_bytes(data[i+5:i+7],'big')
            if not 80<=width<=2000 or not 80<=height<=2000:raise ValueError('JPEG dimensions outside bounds')
            return width,height
        i+=size
    raise ValueError('JPEG frame missing')


def verify(archive,proof):
    root=p.open_absolute_dir(archive)
    try:
        info=os.fstat(root)
        if info.st_uid!=os.geteuid() or (info.st_mode&0o077):raise ValueError('archive must be owner-only')
        content=p.open_dir_at(root,'content');obs=p.open_dir_at(root,'observations')
        try:
            rows=proof.get('portraits')
            if proof.get('jurisdiction_ref')!='us-co-place-0829185' or not isinstance(rows,list) or len(rows)!=6:
                raise ValueError('invalid portrait proof')
            expected={'Fil Archuleta':'Mayor','Gary Sorensen':'Mayor Pro Tem','Alex Lopez':'Trustee','Leigh Sorensen':'Trustee','Katherine Rodriguez':'Trustee','Jasmine Marquez':'Trustee'}
            if ({row.get('name') for row in rows}!=set(expected)
                or len({row.get('profile_sha256') for row in rows})!=6
                or len({row.get('image_sha256') for row in rows})!=6):raise ValueError('portrait roster differs')
            pairs=[];result=[]
            for row in rows:
                name=row['name'];role=expected[name];profile=row['source_profile']
                if row['profile_role_at_capture']!=role or profile!='https://www.townofgardencity.com/'+name.lower().replace(' ','-'):
                    raise ValueError('official profile binding differs')
                page=p.archive_blob(content,row['profile_sha256'],2*1024*1024)
                image_url=profile_image(page,name,role)
                image=p.archive_blob(content,row['image_sha256'],MAX_IMAGE)
                width,height=jpeg_dimensions(image)
                if (width,height)!=(row['width'],row['height']) or len(image)!=row['image_bytes']:
                    raise ValueError('pinned portrait dimensions or size differ')
                pairs.extend([(profile,row['profile_sha256']),(image_url,row['image_sha256'])])
                result.append((row,image))
            p.input_receipts(obs,pairs)
            return result
        finally:os.close(obs);os.close(content)
    finally:os.close(root)


def stage(output, rows):
    if not output.is_absolute() or output.name in ('','.','..'):raise ValueError('absolute fresh output required')
    parent=p.open_absolute_dir(output.parent)
    try:
        os.mkdir(output.name,0o700,dir_fd=parent)
        dest=p.open_dir_at(parent,output.name)
        try:
            manifest=[]
            for row,image in rows:
                filename=row['image_sha256']+'.jpg'
                fd=os.open(filename,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=dest)
                with os.fdopen(fd,'wb') as f:f.write(image);f.flush();os.fsync(f.fileno())
                manifest.append({'name':row['name'],'profile_role_at_capture':row['profile_role_at_capture'],
                    'source_profile':row['source_profile'],'profile_sha256':row['profile_sha256'],
                    'image_sha256':row['image_sha256'],'local_filename':filename,'image_bytes':len(image),
                    'width':row['width'],'height':row['height'],'attribution':'Town of Garden City official profile',
                    'source_notice':row['source_notice']})
            raw=(json.dumps({'state':'private_source_bound_originals','portraits':manifest},indent=2)+'\n').encode()
            fd=os.open('portrait-manifest.json',os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=dest)
            with os.fdopen(fd,'wb') as f:f.write(raw);f.flush();os.fsync(f.fileno())
        finally:os.close(dest)
    finally:os.close(parent)


def main():
    a=argparse.ArgumentParser(description=__doc__);a.add_argument('--archive',type=Path,required=True);a.add_argument('--output',type=Path,required=True);args=a.parse_args()
    if not args.archive.is_absolute() or not args.output.is_absolute():a.error('absolute private paths required')
    resolved=args.output.resolve()
    for public in [(ROOT/'web/panopticon-terminal').resolve(),Path('/var/www').resolve(),Path('/srv/www').resolve(),Path('/srv/panopticon').resolve()]:
        if resolved==public or public in resolved.parents:a.error('output cannot be under a public web root')
    proof=json.loads(PROOF.read_text());rows=verify(args.archive,proof);stage(args.output,rows)
    print('Verified and staged',len(rows),'original source-bound portraits')
if __name__=='__main__':main()
