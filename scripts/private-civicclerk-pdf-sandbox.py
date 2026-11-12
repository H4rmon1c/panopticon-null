#!/usr/bin/env python3
"""Inner helper for private PDF screening; requires bounded scope and tmpfs."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys


def require_limits():
    membership = Path('/proc/self/cgroup').read_text().splitlines()
    groups = [line[4:] for line in membership if line.startswith('0::/')]
    if len(groups) != 1 or any(x in ('', '.', '..') for x in groups[0].split('/')): raise ValueError('cgroup membership invalid')
    group = Path('/sys/fs/cgroup') / groups[0]
    mem = int((group/'memory.max').read_text()); swap = int((group/'memory.swap.max').read_text()); tasks = int((group/'pids.max').read_text()); cpu = [int(x) for x in (group/'cpu.max').read_text().split()]
    if not 0 < mem <= 512*1024*1024 or swap != 0 or not 0 < tasks <= 16 or len(cpu)!=2 or not 0 < cpu[0] <= cpu[1]: raise ValueError('cgroup limits invalid')
    for wanted in ('/tmp','/archive'):
        entries = [line.split(' - ',1) for line in Path('/proc/self/mountinfo').read_text().splitlines() if ' - ' in line and line.split(' - ',1)[0].split()[4] == wanted]
        if not entries: raise ValueError('tmpfs absent')
        fs = entries[-1][1].split(); options = fs[2].split(',') if len(fs)>2 else []
        size = next((x[5:] for x in options if x.startswith('size=')), None)
        if fs[0] != 'tmpfs' or size is None: raise ValueError('tmpfs limit absent')
        m = re.fullmatch(r'(\d+)([km]?)',size)
        if not m or not 0 < int(m[1])*{'':1,'k':1024,'m':1048576}[m[2]] <= 128*1024*1024: raise ValueError('tmpfs limit invalid')


def main():
    require_limits()
    cmd=['/pnull','--data-dir','/archive','--rules','/rules/surveillance.yml','source','screen-research-pdf','--bounded-private',*sys.argv[1:]]
    with open('/tmp/result','wb') as out, open('/tmp/error','wb') as err:
        run=subprocess.run(cmd,stdin=subprocess.DEVNULL,stdout=out,stderr=err,timeout=100)
    if run.returncode: raise ValueError('extractor failed')
    raw=Path('/tmp/result').read_bytes()
    if len(raw)>2*1024*1024: raise ValueError('extractor output too large')
    result=json.loads(raw); digest=result.get('extracted_sha256')
    if not isinstance(digest,str) or not re.fullmatch(r'[0-9a-f]{64}',digest): raise ValueError('invalid extracted digest')
    text=(Path('/archive/extracted')/digest).read_bytes()
    if len(text)>5*1024*1024 or hashlib.sha256(text).hexdigest()!=digest: raise ValueError('invalid extracted text')
    print(json.dumps({'result':result,'text':text.decode('utf8','strict')},separators=(',',':')))

if __name__ == '__main__': main()
