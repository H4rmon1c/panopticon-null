#!/usr/bin/env python3
import hashlib
import importlib.util
import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('backlog', Path(__file__).with_name('private-ocr-backlog.py'))
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)

class BacklogTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); os.chmod(self.root, 0o700)
        self.root_fd = os.open(self.root, m.DIR_FLAGS); self.addCleanup(os.close, self.root_fd)
        for name in ('a', 'b', 'c'): (self.root / name).mkdir()
    def queue(self, rows):
        return m.parse_jobs(json.dumps({'jobs': rows}).encode(), self.root_fd)
    def test_rejects_bad_paths_pages_and_duplicates(self):
        outside = self.root / 'outside'; outside.symlink_to('/tmp')
        for rows in ([{'archive':'../tmp','page':1}], [{'archive':'outside','page':1}], [{'archive':'a/','page':1}],
                     [{'archive':'a','page':True}], [{'archive':'a','page':301}],
                     [{'archive':'a','page':1}, {'archive':'a','page':1}]):
            with self.subTest(rows=rows), self.assertRaises((ValueError,OSError)): self.queue(rows)
    def test_preflight_reports_missing_or_unsafe_dependencies(self):
        binary=self.root/'pnull';binary.write_bytes(b'fixture');binary.chmod(0o700)
        tess=self.root/'tess';tess.mkdir()
        with self.assertRaisesRegex(ValueError,'English OCR language data unavailable'):m.preflight(binary,tess)
        (tess/'eng.traineddata').write_bytes(b'data');m.preflight(binary,tess)
        if os.geteuid() != 0:
            binary.chmod(0o001)
            with self.assertRaisesRegex(ValueError,'OCR binary unavailable'):m.preflight(binary,tess)
            binary.chmod(0o700)
        (tess/'eng.traineddata').unlink();(tess/'eng.traineddata').symlink_to('/etc/passwd')
        with self.assertRaisesRegex(ValueError,'English OCR language data unavailable'):m.preflight(binary,tess)
        binary.unlink()
        with self.assertRaisesRegex(ValueError,'OCR binary unavailable'):m.preflight(binary,tess)
    def test_missing_model_fails_cli_before_creating_checkpoint(self):
        binary=self.root/'pnull';binary.write_bytes(b'fixture');binary.chmod(0o700)
        queue=self.root/'queue.json';queue.write_text(json.dumps({'jobs':[{'archive':'a','page':1}]}))
        argv=['private-ocr-backlog.py','--private-root',str(self.root),'--queue',str(queue),
              '--binary',str(binary),'--tessdata',str(self.root/'missing')]
        stderr=io.StringIO()
        with patch.object(sys,'argv',argv),contextlib.redirect_stderr(stderr),self.assertRaises(SystemExit):m.main()
        self.assertIn('English OCR language data unavailable',stderr.getvalue())
        self.assertFalse((self.root/'ocr-backlog-state.json').exists())
    def test_progress_retry_cache_and_changed_artifact(self):
        jobs = self.queue([{'archive':'a','page':1},{'archive':'b','page':2},{'archive':'c','page':3}])
        state = {'queue_sha256':'q','pages':{}}; calls = []
        def execute(root, root_fd, job, binary, tess):
            calls.append(job['archive'])
            if job['archive'] == 'a': return 'incomplete', None, None
            page = m.artifact(root,job); page.parent.mkdir(exist_ok=True); page.write_text(job['archive'])
            digest, identity = m.artifact_identity(root_fd,job)
            return 'worker_succeeded', digest, identity
        r = m.run(jobs, 'q', self.root, self.root_fd, state, Path('/bin/true'), Path('/tmp'), 1, 1200, now=lambda:0, execute=execute)
        self.assertEqual(calls, ['a']); self.assertEqual(r['results'][0]['state'],'incomplete')
        r = m.run(jobs, 'q', self.root, self.root_fd, state, Path('/bin/true'), Path('/tmp'), 1, 1200, now=lambda:0, execute=execute)
        self.assertEqual(calls, ['a','b']); self.assertEqual(r['results'][0]['state'],'worker_succeeded')
        r = m.run(jobs, 'q', self.root, self.root_fd, state, Path('/bin/true'), Path('/tmp'), 1, 1200, now=lambda:0, execute=execute)
        self.assertEqual(calls, ['a','b','c'])
        self.assertIn('cached_not_reverified', [item['state'] for item in r['results']])
        m.artifact(self.root,jobs[1]).write_text('tampered')
        r = m.run(jobs, 'q', self.root, self.root_fd, state, Path('/bin/true'), Path('/tmp'), 3, 1200, now=lambda:0, execute=execute)
        self.assertIn('b', calls[3:])
        self.assertIn('OCR output is provisional',r['notice'])
    def test_queue_digest_and_deadline(self):
        jobs=self.queue([{'archive':'a','page':1}]);state={'queue_sha256':'q','pages':{}}
        with self.assertRaises(ValueError): m.run(jobs,'other',self.root,self.root_fd,state,Path('/bin/true'),Path('/tmp'),1,1200)
        times=iter([0,100,100]);r=m.run(jobs,'q',self.root,self.root_fd,state,Path('/bin/true'),Path('/tmp'),1,150,now=lambda:next(times),execute=lambda *x: self.fail('deadline'))
        self.assertEqual(r['attempted'],0);self.assertEqual(r['results'][0]['state'],'deferred')
    def test_failed_pages_retry_in_fair_order_across_runs(self):
        jobs=self.queue([{'archive':'a','page':1},{'archive':'b','page':1},{'archive':'c','page':1}])
        state={'queue_sha256':'q','pages':{}};calls=[]
        def fail(root,root_fd,job,binary,tess): calls.append(job['archive']);return 'incomplete',None,None
        for _ in range(6):
            m.run(jobs,'q',self.root,self.root_fd,state,Path('/bin/true'),Path('/tmp'),1,1200,now=lambda:0,execute=fail)
        self.assertEqual(calls,['a','b','c','a','b','c'])
    def test_changed_cached_page_is_retried_fairly_with_failures(self):
        jobs=self.queue([{'archive':'a','page':1},{'archive':'b','page':1}])
        page=m.artifact(self.root,jobs[1]);page.parent.mkdir();page.write_text('changed')
        state={'queue_sha256':'q','sequence':2,'pages':{
            '0':{'status':'incomplete','artifact_sha256':None,'last_attempt':1},
            '1':{'status':'worker_succeeded','artifact_sha256':'old','last_attempt':2}}}
        calls=[]
        def fail(root,root_fd,job,binary,tess): calls.append(job['archive']);return 'incomplete',None,None
        for _ in range(2):
            m.run(jobs,'q',self.root,self.root_fd,state,Path('/bin/true'),Path('/tmp'),1,1200,now=lambda:0,execute=fail)
        self.assertEqual(calls,['a','b'])
    def test_symlink_artifact_is_not_trusted(self):
        job=self.queue([{'archive':'a','page':1}])[0];p=m.artifact(self.root,job);p.parent.mkdir();p.symlink_to('/etc/passwd')
        with self.assertRaises(OSError):m.digest_artifact(self.root_fd,job)
    def test_ancestor_swap_after_validation_cannot_redirect_reads(self):
        job=self.queue([{'archive':'a','page':1}])[0]
        page=m.artifact(self.root,job);page.parent.mkdir();page.write_text('inside')
        with tempfile.TemporaryDirectory() as elsewhere:
            outside=Path(elsewhere); (outside/'ocr-pages').mkdir();(outside/'ocr-pages'/'0001.json').write_text('outside')
            (self.root/'a').rename(self.root/'old-a');(self.root/'a').symlink_to(outside, target_is_directory=True)
            with self.assertRaises(OSError):m.digest_artifact(self.root_fd,job)
            with self.assertRaises(OSError):m.read_regular_at(self.root_fd,'a/ocr-pages/0001.json',100)
    def test_replaced_archive_with_identical_artifact_is_not_cached(self):
        job=self.queue([{'archive':'a','page':1}])[0]
        page=m.artifact(self.root,job);page.parent.mkdir();page.write_text('same bytes')
        digest,identity=m.artifact_identity(self.root_fd,job)
        state={'queue_sha256':'q','sequence':1,'pages':{'0':{'status':'worker_succeeded',
               'artifact_sha256':digest,'archive_identity':identity,'last_attempt':1}}}
        (self.root/'a').rename(self.root/'old-a');(self.root/'a').mkdir()
        new=m.artifact(self.root,job);new.parent.mkdir();new.write_text('same bytes')
        calls=[]
        def fail(root,root_fd,job,binary,tess):calls.append(job['archive']);return 'incomplete',None,None
        result=m.run([job],'q',self.root,self.root_fd,state,Path('/bin/true'),Path('/tmp'),1,1200,now=lambda:0,execute=fail)
        self.assertEqual(calls,['a']);self.assertEqual(result['results'][0]['state'],'incomplete')
    def test_worker_handoff_uses_fds_and_detects_archive_swap(self):
        job=self.queue([{'archive':'a','page':1}])[0]
        page=m.artifact(self.root,job);page.parent.mkdir();page.write_text('same bytes')
        def fake_run(cmd,**kwargs):
            root_fd,archive_fd=kwargs['pass_fds']
            self.assertEqual(root_fd,self.root_fd)
            self.assertEqual(os.fstat(archive_fd).st_ino,(self.root/'a').stat().st_ino)
            self.assertIn('--private-root-fd',cmd);self.assertIn('--archive-fd',cmd)
            (self.root/'a').rename(self.root/'old-a');(self.root/'a').mkdir()
            new=m.artifact(self.root,job);new.parent.mkdir();new.write_text('same bytes')
            return SimpleNamespace(returncode=0)
        with patch.object(m.subprocess,'run',side_effect=fake_run):
            self.assertEqual(m.run_job(self.root,self.root_fd,job,Path('/bin/true'),Path('/tmp')),
                             ('incomplete',None,None))
    def test_pinned_root_survives_path_replacement_for_state_and_queue(self):
        (self.root/'queue.json').write_text('inside')
        moved=self.root.with_name(self.root.name+'-moved')
        try:
            self.root.rename(moved);self.root.symlink_to('/tmp',target_is_directory=True)
            self.assertEqual(m.read_regular_at(self.root_fd,'queue.json',100),b'inside')
            m.save_state(self.root_fd,{'safe':True})
            self.assertEqual(json.loads((moved/'ocr-backlog-state.json').read_text()),{'safe':True})
        finally:
            self.root.unlink();moved.rename(self.root)

if __name__ == '__main__': unittest.main()
