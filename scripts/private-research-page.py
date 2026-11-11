#!/usr/bin/env python3
"""Resume one private OCR page against an already receipt-bound research archive."""
import argparse
import hashlib
import importlib.util
import json
import os
import re
import secrets
import stat
from pathlib import Path
import subprocess
import tempfile

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('research',ROOT/'scripts/colorado-research.py')
r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
METHOD='pnull-research-page-v1; poppler-pdftotext-layout; pdftoppm-200dpi-4000max; tesseract-eng-tsv'


def open_dir_at(parent_fd, name):
    # O_NOFOLLOW applies to this directory component, not only the final file.
    return os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=parent_fd)


def open_absolute_dir(path):
    if not path.is_absolute() or any(part in ('.','..') for part in path.parts):
        raise ValueError('absolute path with ordinary components required')
    fd=os.open('/',os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW|os.O_CLOEXEC)
    try:
        for part in path.parts[1:]:
            child=open_dir_at(fd,part);os.close(fd);fd=child
        return fd
    except BaseException:
        os.close(fd);raise


def open_archive(private_fd, relative):
    if relative.is_absolute() or any(part in ('.','..') for part in relative.parts):
        raise ValueError('invalid relative archive path')
    fd=os.dup(private_fd)
    try:
        for part in relative.parts:
            child=open_dir_at(fd,part);os.close(fd);fd=child
        return fd
    except BaseException:
        os.close(fd);raise


def same_directory(left, right):
    a=os.fstat(left);b=os.fstat(right)
    return stat.S_ISDIR(a.st_mode) and stat.S_ISDIR(b.st_mode) and (a.st_dev,a.st_ino)==(b.st_dev,b.st_ino)


def bind_archive(private_root, archive, inherited_root=None, inherited_archive=None):
    if (inherited_root is None)!=(inherited_archive is None):
        raise ValueError('both inherited directory FDs are required')
    walked_root=open_absolute_dir(private_root)
    try:
        if inherited_root is not None and not same_directory(walked_root,inherited_root):
            raise ValueError('inherited private root differs from declared root')
        meta=os.fstat(walked_root)
        if meta.st_uid!=os.geteuid() or (meta.st_mode & 0o077):
            raise ValueError('private root must be a real owner-only directory')
        try:relative=archive.relative_to(private_root)
        except ValueError:raise ValueError('archive must be inside the private root') from None
        rootfd=os.dup(inherited_root) if inherited_root is not None else os.dup(walked_root)
        try:
            walked_archive=open_archive(rootfd,relative)
            try:
                if inherited_archive is not None and not same_directory(walked_archive,inherited_archive):
                    raise ValueError('inherited archive differs from declared archive')
                meta=os.fstat(walked_archive)
                if meta.st_uid!=os.geteuid() or (meta.st_mode & 0o077):
                    raise ValueError('archive must be a real owner-only directory')
                archivefd=os.dup(inherited_archive) if inherited_archive is not None else os.dup(walked_archive)
            finally:os.close(walked_archive)
        except BaseException:
            os.close(rootfd);raise
        return rootfd,archivefd
    finally:os.close(walked_root)


def read_at(directory_fd, name, maximum):
    if not isinstance(name,str) or name in ('','.','..') or '/' in name:
        raise ValueError('invalid archive member name')
    fd=os.open(name,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK|os.O_CLOEXEC,dir_fd=directory_fd)
    with os.fdopen(fd,'rb') as f:
        meta=os.fstat(f.fileno())
        if not stat.S_ISREG(meta.st_mode) or meta.st_size>maximum:raise ValueError('unsafe or oversized archive member')
        data=f.read(maximum+1)
        if len(data)!=meta.st_size:raise ValueError('archive member changed during read')
        return data


def archive_blob(content_fd, digest, maximum):
    if not isinstance(digest,str) or not re.fullmatch(r'[0-9a-f]{64}',digest):raise ValueError('invalid digest')
    data=read_at(content_fd,digest,maximum)
    if hashlib.sha256(data).hexdigest()!=digest:raise ValueError('archive digest mismatch')
    return data


