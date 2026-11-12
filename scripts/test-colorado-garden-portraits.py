#!/usr/bin/env python3
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('portraits',Path(__file__).with_name('colorado_garden_portraits.py'))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)

class Tests(unittest.TestCase):
    def profile(self,name,role,url):
        return f'<article id="poc"><header><h2>{name}</h2></header><section class="profile-fields"><strong>Position: </strong>{role}</section><div class="body"><figure><img src="{url}"></figure></div></article>'.encode()
    def jpeg(self,variant=0):
        return b'\xff\xd8\xff\xc0\x00\x0b\x08\x00\x50\x00\x60\x03\x01\x02\x03\xff\xd9'+bytes([variant])
    def test_profile_must_bind_name_role_and_one_image(self):
        url='https://streamline.imgix.net/a/photo.jpg?signature=secret'
        raw=self.profile('Alex Lopez','Trustee',url)
        self.assertEqual(m.profile_image(raw,'Alex Lopez','Trustee'),url)
        for altered in (raw.replace(b'Alex Lopez',b'Other'),raw.replace(b'Trustee',b'Mayor'),raw.replace(b'</figure>',b'<img src="https://example.org/x"></figure>'),raw.replace(b'streamline.imgix.net',b'example.org')):
            with self.assertRaises(ValueError):m.profile_image(altered,'Alex Lopez','Trustee')
    def test_jpeg_markers_size_and_dimensions(self):
        data=self.jpeg();self.assertEqual(m.jpeg_dimensions(data),(96,80))
        for bad in (b'PNG',data[:8],b'\xff\xd8\xff\xc0\xff\xff'+b'x'*8,b'\xff\xd8\xff\xc0\x00\x0b\x08\xff\xff\x00\x60\x03\x01\x02\x03'):
            with self.assertRaises(ValueError):m.jpeg_dimensions(bad)
    def test_verified_receipts_and_sanitized_original_bytes(self):
        names={'Fil Archuleta':'Mayor','Gary Sorensen':'Mayor Pro Tem','Alex Lopez':'Trustee','Leigh Sorensen':'Trustee','Katherine Rodriguez':'Trustee','Jasmine Marquez':'Trustee'}
        with tempfile.TemporaryDirectory() as tmp:
            base=Path(tmp);archive=base/'private';archive.mkdir(mode=0o700);(archive/'content').mkdir();(archive/'observations').mkdir();rows=[]
            def save(raw):
                h=hashlib.sha256(raw).hexdigest();(archive/'content'/h).write_bytes(raw);return h
            def receipt(url,digest):
                return {'url':url,'sha256':digest,'status':200,'resolved_ips':['8.8.8.8'],'connected_ip':'8.8.8.8','retrieved_at':'2026-11-11T00:00:00Z'}
            for i,(name,role) in enumerate(names.items()):
                profile='https://www.townofgardencity.com/'+name.lower().replace(' ','-');url=f'https://streamline.imgix.net/{i}.jpg?secret=private'
                body=self.profile(name,role,url);image=self.jpeg(i);ph=save(body);ih=save(image)
                for suffix,item in (('profile',receipt(profile,ph)),('image',receipt(url,ih))):
                    (archive/'observations'/f'{i}-{suffix}.json').write_text(json.dumps(item))
                rows.append({'name':name,'profile_role_at_capture':role,'source_profile':profile,'profile_sha256':ph,'image_sha256':ih,'image_bytes':len(image),'width':96,'height':80,'source_notice':'Town notice'})
            proof={'jurisdiction_ref':'us-co-place-0829185','portraits':rows};verified=m.verify(archive,proof);self.assertEqual(len(verified),6)
            out=base/'staged';m.stage(out,verified);manifest=(out/'portrait-manifest.json').read_text();self.assertNotIn('streamline.imgix.net',manifest);self.assertNotIn('?secret=',manifest)
            for row,image in verified:self.assertEqual((out/(row['image_sha256']+'.jpg')).read_bytes(),image)
            with self.assertRaises(FileExistsError):m.stage(out,verified)
            (archive/'observations'/'0-image.json').unlink()
            with self.assertRaises(ValueError):m.verify(archive,proof)
            (archive/'observations'/'0-image.json').write_text(json.dumps(receipt('https://streamline.imgix.net/0.jpg?secret=private',rows[0]['image_sha256'])))
            (archive/'content'/rows[0]['image_sha256']).write_bytes(b'tampered')
            with self.assertRaises(ValueError):m.verify(archive,proof)
    def test_duplicate_and_swapped_identity_cannot_pass(self):
        proof=json.loads(m.PROOF.read_text());proof['portraits'][1]['image_sha256']=proof['portraits'][0]['image_sha256']
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'content').mkdir();(root/'observations').mkdir()
            with self.assertRaises(ValueError):m.verify(root,proof)
if __name__=='__main__':unittest.main()
