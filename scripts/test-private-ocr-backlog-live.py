#!/usr/bin/env python3
"""Optional real-sandbox integration: set PNUL_TEST_BINARY and PNUL_TESSDATA_DIR."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('backlog',ROOT/'scripts/private-ocr-backlog.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
binary=os.environ.get('PNUL_TEST_BINARY');tessdata=os.environ.get('PNUL_TESSDATA_DIR')

@unittest.skipUnless(binary and tessdata, 'set PNUL_TEST_BINARY and PNUL_TESSDATA_DIR for real isolated OCR')
class RealFreshArchive(unittest.TestCase):
    def test_fresh_archive_runs_through_fd_handoff_and_resumes(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);os.chmod(root,0o700);archive=root/'fixture';archive.mkdir(mode=0o700)
            (archive/'content').mkdir(mode=0o700);(archive/'observations').mkdir(mode=0o700)
            body=(ROOT/'fixtures/pdf/mixed-native-image.pdf').read_bytes()
            digest=hashlib.sha256(body).hexdigest();url='https://example.gov/minutes.pdf'
            index=b'<a href="https://example.gov/minutes.pdf">Agenda PDF</a>'
            index_digest=hashlib.sha256(index).hexdigest();index_url='https://example.gov/meetings'
            for key,data in ((digest,body),(index_digest,index)):(archive/'content'/key).write_bytes(data)
            candidate={'url':url,'index_url':index_url,'index_sha256':index_digest,'anchor_index':0,'kind':'agenda','label':'Agenda PDF'}
            (archive/'screen.json').write_text(json.dumps({'candidate':candidate,'url':url,'document_sha256':digest,'jurisdiction_ref':'us-co-place-0810105'}))
            for n,(source,key) in enumerate(((url,digest),(index_url,index_digest))):
                receipt={'url':source,'sha256':key,'status':200,'resolved_ips':['8.8.8.8'],
                         'connected_ip':'8.8.8.8','retrieved_at':'2026-11-11T00:00:00Z'}
                (archive/'observations'/str(n)).write_text(json.dumps(receipt))
            self.assertFalse((archive/'ocr-pages').exists())
            m.preflight(Path(binary),Path(tessdata))
            fd=os.open(root,m.DIR_FLAGS)
            try:
                jobs=m.parse_jobs(json.dumps({'jobs':[{'archive':'fixture','page':2}]}).encode(),fd)
                state={'queue_sha256':'q','pages':{}}
                result=m.run(jobs,'q',root,fd,state,Path(binary),Path(tessdata),1,150)
                self.assertEqual(result['results'][0]['state'],'worker_succeeded')
                self.assertTrue((archive/'ocr-pages'/'0002.json').is_file())
                cached=m.run(jobs,'q',root,fd,state,Path(binary),Path(tessdata),1,150)
                self.assertEqual(cached['results'][0]['state'],'cached_not_reverified')
            finally:os.close(fd)

if __name__=='__main__':unittest.main()
