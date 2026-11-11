#!/usr/bin/env python3
"""Real sandbox regression. Set PNUL_TEST_BINARY and PNUL_TESSDATA_DIR."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import shutil
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
BINARY = os.environ.get('PNUL_TEST_BINARY')

def page_command(root, digest, page, tmp_size='134217728'):
    scope=['/usr/bin/systemd-run','--user','--scope','--quiet','--property=MemoryMax=512M',
           '--property=MemorySwapMax=0','--property=TasksMax=16','--property=CPUQuota=100%',
           '--property=RuntimeMaxSec=60','--']
    outer=['/usr/bin/bwrap','--unshare-user','--unshare-pid','--unshare-net','--unshare-ipc','--unshare-uts',
           '--die-with-parent','--new-session','--size','1048576','--tmpfs','/',
           '--ro-bind','/usr','/usr','--symlink','usr/lib','/lib','--symlink','usr/lib','/lib64',
           '--symlink','usr/bin','/bin','--proc','/proc','--dev','/dev','--size',tmp_size,'--tmpfs','/tmp',
           '--ro-bind','/sys/fs/cgroup','/sys/fs/cgroup','--ro-bind',BINARY,'/pnull',
           '--ro-bind',str(root),'/archive','--ro-bind',str(ROOT/'configs'),'/configs',
           '--ro-bind',str(ROOT/'rules'),'/rules']
    if Path('/nix/store').is_dir():outer+=['--ro-bind','/nix/store','/nix/store']
    outer+=['--clearenv','--setenv','PATH','/usr/bin:/bin','--setenv','PNUL_TESSDATA_DIR',os.environ['PNUL_TESSDATA_DIR'],
            '--chdir','/','/pnull','--data-dir','/archive','source','screen-research-page',
            '--digest',digest,'--source-url','https://example.gov/minutes.pdf',
            '--jurisdiction-ref','us-co-place-0810105','--retrieved-at','2026-11-11T00:00:00Z','--page',str(page)]
    return scope+outer

@unittest.skipUnless(BINARY, 'set PNUL_TEST_BINARY to the freshly built pnull binary')
class Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not Path(BINARY).is_file():
            raise RuntimeError('PNUL_TEST_BINARY must point to a built pnull binary')
        for program in ('systemd-run','bwrap','pdfinfo','pdftotext','pdftoppm','tesseract','prlimit','unshare'):
            if not shutil.which(program, path='/usr/bin:/bin'):
                raise unittest.SkipTest(program+' is unavailable')
        tess=Path(os.environ.get('PNUL_TESSDATA_DIR','/nonexistent'))/'eng.traineddata'
        if not tess.is_file():raise unittest.SkipTest('English Tesseract data is unavailable')
        scope=['/usr/bin/systemd-run','--user','--scope','--quiet','--property=MemoryMax=512M',
               '--property=MemorySwapMax=0','--property=TasksMax=16','--property=CPUQuota=100%',
               '--property=RuntimeMaxSec=5','--','/usr/bin/true']
        probe=subprocess.run(scope,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=10)
        if probe.returncode:raise unittest.SkipTest('bounded systemd user scope is unavailable')
        namespace=['/usr/bin/bwrap','--unshare-user','--unshare-pid','--unshare-net','--disable-userns',
                   '--tmpfs','/','--ro-bind','/usr','/usr','--symlink','usr/lib','/lib',
                   '--symlink','usr/lib','/lib64','--symlink','usr/bin','/bin','--proc','/proc','--dev','/dev','/usr/bin/true']
        probe=subprocess.run(namespace,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=10)
        if probe.returncode:raise unittest.SkipTest('required bubblewrap user namespaces are unavailable')
    def run_page(self, fixture, page):
        body = (ROOT/'fixtures/pdf'/fixture).read_bytes()
        digest = hashlib.sha256(body).hexdigest()
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); (root/'content').mkdir(); (root/'content'/digest).write_bytes(body)
            env={**os.environ,'PATH':'/usr/bin:/bin'}
            return subprocess.run(page_command(root,digest,page),cwd=ROOT,env=env,text=True,capture_output=True,timeout=40)
    @unittest.skipUnless(os.environ.get('PNUL_TEST_LEGACY_BINARY'), 'set PNUL_TEST_LEGACY_BINARY to demonstrate the older extractor defect')
    def test_legacy_false_negative_regression_fixture(self):
        body=(ROOT/'fixtures/pdf/mixed-native-image.pdf').read_bytes();digest=hashlib.sha256(body).hexdigest()
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'content').mkdir();(root/'content'/digest).write_bytes(body)
            cmd=[os.environ['PNUL_TEST_LEGACY_BINARY'],'--data-dir',str(root),'source','screen-research-pdf',
                 '--digest',digest,'--source-url','https://example.gov/minutes.pdf',
                 '--jurisdiction-ref','us-co-place-0810105','--retrieved-at','2026-11-11T00:00:00Z','--ocr']
            before=subprocess.run(cmd,cwd=ROOT,env={**os.environ,'PATH':'/usr/bin:/bin'},capture_output=True,text=True,timeout=40)
        self.assertEqual(before.returncode,0,before.stderr)
        legacy=json.loads(before.stdout)
        self.assertEqual(legacy['state'],'no_match');self.assertEqual(legacy['extraction_status'],'complete')
        after=self.run_page('mixed-native-image.pdf',2)
        self.assertEqual(after.returncode,0,after.stderr)
        self.assertIn('FLOCK SAFETY CAMERAS',json.loads(after.stdout)['ocr_text'])
    def test_text_and_scanned_pages_are_both_explicitly_ocrd(self):
        one=self.run_page('mixed-native-image.pdf',1)
        self.assertEqual(one.returncode,0,one.stderr)
        text=json.loads(one.stdout)
        self.assertIn('Routine meeting agenda',text['native_text'])
        self.assertTrue(text['ocr_text'].strip())
        self.assertIn('FLOCK SAFETY CAMERAS',text['ocr_text'])
        two=self.run_page('mixed-native-image.pdf',2)
        self.assertEqual(two.returncode,0,two.stderr)
        image=json.loads(two.stdout)
        self.assertEqual(image['page'],2);self.assertEqual(image['page_count'],2)
        self.assertIn('FLOCK SAFETY CAMERAS',image['ocr_text'])
        self.assertEqual(image['state'],'private_provisional_page')
        self.assertNotIn('no_match',image)
    def test_resumable_page_requires_receipts_and_rejects_tampering(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp); archive=root/'private';archive.mkdir(mode=0o700);(archive/'content').mkdir();(archive/'observations').mkdir()
            body=(ROOT/'fixtures/pdf/mixed-native-image.pdf').read_bytes();digest=hashlib.sha256(body).hexdigest()
            index=b'<a href="https://example.gov/minutes.pdf">Agenda PDF</a>'
            idx=hashlib.sha256(index).hexdigest();url='https://example.gov/minutes.pdf'; index_url='https://example.gov/meetings'
            for digest_,raw in ((digest,body),(idx,index)):(archive/'content'/digest_).write_bytes(raw)
            candidate={'url':url,'index_url':index_url,'index_sha256':idx,'anchor_index':0,'kind':'agenda','label':'Agenda PDF'}
            (archive/'screen.json').write_text(json.dumps({'candidate':candidate,'url':url,'document_sha256':digest,'jurisdiction_ref':'us-co-place-0810105'}))
            def receipt(url_,digest_):return {'url':url_,'sha256':digest_,'status':200,'resolved_ips':['8.8.8.8'],
                'connected_ip':'8.8.8.8','retrieved_at':'2026-11-11T00:00:00Z'}
            (archive/'observations'/'one.json').write_text(json.dumps(receipt(url,digest)))
            (archive/'observations'/'two.json').write_text(json.dumps(receipt(index_url,idx)))
            cmd=['python3',str(ROOT/'scripts/private-research-page.py'),'--archive',str(archive),'--private-root',str(archive),'--binary',BINARY,
                 '--tessdata',os.environ['PNUL_TESSDATA_DIR'],'--page','2']
            first=subprocess.run(cmd,cwd=ROOT,capture_output=True,text=True,timeout=90)
            self.assertEqual(first.returncode,0,first.stderr)
            output=archive/'ocr-pages'/'0002.json'; saved=json.loads(output.read_text())
            self.assertIn('FLOCK SAFETY CAMERAS',saved['result']['ocr_text'])
            again=subprocess.run(cmd,cwd=ROOT,capture_output=True,text=True,timeout=90)
            self.assertEqual(again.returncode,0,again.stderr);self.assertIn('verified and preserved',again.stdout)
            rootfd=os.open(archive,os.O_RDONLY|os.O_DIRECTORY);archivefd=os.open(archive,os.O_RDONLY|os.O_DIRECTORY)
            try:
                inherited=cmd+['--private-root-fd',str(rootfd),'--archive-fd',str(archivefd)]
                verified=subprocess.run(inherited,cwd=ROOT,capture_output=True,text=True,timeout=90,pass_fds=(rootfd,archivefd))
                self.assertEqual(verified.returncode,0,verified.stderr);self.assertIn('verified and preserved',verified.stdout)
            finally:os.close(archivefd);os.close(rootfd)
            saved['result']['ocr_text']='fabricated result';output.write_text(json.dumps(saved))
            tampered=subprocess.run(cmd,cwd=ROOT,capture_output=True,text=True,timeout=90)
            self.assertNotEqual(tampered.returncode,0)
            self.assertEqual(json.loads(output.read_text())['result']['ocr_text'],'fabricated result')
            output.unlink()
            for name in ('content','observations','ocr-pages'):
                actual=archive/name; moved=archive/(name+'-moved');actual.rename(moved)
                actual.symlink_to(moved,target_is_directory=True)
                try:
                    unsafe=subprocess.run(cmd,cwd=ROOT,capture_output=True,text=True,timeout=90)
                    self.assertNotEqual(unsafe.returncode,0, f'{name} symlink accepted')
                    self.assertFalse((moved/'0002.json').exists())
                finally:actual.unlink();moved.rename(actual)
            (archive/'observations'/'two.json').unlink()
            missing=subprocess.run(cmd,cwd=ROOT,capture_output=True,text=True,timeout=90)
            self.assertNotEqual(missing.returncode,0);self.assertFalse(output.exists())
    def test_inherited_archive_swap_fails_before_worker_launch(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)/'private';root.mkdir(mode=0o700);archive=root/'archive';archive.mkdir(mode=0o700)
            rootfd=os.open(root,os.O_RDONLY|os.O_DIRECTORY);archivefd=os.open(archive,os.O_RDONLY|os.O_DIRECTORY)
            try:
                archive.rename(root/'old');archive.mkdir(mode=0o700)
                cmd=['python3',str(ROOT/'scripts/private-research-page.py'),'--archive',str(archive),'--private-root',str(root),
                     '--private-root-fd',str(rootfd),'--archive-fd',str(archivefd),'--binary',BINARY,
                     '--tessdata',os.environ['PNUL_TESSDATA_DIR'],'--page','1']
                result=subprocess.run(cmd,cwd=ROOT,capture_output=True,text=True,timeout=30,pass_fds=(rootfd,archivefd))
                self.assertNotEqual(result.returncode,0);self.assertFalse((archive/'ocr-pages').exists())
            finally:os.close(archivefd);os.close(rootfd)
    def test_unbounded_cgroup_is_refused_before_extraction(self):
        body=(ROOT/'fixtures/pdf/mixed-native-image.pdf').read_bytes();digest=hashlib.sha256(body).hexdigest()
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'content').mkdir();(root/'content'/digest).write_bytes(body)
            cmd=['/usr/bin/systemd-run','--user','--scope','--quiet','--property=MemoryMax=infinity',
                 '--property=TasksMax=infinity','--property=CPUQuota=infinity','--',BINARY,
                 '--data-dir',str(root),'source','screen-research-page','--digest',digest,
                 '--source-url','https://example.gov/minutes.pdf','--jurisdiction-ref','us-co-place-0810105',
                 '--retrieved-at','2026-11-11T00:00:00Z','--page','2']
            result=subprocess.run(cmd,cwd=ROOT,env={**os.environ,'PATH':'/usr/bin:/bin'},capture_output=True,text=True,timeout=30)
        self.assertNotEqual(result.returncode,0)
        self.assertNotIn('private_provisional_page',result.stdout)
    def test_oversized_tmpfs_is_refused(self):
        body=(ROOT/'fixtures/pdf/mixed-native-image.pdf').read_bytes();digest=hashlib.sha256(body).hexdigest()
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'content').mkdir();(root/'content'/digest).write_bytes(body)
            result=subprocess.run(page_command(root,digest,2,tmp_size='268435456'),cwd=ROOT,
                env={**os.environ,'PATH':'/usr/bin:/bin'},capture_output=True,text=True,timeout=30)
        self.assertNotEqual(result.returncode,0)
        self.assertIn('tmpfs must be at most 128 MiB',result.stderr)
        self.assertNotIn('private_provisional_page',result.stdout)
    def test_scope_runtime_limit_tears_down_tmpfs_job(self):
        cmd=['/usr/bin/systemd-run','--user','--scope','--quiet','--property=MemoryMax=512M',
             '--property=MemorySwapMax=0','--property=TasksMax=16','--property=CPUQuota=100%',
             '--property=RuntimeMaxSec=2','--','/usr/bin/bwrap','--unshare-user','--unshare-pid',
             '--unshare-net','--die-with-parent','--new-session','--size','1048576','--tmpfs','/',
             '--ro-bind','/usr','/usr','--symlink','usr/lib','/lib','--symlink','usr/lib','/lib64',
             '--symlink','usr/bin','/bin','--dev','/dev','--proc','/proc','--size','2097152','--tmpfs','/tmp',
             '/usr/bin/sh','-c','echo scratch-ready; echo data >/tmp/scratch; exec sleep 10']
        started=time.monotonic(); result=subprocess.run(cmd,capture_output=True,text=True,timeout=8)
        self.assertIn('scratch-ready',result.stdout)
        self.assertNotEqual(result.returncode,0)
        self.assertLess(time.monotonic()-started,8)
    def test_empty_ocr_and_out_of_range_fail_closed(self):
        for fixture,page in [('empty-ocr-page.pdf',1),('mixed-native-image.pdf',3),('mixed-native-image.pdf',0)]:
            with self.subTest(fixture=fixture,page=page):
                result=self.run_page(fixture,page)
                self.assertNotEqual(result.returncode,0)
                self.assertNotIn('private_provisional_page',result.stdout)

if __name__=='__main__':unittest.main()