def check_direct_candidate(content_fd,url,candidate):
    if not isinstance(candidate,dict) or any(k in candidate for k in ('springs_source','redirect_receipt','indexed_redirect_receipt','linked_url','origin_url','capture_adjustment')):
        raise ValueError('only direct source candidates are supported')
    index_url=candidate['index_url'];index=archive_blob(content_fd,candidate['index_sha256'],r.MAX_PAGE_BYTES)
    links,base=r.page_links(index,index_url);anchor=candidate['anchor_index']
    if candidate['url']!=url or type(anchor) is not int or not 0<=anchor<len(links):
        raise ValueError('document is not bound to its preserved source index')
    linked=r.resolve_page_link(index_url,base,links[anchor][0])
    if linked!=url or not r.candidate_kind(linked,index_url,links[anchor][1]):
        raise ValueError('document is not bound to its preserved source index')


def input_receipts(observations_fd, pairs):
    found={}
    for name in os.listdir(observations_fd):
        try:
            item=json.loads(read_at(observations_fd,name,65536));pair=(item.get('url'),item.get('sha256'))
            if pair in pairs and r.receipt_valid(item,*pair):found[pair]=item
        except (OSError,ValueError,TypeError):pass
    if set(found)!=set(pairs):raise ValueError('missing source or index receipt')
    return found


def file_hash(path):
    # Toolchain inputs are trusted local executable files, but bounded so a
    # mistaken directory or device cannot cause an unbounded read.
    fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
    with os.fdopen(fd,'rb') as f:
        stat=os.fstat(f.fileno())
        if not __import__('stat').S_ISREG(stat.st_mode) or stat.st_size>256*1024*1024:
            raise ValueError('invalid toolchain input')
        h=hashlib.sha256()
        for block in iter(lambda:f.read(1024*1024),b''):h.update(block)
        return h.hexdigest()


