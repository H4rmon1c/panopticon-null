#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import unittest
import tempfile
import shutil
import subprocess
import json
spec=importlib.util.spec_from_file_location('large',Path(__file__).with_name('colorado_large_pdf_private.py'))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)

class Tests(unittest.TestCase):
    def worker(self, *, pages=102, image=False, empty=False, missing=False, hit=26):
        def run(pdf, tool, args):
            if tool=='pdfinfo': return f'Pages: {pages}\n'
            if tool=='pdfimages':
                return 'page num type\n------------\n'+(' 1 0 image\n' if image else '')
            first=int(args[args.index('-f')+1]);last=int(args[args.index('-l')+1]); out=[]
            for page in range(first,last+1):
                out.append('Flock safety contract' if page==hit else ('' if empty and page==2 else 'ordinary page'))
            if missing: out.pop()
            return '\f'.join(out)+'\f'
        return run
    def test_multi_segment_page_locator(self):
        result=m.screen('unused',self.worker())
        self.assertEqual(result['state'],'private_keyword_lead')
        self.assertEqual(result['pages'],102)
        self.assertEqual(result['findings'][0]['page'],26)
        self.assertEqual(result['findings'][0]['line'],1)
    def test_empty_image_and_incomplete_fail_closed(self):
        for kwargs in ({'image':True},{'empty':True},{'missing':True}):
            with self.subTest(kwargs=kwargs),self.assertRaises(ValueError):m.screen('unused',self.worker(**kwargs))
    def test_bounds_and_bad_inventory(self):
        for count in (0,100,301):
            with self.subTest(count=count),self.assertRaises(ValueError):m.screen('unused',self.worker(pages=count))
        def bad(pdf,tool,args): return 'Pages: 102\n' if tool=='pdfinfo' else 'unparseable list'
        with self.assertRaises(ValueError):m.screen('unused',bad)
    def test_no_keyword_requires_every_nonempty_text_page(self):
        result=m.screen('unused',self.worker(hit=-1))
        self.assertEqual(result['state'],'private_extracted_text_no_keyword')
        self.assertEqual(result['findings'],[])
    def test_multihop_candidates_are_rejected(self):
        m.require_direct_candidate({'url':'https://example.gov/file.pdf'})
        for key in ('springs_source','redirect_receipt','indexed_redirect_receipt',
                    'linked_url','origin_url','capture_adjustment'):
            with self.subTest(key=key),self.assertRaises(ValueError):
                m.require_direct_candidate({key:None})
        with self.assertRaises(ValueError):m.require_direct_candidate(None)
    def test_both_index_and_pdf_receipts_are_required_and_bound(self):
        pairs=[('https://example.gov/page','a'*64),('https://example.gov/file','b'*64)]
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); (root/'observations').mkdir()
            def receipt(pair):
                return {'url':pair[0], 'sha256':pair[1], 'status':200, 'resolved_ips':['8.8.8.8'],
                        'connected_ip':'8.8.8.8','retrieved_at':'2026-11-01T00:00:00+00:00'}
            (root/'observations'/'pdf.json').write_text(json.dumps(receipt(pairs[1])))
            with self.assertRaises(ValueError):m.find_receipts(root,pairs)
            (root/'observations'/'index.json').write_text(json.dumps(receipt(pairs[0])))
            self.assertEqual(set(m.find_receipts(root,pairs)),set(pairs))
            forged=receipt(pairs[0]);forged['connected_ip']='127.0.0.1'
            (root/'observations'/'index.json').write_text(json.dumps(forged))
            with self.assertRaises(ValueError):m.find_receipts(root,pairs)
    def test_real_sandbox_maps_later_page(self):
        if not shutil.which('bwrap'):
            self.skipTest('bubblewrap is unavailable')
        command=['bwrap','--unshare-all','--die-with-parent','--new-session',
                 '--tmpfs','/','--ro-bind','/usr','/usr','--symlink','usr/lib','/lib',
                 '--symlink','usr/lib','/lib64','--symlink','usr/bin','/bin',
                 '--proc','/proc','--dev','/dev','/usr/bin/true']
        try:
            probe=subprocess.run(command,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,timeout=10)
        except (FileNotFoundError,PermissionError) as error:
            self.skipTest('bubblewrap unavailable: '+str(error))
        detail=probe.stderr.decode('utf8','replace').lower()
        unavailable=('operation not permitted','no permissions to creating new namespace',
                     'user namespaces are not enabled','unprivileged user namespaces')
        if probe.returncode and any(message in detail for message in unavailable):
            self.skipTest('user namespaces unavailable: '+detail[:160])
        self.assertEqual(probe.returncode,0,'sandbox probe failed: '+detail[:300])
        # Minimal 102-page PDF with searchable built-in-font text.
        objects=[b'<< /Type /Catalog /Pages 2 0 R >>', None,
                 b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>']
        kids=[]
        for n in range(1,103):
            page_id=len(objects)+1; stream_id=page_id+1; kids.append(f'{page_id} 0 R')
            text='Flock safety' if n==77 else 'Ordinary meeting'
            stream=f'BT /F1 12 Tf 50 700 Td ({text}) Tj ET'.encode()
            objects.append(f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {stream_id} 0 R >>'.encode())
            objects.append(b'<< /Length '+str(len(stream)).encode()+b' >>\nstream\n'+stream+b'\nendstream')
        objects[1]=('<< /Type /Pages /Count 102 /Kids ['+' '.join(kids)+'] >>').encode()
        data=bytearray(b'%PDF-1.4\n'); offsets=[0]
        for i,obj in enumerate(objects,1):
            offsets.append(len(data)); data.extend(f'{i} 0 obj\n'.encode()+obj+b'\nendobj\n')
        xref=len(data);data.extend(f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode())
        for offset in offsets[1:]:data.extend(f'{offset:010d} 00000 n \n'.encode())
        data.extend(f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'.encode())
        with tempfile.TemporaryDirectory() as tmp:
            pdf=Path(tmp)/'fixture.pdf';pdf.write_bytes(data)
            result=m.screen(pdf)
        self.assertEqual(result['state'],'private_keyword_lead')
        self.assertEqual(result['findings'][0]['page'],77)
if __name__=='__main__':unittest.main()
