#!/usr/bin/env python3
"""Bounded private scheduler for the separately sandboxed one-page OCR worker.

This is a prototype, not a service or a publication pipeline. Each successful
result is provisional, and cached scheduling state is not evidence.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import secrets
import stat
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
WORKER = ROOT / 'scripts/private-research-page.py'
MAX_QUEUE = 10000


def preflight(binary, tessdata):
    for label, path in (('OCR binary', binary), ('English OCR language data', tessdata / 'eng.traineddata')):
        try:
            fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK)
            try:
                info = os.fstat(fd)
                if not stat.S_ISREG(info.st_mode) or not 0 < info.st_size <= 256 * 1024 * 1024:
                    raise ValueError('invalid dependency')
                if label == 'OCR binary' and (os.access not in os.supports_effective_ids or
                        not os.access(f'/proc/self/fd/{fd}', os.X_OK, effective_ids=True)):
                    raise ValueError('binary is not executable by the effective user')
            finally: os.close(fd)
        except (OSError, ValueError) as error:
            raise ValueError(label + ' unavailable or invalid') from error


DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC


def components(value):
    path = Path(value)
    if path.is_absolute() or not path.parts or str(path) != str(value) or any(p in ('', '.', '..') for p in path.parts):
        raise ValueError('invalid private relative path')
    return path.parts


def open_dir_at(root_fd, path):
    fd = os.dup(root_fd)
    try:
        for part in components(path):
            new = os.open(part, DIR_FLAGS, dir_fd=fd)
            os.close(fd); fd = new
        return fd
    except BaseException:
        os.close(fd); raise


def open_file_at(root_fd, path, flags=os.O_RDONLY, mode=0o600):
    parts = components(path)
    parent = os.dup(root_fd) if len(parts) == 1 else open_dir_at(root_fd, str(Path(*parts[:-1])))
    try:
        return os.open(parts[-1], flags | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK, mode, dir_fd=parent)
    finally: os.close(parent)


def read_regular_at(root_fd, path, limit):
    fd = open_file_at(root_fd, path)
    with os.fdopen(fd, 'rb') as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
            raise ValueError('invalid or oversized input file')
        data = stream.read(limit + 1)
        if len(data) > limit:
            raise ValueError('input file grew beyond limit')
        return data


def private_dir(path):
    if not path.is_absolute() or path == Path('/'):
        raise ValueError('private root must be an absolute real directory')
    base = os.open('/', DIR_FLAGS)
    try: fd = open_dir_at(base, str(path.relative_to('/')))
    finally: os.close(base)
    try:
        info = os.fstat(fd)
        if info.st_uid != os.geteuid() or info.st_mode & 0o077:
            raise ValueError('private root must be owned by the operator with no group/other access')
        root = Path(os.readlink(f'/proc/self/fd/{fd}'))
        if not root.is_absolute() or str(root).endswith(' (deleted)'):
            raise ValueError('private root is no longer available')
        for public in ((ROOT / 'web/panopticon-terminal').resolve(), Path('/var/www').resolve(), Path('/srv/www').resolve(), Path('/srv/panopticon').resolve()):
            if root == public or public in root.parents:
                raise ValueError('private root cannot be public')
        return root, fd
    except BaseException:
        os.close(fd); raise


def parse_jobs(raw, root_fd):
    queue = json.loads(raw)
    if not isinstance(queue, dict) or set(queue) != {'jobs'} or not isinstance(queue['jobs'], list) or len(queue['jobs']) > MAX_QUEUE:
        raise ValueError('invalid queue')
    result = []; seen = set()
    for item in queue['jobs']:
        if not isinstance(item, dict) or set(item) != {'archive', 'page'}:
            raise ValueError('invalid queue job')
        name, page = item['archive'], item['page']
        if not isinstance(name, str) or not name or len(name) > 300 or type(page) is not int or not 1 <= page <= 300:
            raise ValueError('invalid archive or page')
        fd = open_dir_at(root_fd, name); os.close(fd)
        key = (name, page)
        if key in seen: raise ValueError('duplicate page job')
        seen.add(key); result.append({'archive': name, 'page': page})
    return result


def save_state(root_fd, state):
    tmp = '.ocr-backlog-' + secrets.token_hex(16)
    fd = open_file_at(root_fd, tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL)
    try:
        with os.fdopen(fd, 'w') as out:
            json.dump(state, out, sort_keys=True); out.write('\n'); out.flush(); os.fsync(out.fileno())
        os.replace(tmp, 'ocr-backlog-state.json', src_dir_fd=root_fd, dst_dir_fd=root_fd)
        os.fsync(root_fd)
    finally:
        try: os.unlink(tmp, dir_fd=root_fd)
        except FileNotFoundError: pass


def artifact(root, job):
    return root / job['archive'] / 'ocr-pages' / f"{job['page']:04d}.json"


def digest_artifact(root_fd, job):
    path = str(Path(job['archive']) / 'ocr-pages' / f"{job['page']:04d}.json")
    return hashlib.sha256(read_regular_at(root_fd, path, 12 * 1024 * 1024)).hexdigest()


def artifact_identity(root_fd, job):
    archive_fd = open_dir_at(root_fd, job['archive'])
    try:
        info = os.fstat(archive_fd)
        path = str(Path('ocr-pages') / f"{job['page']:04d}.json")
        digest = hashlib.sha256(read_regular_at(archive_fd, path, 12 * 1024 * 1024)).hexdigest()
        return digest, [info.st_dev, info.st_ino]
    finally: os.close(archive_fd)


def run_job(root, root_fd, job, binary, tessdata):
    try: archive_fd = open_dir_at(root_fd, job['archive'])
    except (OSError, ValueError): return 'incomplete', None, None
    try:
        info = os.fstat(archive_fd); identity = [info.st_dev, info.st_ino]
        cmd = ['/usr/bin/python3', str(WORKER), '--archive', str(root / job['archive']), '--private-root', str(root),
               '--private-root-fd', str(root_fd), '--archive-fd', str(archive_fd),
               '--binary', str(binary), '--tessdata', str(tessdata), '--page', str(job['page'])]
        with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
            try: result = subprocess.run(cmd, stdin=subprocess.DEVNULL, stdout=output, stderr=errors,
                                         timeout=105, check=False, pass_fds=(root_fd, archive_fd))
            except subprocess.TimeoutExpired: return 'incomplete', None, None
            if result.returncode != 0: return 'incomplete', None, None
        try:
            digest = hashlib.sha256(read_regular_at(archive_fd, str(Path('ocr-pages') / f"{job['page']:04d}.json"),
                                                    12 * 1024 * 1024)).hexdigest()
            current_digest, current_identity = artifact_identity(root_fd, job)
            if current_identity != identity or current_digest != digest: return 'incomplete', None, None
            return 'worker_succeeded', digest, identity
        except (OSError, ValueError): return 'incomplete', None, None
    finally: os.close(archive_fd)


def run(queue, queue_digest, root, root_fd, state, binary, tessdata, max_pages, max_seconds, now=time.monotonic, execute=run_job):
    if (state.get('queue_sha256') != queue_digest or not isinstance(state.get('pages'), dict)
            or type(state.get('sequence', 0)) is not int or state.get('sequence', 0) < 0):
        raise ValueError('queue differs from saved scheduler state')
    deadline = now() + max_seconds; attempted = 0; records = []
    # Always try never-attempted work before retrying earlier failures so a
    # bad page cannot starve a large backlog.
    cache_valid = {}
    def priority(index):
        old = state['pages'].get(str(index))
        if old is None: return (0, 0, index)
        if old.get('status') == 'worker_succeeded' and now() < deadline:
            try:
                digest, identity = artifact_identity(root_fd, queue[index])
                cache_valid[index] = digest == old.get('artifact_sha256') and identity == old.get('archive_identity')
            except (OSError, ValueError): cache_valid[index] = False
            if cache_valid[index]: return (2, 0, index)
        return (1, old.get('last_attempt', 0), index)
    for index in sorted(range(len(queue)), key=priority):
        job = queue[index]
        key = str(index); old = state['pages'].get(key)
        if now() >= deadline:
            records.append({'index': index, 'state': 'deferred'}); continue
        if cache_valid.get(index):
            records.append({'index': index, 'state': 'cached_not_reverified'}); continue
        # Do not start a worker without enough time for its timeout.
        if attempted >= max_pages or deadline - now() < 105:
            records.append({'index': index, 'state': 'deferred'}); continue
        status, digest, identity = execute(root, root_fd, job, binary, tessdata); attempted += 1
        if (status not in ('worker_succeeded', 'incomplete') or
                (status == 'worker_succeeded' and (not digest or not isinstance(identity, list) or len(identity) != 2))):
            raise ValueError('invalid worker result')
        state['sequence'] = state.get('sequence', 0) + 1
        state['pages'][key] = {'status': status, 'artifact_sha256': digest, 'archive_identity': identity,
                               'last_attempt': state['sequence']}
        save_state(root_fd, state)
        records.append({'index': index, 'state': status})
    return {'state': 'private_scheduler_only', 'notice': 'Cached pages were not reverified this run; OCR output is provisional and never establishes complete document coverage.',
            'attempted': attempted, 'results': records}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--private-root', type=Path, required=True); p.add_argument('--queue', type=Path, required=True)
    p.add_argument('--binary', type=Path, required=True); p.add_argument('--tessdata', type=Path, required=True)
    p.add_argument('--max-pages', type=int, default=10); p.add_argument('--max-seconds', type=int, default=1200)
    a = p.parse_args()
    if not 1 <= a.max_pages <= 50 or not 105 <= a.max_seconds <= 3600 or not a.binary.is_absolute() or not a.tessdata.is_absolute():
        p.error('invalid run limits or tool paths')
    try: preflight(a.binary, a.tessdata)
    except ValueError as error: p.error(str(error))
    root, root_fd = private_dir(a.private_root)
    try: queue_path = a.queue.relative_to(root)
    except ValueError: p.error('queue must be a regular file inside the private root')
    if not WORKER.is_file(): p.error('verified private single-page worker is not installed')
    raw = read_regular_at(root_fd, str(queue_path), 2 * 1024 * 1024); queue = parse_jobs(raw, root_fd); qhash = hashlib.sha256(raw).hexdigest()
    lock = open_file_at(root_fd, '.ocr-backlog.lock', os.O_WRONLY | os.O_CREAT)
    try:
        if not stat.S_ISREG(os.fstat(lock).st_mode): raise ValueError('invalid scheduler lock')
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        try: state = json.loads(read_regular_at(root_fd, 'ocr-backlog-state.json', 4 * 1024 * 1024))
        except FileNotFoundError: state = {'queue_sha256': qhash, 'pages': {}}
        print(json.dumps(run(queue, qhash, root, root_fd, state, a.binary, a.tessdata, a.max_pages, a.max_seconds)))
    finally:
        os.close(lock); os.close(root_fd)

if __name__ == '__main__': main()
