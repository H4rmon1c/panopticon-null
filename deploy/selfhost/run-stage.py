#!/usr/bin/env python3
"""Run a bounded systemd stage, persist health, and expose only operational metadata."""
import argparse
from datetime import datetime, timezone
import fcntl
import json
import os
from pathlib import Path
import signal
import stat
import sqlite3
import subprocess
import sys
import tempfile

STAGES = ('collect', 'candidates', 'publish')
MAX_AGE = 172800


def now():
    return datetime.now(timezone.utc)


def timestamp(value):
    return value.isoformat().replace('+00:00', 'Z')


def parse_time(value):
    return datetime.fromisoformat(value.replace('Z', '+00:00'))


def atomic_json(path, value, mode):
    if path.is_symlink():
        raise ValueError('health output must not be a symlink')
    path.parent.mkdir(mode=0o700 if mode == 0o600 else 0o755, parents=True, exist_ok=True)
    handle, temporary = tempfile.mkstemp(prefix='.health-', dir=path.parent)
    try:
        os.fchmod(handle, mode)
        with os.fdopen(handle, 'w') as output:
            json.dump(value, output, sort_keys=True, indent=2)
            output.write('\n')
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def review_summary(data_dir, repo_dir, moment):
    """Expiry counts are informational; the Rust CLI verifies scope/config/receipts."""
    try:
        scheduled = set()
        matrix = repo_dir / os.environ.get('PNUL_COLLECT_MATRIX_PATH', 'deploy/selfhost/collection-sources.tsv')
        for line in matrix.read_text().splitlines():
            if line and not line.startswith('#'):
                parts = line.split('\t')
                if len(parts) != 3:
                    raise ValueError('invalid matrix')
                scheduled.add(parts[1])
        if not scheduled:
            raise ValueError('empty collection schedule')
        latest = {}
        with sqlite3.connect(f'file:{data_dir / "pnull.db"}?mode=ro', uri=True, timeout=5) as database:
            for (raw,) in database.execute('SELECT review_json FROM source_reviews'):
                item = json.loads(raw)
                identity = item['source_id']
                if identity not in latest or parse_time(item['reviewed_at']) > parse_time(latest[identity]['reviewed_at']):
                    latest[identity] = item
        expires = [parse_time(latest[key]['expires_at']) for key in scheduled if key in latest]
        missing = len(scheduled - set(latest))
        expired = sum(value <= moment for value in expires)
        return {'state': 'needs_attention' if missing or expired else 'recorded',
                'scheduled_sources': len(scheduled), 'missing_record_count': missing,
                'expired_record_count': expired,
                'earliest_recorded_expiry': timestamp(min(expires)) if expires else None,
                'scope_and_config_validation': 'enforced_by_collector'}
    except (OSError, ValueError, KeyError, TypeError, sqlite3.Error):
        return {'state': 'unknown', 'scope_and_config_validation': 'enforced_by_collector'}