def provenance(binary,tessdata):
    inputs={'pnull':binary,'eng.traineddata':tessdata/'eng.traineddata',
            **{name:Path('/usr/bin')/name for name in ('pdfinfo','pdftotext','pdftoppm','tesseract','bwrap','prlimit','systemd-run')}}
    return {name:file_hash(path) for name,path in inputs.items()}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--archive',type=Path,required=True);p.add_argument('--private-root',type=Path,required=True)
    p.add_argument('--private-root-fd',type=int);p.add_argument('--archive-fd',type=int)
    p.add_argument('--binary',type=Path,required=True)
    p.add_argument('--tessdata',type=Path,required=True);p.add_argument('--page',type=int,required=True)
    a=p.parse_args()
    if not all(x.is_absolute() for x in (a.archive,a.private_root,a.binary,a.tessdata)) or not 1<=a.page<=300:
        p.error('absolute paths and a page between 1 and 300 are required')
    # Optional inherited FDs let a coordinator pin identity across process
    # handoff. Both are compared with no-follow walks of the declared paths.
    private_fd,root_fd=bind_archive(a.private_root,a.archive,a.private_root_fd,a.archive_fd)
    archive=a.archive.resolve()
    for public_path in [ROOT/'web/panopticon-terminal',Path('/var/www'),Path('/srv/www'),Path('/srv/panopticon')]:
        public=public_path.resolve()
        if archive==public or public in archive.parents:p.error('archive cannot be under a configured public root')
    tess=a.tessdata.resolve()
    if not (tess==Path('/usr') or Path('/usr') in tess.parents or tess==Path('/nix/store') or Path('/nix/store') in tess.parents):
        p.error('tessdata must be inside /usr or /nix/store')
    # Anchor all archive members to open directory descriptors, so parent
    # symlink swaps cannot redirect source reads or page writes.
    content_fd=open_dir_at(root_fd,'content'); observations_fd=open_dir_at(root_fd,'observations')
    doc=json.loads(read_at(root_fd,'screen.json',2*1024*1024))
    candidate=doc['candidate'];url=doc['url'];digest=doc['document_sha256'];ref=doc['jurisdiction_ref']
    check_direct_candidate(content_fd,url,candidate)
    archive_blob(content_fd,digest,20*1024*1024)
    pairs=[(url,digest),(candidate['index_url'],candidate['index_sha256'])]
    receipts=input_receipts(observations_fd,pairs);receipt=receipts[(url,digest)]
    tools=provenance(a.binary,a.tessdata)
    try:os.mkdir('ocr-pages',mode=0o700,dir_fd=root_fd)
    except FileExistsError:pass
    pages_fd=open_dir_at(root_fd,'ocr-pages');os.fchmod(pages_fd,0o700)
    target=f'{a.page:04d}.json'
    def validate(item):
        return (item.get('state')=='private_provisional_page' and item.get('page')==a.page
                and item.get('document_sha256')==digest and item.get('source_url')==url
                and item.get('jurisdiction_ref')==ref and item.get('retrieved_at')==receipt['retrieved_at']
                and item.get('method')==METHOD and isinstance(item.get('page_count'),int)
                and a.page<=item['page_count']<=300 and isinstance(item.get('native_text'),str)
                and isinstance(item.get('ocr_text'),str) and bool(item['ocr_text'].strip()))
    try:prior=json.loads(read_at(pages_fd,target,12*1024*1024))
    except FileNotFoundError:prior=None
    if prior is not None and (not validate(prior.get('result',{})) or prior.get('toolchain_sha256')!=tools or prior.get('candidate')!=candidate):
        p.error('existing page differs from source, method or toolchain; use a fresh archive')
    # A per-page systemd scope caps aggregate tasks, memory, swap, CPU, and
    # lifetime. The outer namespace puts every pnull scratch/staging file in a
    # 128 MiB tmpfs, which disappears on teardown, including after SIGKILL.
    cmd=['/usr/bin/systemd-run','--user','--scope','--quiet',
         '--property=MemoryMax=512M','--property=MemorySwapMax=0','--property=TasksMax=16',
         '--property=CPUQuota=100%','--property=RuntimeMaxSec=60','--',
         '/usr/bin/bwrap','--unshare-user','--unshare-pid','--unshare-net','--unshare-ipc','--unshare-uts',
         '--die-with-parent','--new-session',
         '--size','1048576','--tmpfs','/','--ro-bind','/usr','/usr',
         '--symlink','usr/lib','/lib','--symlink','usr/lib','/lib64','--symlink','usr/bin','/bin',
         '--proc','/proc','--dev','/dev','--size','134217728','--tmpfs','/tmp',
         '--ro-bind','/sys/fs/cgroup','/sys/fs/cgroup',
         '--ro-bind',str(a.binary),'/pnull','--dir','/archive',
         '--ro-bind',f'/proc/self/fd/{content_fd}','/archive/content',
         '--ro-bind',str(ROOT/'configs'),'/configs','--ro-bind',str(ROOT/'rules'),'/rules']
    if Path('/nix/store').is_dir():cmd.extend(['--ro-bind','/nix/store','/nix/store'])
    cmd.extend(['--clearenv','--setenv','PATH','/usr/bin:/bin','--setenv','PNUL_TESSDATA_DIR',str(tess),
                '--chdir','/','/pnull','--data-dir','/archive','source','screen-research-page','--digest',digest,
                '--source-url',url,'--jurisdiction-ref',ref,'--retrieved-at',receipt['retrieved_at'],'--page',str(a.page)])
    # systemd-run needs the user's bus/runtime environment; none is inherited by
    # the inner extractor, whose environment is cleared by the Rust sandbox.
    env={**os.environ,'PATH':'/usr/bin:/bin','LANG':'C.UTF-8'}
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        try:run=subprocess.run(cmd,stdout=output,stderr=errors,timeout=90,env=env,cwd=ROOT,pass_fds=(content_fd,))
        except subprocess.TimeoutExpired:raise ValueError('page extraction timed out')
        if run.returncode or output.tell()>12*1024*1024:
            errors.seek(0); detail=r.public_text(errors.read(4096).decode('utf8','replace'),200)
            raise ValueError('page extraction failed or exceeded output bound: '+detail)
        output.seek(0);result=json.loads(output.read())
    if not validate(result):raise ValueError('page output does not match source or is incomplete')
    if prior is not None:
        if prior['result'] != result:
            raise ValueError('preserved page differs from independent re-extraction')
        print('verified and preserved');return
    envelope={'result':result,'candidate':candidate,'toolchain_sha256':tools,
              'notice':'Private page result only; OCR may miss text. No whole-document completeness or public claim.'}
    temp='.page-'+secrets.token_hex(16)
    fd=os.open(temp,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=pages_fd)
    try:
        with os.fdopen(fd,'w') as f:json.dump(envelope,f);f.write('\n');f.flush();os.fsync(f.fileno())
        os.link(temp,target,src_dir_fd=pages_fd,dst_dir_fd=pages_fd,follow_symlinks=False)
    finally:os.unlink(temp,dir_fd=pages_fd)
    print('private provisional page preserved')

if __name__=='__main__':main()
