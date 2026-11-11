#!/usr/bin/env python3
import importlib.util,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
helper=ROOT/'deploy/selfhost/locked-run.py'
spec=importlib.util.spec_from_file_location('installer',ROOT/'deploy/selfhost/install-directories.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
with tempfile.TemporaryDirectory() as temporary:
    base=Path(temporary);target=base/'live-page';target.write_text('public content');target.chmod(0o644)
    link=base/'poison.lock';link.symlink_to(target)
    result=subprocess.run(['python3',str(helper),str(link),'--','/bin/true'],capture_output=True)
    assert result.returncode!=0 and target.read_text()=='public content' and target.stat().st_mode&0o777==0o644
    lock=base/'good.lock';lock.write_text('existing lock bytes')
    result=subprocess.run(['python3',str(helper),str(lock),'--','python3',str(helper),'--verify',str(lock)],capture_output=True)
    assert result.returncode==0,result.stderr.decode()
    assert lock.read_text()=='existing lock bytes'
    real=base/'root-config';real.mkdir();real.chmod(0o700)
    poison=base/'site';poison.symlink_to(real)
    try: module.directory(poison,os.getuid(),os.getgid(),0o755)
    except OSError: pass
    else: raise AssertionError('directory installer followed symlink')
    assert real.stat().st_mode&0o777==0o700
    module.directory(base/'new/real',os.getuid(),os.getgid(),0o750)
    assert (base/'new/real').stat().st_mode&0o777==0o750
    print('Nontruncating no-follow locks and directory installation symlink checks passed.')
