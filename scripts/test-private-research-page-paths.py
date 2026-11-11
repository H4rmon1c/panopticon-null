#!/usr/bin/env python3
"""Deterministic archive path-security tests; no sandbox or OCR required."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('private_page',Path(__file__).with_name('private-research-page.py'))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)

class Tests(unittest.TestCase):
    def test_directory_symlinks_and_nondirectories_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'real').mkdir();(root/'file').write_text('file')
            rootfd=os.open(root,os.O_RDONLY|os.O_DIRECTORY)
            try:
                for name in ('content','observations','ocr-pages'):
                    (root/name).symlink_to(root/'real',target_is_directory=True)
                    with self.subTest(name=name),self.assertRaises(OSError):m.open_dir_at(rootfd,name)
                with self.assertRaises(OSError):m.open_dir_at(rootfd,'file')
            finally:os.close(rootfd)
    def test_fd_handoff_validates_identity_and_rejects_replacement(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)/'private';root.mkdir(mode=0o700);archive=root/'archive';archive.mkdir(mode=0o700)
            rootfd=m.open_absolute_dir(root);archivefd=m.open_archive(rootfd,Path('archive'))
            try:
                pinned_root,pinned_archive=m.bind_archive(root,archive,rootfd,archivefd)
                os.close(pinned_root);os.close(pinned_archive)
                with self.assertRaises(ValueError):m.bind_archive(root,archive,rootfd,None)
                archive.rename(root/'moved');archive.mkdir(mode=0o700)
                with self.assertRaises(ValueError):m.bind_archive(root,archive,rootfd,archivefd)
                archive.rmdir();(root/'moved').rename(archive)
                root.rename(Path(tmp)/'old-private');root.mkdir(mode=0o700)
                (root/'archive').mkdir(mode=0o700)
                with self.assertRaises(ValueError):m.bind_archive(root,archive,rootfd,archivefd)
            finally:os.close(archivefd);os.close(rootfd)
    def test_fd_handoff_rejects_non_directory_and_open_permissions(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)/'private';root.mkdir(mode=0o700);archive=root/'archive';archive.mkdir(mode=0o700)
            target=root/'file';target.write_text('x')
            rootfd=m.open_absolute_dir(root);archivefd=m.open_archive(rootfd,Path('archive'));filefd=os.open(target,os.O_RDONLY)
            try:
                with self.assertRaises(ValueError):m.bind_archive(root,archive,rootfd,filefd)
                root.chmod(0o755)
                with self.assertRaises(ValueError):m.bind_archive(root,archive,rootfd,archivefd)
                root.chmod(0o700);archive.chmod(0o755)
                with self.assertRaises(ValueError):m.bind_archive(root,archive,rootfd,archivefd)
            finally:os.close(filefd);os.close(archivefd);os.close(rootfd)
    def test_private_root_ancestor_symlink_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'real').mkdir();(root/'alias').symlink_to(root/'real',target_is_directory=True)
            (root/'real'/'private').mkdir(mode=0o700)
            with self.assertRaises(OSError):m.open_absolute_dir(root/'alias'/'private')
            fd=m.open_absolute_dir(root/'real'/'private');os.close(fd)
    def test_archive_ancestor_swap_cannot_redirect_open_fd(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'private'/'sub'/'archive').mkdir(parents=True)
            (root/'private'/'sub'/'archive'/'marker').write_text('original')
            (root/'attacker'/'archive').mkdir(parents=True);(root/'attacker'/'archive'/'marker').write_text('wrong')
            privatefd=m.open_absolute_dir(root/'private');archivefd=m.open_archive(privatefd,Path('sub/archive'))
            try:
                (root/'private'/'sub').rename(root/'private'/'moved')
                (root/'private'/'sub').symlink_to(root/'attacker',target_is_directory=True)
                self.assertEqual(m.read_at(archivefd,'marker',100),b'original')
                with self.assertRaises(OSError):m.open_archive(privatefd,Path('sub/archive'))
                with self.assertRaises(ValueError):m.open_archive(privatefd,Path('../attacker'))
            finally:os.close(archivefd);os.close(privatefd)
    def test_anchored_directory_survives_path_swap(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'content').mkdir();(root/'attacker').mkdir()
            blob=b'original';digest=hashlib.sha256(blob).hexdigest()
            (root/'content'/digest).write_bytes(blob);(root/'attacker'/digest).write_bytes(b'wrong')
            rootfd=os.open(root,os.O_RDONLY|os.O_DIRECTORY);contentfd=m.open_dir_at(rootfd,'content')
            try:
                (root/'content').rename(root/'moved');(root/'content').symlink_to(root/'attacker',target_is_directory=True)
                self.assertEqual(m.archive_blob(contentfd,digest,100),blob)
                with self.assertRaises(OSError):m.open_dir_at(rootfd,'content')
            finally:os.close(contentfd);os.close(rootfd)
    def test_file_symlink_and_unsafe_member_names_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'target').write_text('secret');(root/'link').symlink_to(root/'target')
            fd=os.open(root,os.O_RDONLY|os.O_DIRECTORY)
            try:
                with self.assertRaises(OSError):m.read_at(fd,'link',100)
                for name in ('','..','../target','/tmp/x'):
                    with self.subTest(name=name),self.assertRaises(ValueError):m.read_at(fd,name,100)
            finally:os.close(fd)
    def test_direct_index_anchor_bound_to_pinned_bytes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);url='https://example.gov/minutes.pdf';index=b'<a href="/minutes.pdf">Agenda PDF</a>'
            digest=hashlib.sha256(index).hexdigest();(root/digest).write_bytes(index)
            candidate={'url':url,'index_url':'https://example.gov/meetings','index_sha256':digest,'anchor_index':0}
            fd=os.open(root,os.O_RDONLY|os.O_DIRECTORY)
            try:
                m.check_direct_candidate(fd,url,candidate)
                with self.assertRaises(ValueError):m.check_direct_candidate(fd,url,{**candidate,'anchor_index':1})
                with self.assertRaises(ValueError):m.check_direct_candidate(fd,url,{**candidate,'redirect_receipt':{}})
            finally:os.close(fd)
if __name__=='__main__':unittest.main()