def publish_health(state, data_dir, repo_dir):
    moment = now()
    stages = {}
    for name in STAGES:
        raw = state.get('stages', {}).get(name, {})
        entry = {}
        status = raw.get('state', 'not_run') if isinstance(raw, dict) else 'invalid'
        if status not in ('not_run', 'ok', 'failed', 'disabled', 'running'):
            status = 'invalid'
        for key in ('started_at', 'finished_at', 'last_success_at'):
            if isinstance(raw, dict) and key in raw:
                try:
                    value = parse_time(raw[key])
                    if value.tzinfo is None or (value - moment).total_seconds() > 300:
                        status = 'clock_error'
                    else:
                        entry[key] = timestamp(value)
                except (ValueError, TypeError, AttributeError):
                    status = 'invalid'
        code = raw.get('exit_code') if isinstance(raw, dict) else None
        if code is None or (isinstance(code, int) and not isinstance(code, bool) and -255 <= code <= 255):
            entry['exit_code'] = code
        else:
            status = 'invalid'
        last_success = entry.get('last_success_at')
        if status not in ('disabled', 'running') and last_success:
            if (moment - parse_time(last_success)).total_seconds() > MAX_AGE:
                status = 'stale' if status == 'ok' else status
        if status == 'running':
            if not entry.get('started_at'):
                status = 'invalid'
            elif (moment - parse_time(entry['started_at'])).total_seconds() > 7200:
                status = 'stale_running'
        if status == 'ok' and not last_success:
            status = 'invalid'
        entry['state'] = status
        stages[name] = entry
    reviews = review_summary(data_dir, repo_dir, moment)
    statuses = [entry['state'] for entry in stages.values()]
    if any(status not in ('ok', 'disabled', 'running') for status in statuses) or reviews['state'] != 'recorded':
        status = 'degraded'
    elif 'running' in statuses:
        status = 'running'
    else:
        status = 'ok'
    public = {'schema_version': 'panopticon-health-v1', 'generated_at': timestamp(moment),
              'state': status, 'stale_after_seconds': MAX_AGE, 'stages': stages,
              'source_reviews': reviews,
              'freshness_note': 'Publication success does not establish a fresh source fetch. Collection success applies only to the configured reviewed source schedule; coverage is reported separately.'}
    output = os.environ.get('PNUL_PUBLIC_HEALTH_PATH')
    if output:
        path = Path(output)
        if not path.is_absolute() or path.is_relative_to(data_dir):
            raise ValueError('public health path must be absolute and outside private state')
        atomic_json(path, public, 0o644)
    return public


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage', choices=(*STAGES, 'check'))
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    data_dir = Path(os.environ['PNUL_DATA_DIR']).resolve(strict=True)
    repo_dir = Path(os.environ['PNUL_REPO_DIR']).resolve(strict=True)
    state_dir = data_dir / 'ops'
    state_dir.mkdir(mode=0o700, exist_ok=True)
    if state_dir.is_symlink():
        raise ValueError('private health directory must not be a symlink')
    state_path = state_dir / 'health.json'
    descriptor = os.open(state_dir / 'health.lock', os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    if not stat.S_ISREG(os.fstat(descriptor).st_mode):
        os.close(descriptor)
        raise ValueError('health lock must be a regular file')
    os.fchmod(descriptor, 0o600)
    lock = os.fdopen(descriptor, 'a')

    def update(entry=None):
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            if state_path.is_symlink():
                raise ValueError('private health state must not be a symlink')
            if state_path.exists():
                descriptor = os.open(state_path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
                with os.fdopen(descriptor, 'rb') as input_file:
                    info = os.fstat(input_file.fileno())
                    if not stat.S_ISREG(info.st_mode) or info.st_size > 65536:
                        raise ValueError('private health state is not a bounded regular file')
                    raw = input_file.read(65537)
                    if len(raw) > 65536:
                        raise ValueError('private health state exceeds byte limit')
                    state = json.loads(raw)
            else:
                state = {'stages': {}}
            if not isinstance(state, dict) or not isinstance(state.get('stages'), dict):
                raise ValueError('invalid private health state')
            if entry is not None:
                previous = state['stages'].get(args.stage, {})
                previous.update(entry)
                state['stages'][args.stage] = previous
                atomic_json(state_path, state, 0o600)
            return publish_health(state, data_dir, repo_dir)
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)

    if args.stage == 'check':
        status = update()
        print(json.dumps(status, sort_keys=True))
        return 0 if status['state'] == 'ok' else 1
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command:
        raise ValueError('stage command is required')
    update({'state': 'running', 'started_at': timestamp(now()), 'exit_code': None})
    process = None
    def terminate(signum, _frame):
        if process is not None:
            try:
                os.killpg(process.pid, signum)
            except ProcessLookupError:
                pass
    signal.signal(signal.SIGTERM, terminate)
    signal.signal(signal.SIGINT, terminate)
    try:
        process = subprocess.Popen(command, start_new_session=True)
        result = process.wait()
    except OSError:
        result = 127
    entry = {'state': 'ok' if result == 0 else 'failed', 'finished_at': timestamp(now()), 'exit_code': result}
    if result == 0:
        entry['last_success_at'] = entry['finished_at']
        if args.stage == 'candidates':
            matrix = repo_dir / os.environ.get('PNUL_CANDIDATE_MATRIX_PATH', 'deploy/selfhost/candidate-queues.tsv')
            if not any(line.strip() and not line.startswith('#') for line in matrix.read_text().splitlines()):
                entry['state'] = 'disabled'
    update(entry)
    return result if result >= 0 else 128 - result


if __name__ == '__main__':
    sys.exit(main())
