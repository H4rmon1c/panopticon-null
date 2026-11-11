#!/usr/bin/env python3
import json,os,sqlite3,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
with tempfile.TemporaryDirectory() as temporary:
    base=Path(temporary); private=base/'private';private.mkdir()
    for name in ('evidence','records','discovery-candidates','discovery-archives','research'): (private/name).mkdir()
    with sqlite3.connect(private/'pnull.db') as c:
        c.execute('CREATE TABLE proof (value TEXT)');c.execute("INSERT INTO proof VALUES ('retained')")
    with sqlite3.connect(private/'research/research.sqlite3') as c:
        c.execute('CREATE TABLE research_proof (value TEXT)');c.execute("INSERT INTO research_proof VALUES ('retained research')")
    release=base/'site/releases/one';release.mkdir(parents=True)
    current=base/'site/current';current.symlink_to(release)
    secret=base/'password';secret.write_text('never-print-me');secret.chmod(0o600)
    binary=base/'bin';binary.mkdir();mock=binary/'restic'
    mock.write_text('#!/usr/bin/env python3\nimport json,os,sqlite3,sys\nfrom pathlib import Path\nargs=sys.argv[1:]\np=next(x for x in args if x.endswith("pnull.db"))\nwith sqlite3.connect(p) as c: assert c.execute("SELECT value FROM proof").fetchone()[0]=="retained"\nr=next(x for x in args if x.endswith("research.sqlite3"))\nwith sqlite3.connect(r) as c: assert c.execute("SELECT value FROM research_proof").fetchone()[0]=="retained research"\nPath(os.environ["PNUL_TEST_ARGS"]).write_text(json.dumps(args))\n')
    mock.chmod(0o755)
    env=dict(os.environ,PNUL_DATA_DIR=str(private),PNUL_CURRENT_LINK=str(current),PNUL_RELEASE_ROOT=str(release.parent),RESTIC_REPOSITORY='sftp:backup.example:/repo',RESTIC_PASSWORD_FILE=str(secret),PNUL_TEST_ARGS=str(base/'args'),PATH=str(binary)+os.pathsep+os.environ['PATH'])
    script=['bash',str(ROOT/'deploy/selfhost/backup.sh')]
    result=subprocess.run(script,env=env,capture_output=True);assert result.returncode==0,result.stderr.decode()
    assert not list((private/'ops').glob('.backup.*'))
    assert 'never-print-me' not in (result.stdout+result.stderr).decode()
    saved=json.loads((base/'args').read_text())
    assert str(private/'discovery-candidates') in saved and str(private/'discovery-archives') in saved
    assert str(private/'research') in saved
    research=private/'research/research.sqlite3'; original=private/'research/original.sqlite3';research.rename(original)
    sentinel=base/'sentinel';sentinel.write_text('untouched');research.symlink_to(sentinel)
    assert subprocess.run(script,env=env,capture_output=True).returncode!=0
    assert sentinel.read_text()=='untouched'
    research.unlink();original.rename(research)
    env['RESTIC_REPOSITORY']=str(base/'local-backup')
    assert subprocess.run(script,env=env,capture_output=True).returncode!=0
    print('Backup coherent SQLite snapshot, cleanup, and off-server destination checks passed.')
