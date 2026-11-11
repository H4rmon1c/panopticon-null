#!/usr/bin/env python3
"""Health must preserve failure/expiry/staleness without leaking private data."""
from datetime import datetime, timedelta, timezone
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'deploy/selfhost/run-stage.py'
with tempfile.TemporaryDirectory() as temporary:
    base = Path(temporary)
    private = base / 'private'
    private.mkdir()
    repo = base / 'repo'
    (repo / 'deploy/selfhost').mkdir(parents=True)
    (repo / 'deploy/selfhost/collection-sources.tsv').write_text('config\ttest-source\tarchive\n')
    (repo / 'deploy/selfhost/candidate-queues.tsv').write_text('# no queues enabled\n')
    health = base / 'public/health.json'
    env = dict(os.environ, PNUL_DATA_DIR=str(private), PNUL_REPO_DIR=str(repo), PNUL_PUBLIC_HEALTH_PATH=str(health))
    future = (datetime.now(timezone.utc) + timedelta(days=10)).isoformat()
    review = {'source_id':'test-source','reviewed_at':'2026-01-01T00:00:00Z','expires_at':future,'reviewer':'PRIVATE REVIEWER','note':'PRIVATE NOTE'}
    database = sqlite3.connect(private / 'pnull.db')
    database.execute('CREATE TABLE source_reviews (review_json TEXT)')
    database.execute('INSERT INTO source_reviews VALUES (?)',(json.dumps(review),))
    database.commit()
    def run(stage, success=True):
        result = subprocess.run(['python3', str(SCRIPT), stage, '--', '/bin/true' if success else '/bin/false'], env=env, capture_output=True)
        assert result.returncode == (0 if success else 1), result.stderr.decode()
        return json.loads(health.read_text())
    run('collect')
    run('candidates')
    state=run('publish')
    assert state['state']=='ok' and state['stages']['candidates']['state']=='disabled'
    assert 'PRIVATE' not in health.read_text() and str(private) not in health.read_text()
    success=state['stages']['collect']['last_success_at']
    state=run('collect',False)
    assert state['state']=='degraded' and state['stages']['collect']['last_success_at']==success
    run('collect')
    review['expires_at']='2020-01-01T00:00:00Z'
    database.execute('UPDATE source_reviews SET review_json=?',(json.dumps(review),));database.commit()
    result=subprocess.run(['python3',str(SCRIPT),'check'],env=env,capture_output=True)
    assert result.returncode==1
    assert json.loads(health.read_text())['source_reviews']['expired_record_count']==1
    private_health=private/'ops/health.json'
    state=json.loads(private_health.read_text());state['stages']['publish']['last_success_at']='2020-01-01T00:00:00Z';private_health.write_text(json.dumps(state))
    subprocess.run(['python3',str(SCRIPT),'check'],env=env,capture_output=True)
    assert json.loads(health.read_text())['stages']['publish']['state']=='stale'
    assert private_health.stat().st_mode & 0o777 == 0o600
    state=json.loads(private_health.read_text());state['stages']['publish']['private_note']='PRIVATE NOTE';state['stages']['publish']['last_success_at']='2099-01-01T00:00:00Z';private_health.write_text(json.dumps(state))
    subprocess.run(['python3',str(SCRIPT),'check'],env=env,capture_output=True)
    assert json.loads(health.read_text())['stages']['publish']['state']=='clock_error'
    assert 'PRIVATE NOTE' not in health.read_text()
    (repo / 'deploy/selfhost/collection-sources.tsv').write_text('# empty\n')
    subprocess.run(['python3',str(SCRIPT),'check'],env=env,capture_output=True)
    assert json.loads(health.read_text())['source_reviews']['state']=='unknown'
    lock=private/'ops/health.lock'; lock.unlink(); lock.symlink_to(secret_target := base/'unrelated')
    secret_target.write_text('unchanged'); secret_target.chmod(0o644)
    assert subprocess.run(['python3',str(SCRIPT),'check'],env=env,capture_output=True).returncode!=0
    assert secret_target.read_text()=='unchanged' and secret_target.stat().st_mode & 0o777 == 0o644
    print('Health failure, source-review expiry, staleness, and privacy checks passed.')
