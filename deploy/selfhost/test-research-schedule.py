#!/usr/bin/env python3
"""Exercise the locked research schedule without making network requests."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def main():
    with tempfile.TemporaryDirectory(prefix='panopticon-research-schedule-') as tmp:
        base = Path(tmp)
        mock = base / 'python3'
        mock.write_text('''#!/usr/bin/python3
import json, os, sys
if sys.argv[1].endswith('/locked-run.py'):
    os.execv('/usr/bin/python3', ['/usr/bin/python3', *sys.argv[1:]])
if sys.argv[1] != 'scripts/colorado-research.py':
    raise SystemExit('unexpected command')
with open(os.environ['PNUL_TEST_LOG'], 'a', encoding='utf8') as output:
    output.write(json.dumps(sys.argv[1:]) + '\\n')
raise SystemExit(1 if sys.argv[4] == os.environ.get('PNUL_TEST_FAIL') else 0)
''')
        mock.chmod(0o755)
        data = base / 'private'
        data.mkdir()
        log = base / 'calls.jsonl'
        environment = {**os.environ, 'PATH': f'{base}:/usr/bin:/bin', 'PNUL_REPO_DIR': str(ROOT),
                       'PNUL_DATA_DIR': str(data), 'PNUL_BIN': '/usr/bin/true', 'PNUL_TEST_LOG': str(log)}
        environment.pop('PNUL_HELD_LOCKS', None)
        environment.pop('PNUL_TEST_FAIL', None)
        script = ROOT / 'deploy/selfhost/research.sh'

        def run():
            result = subprocess.run(['bash', str(script)], env=environment, stdout=subprocess.PIPE,
                                    stderr=subprocess.PIPE, text=True, timeout=30)
            calls = [json.loads(line) for line in log.read_text().splitlines()]
            return result, calls

        result, calls = run()
        assert result.returncode == 0, result.stderr
        commands = [call[3] for call in calls]
        assert commands[:3] == ['discover', 'civic-history', 'scan']
        assert commands.count('scan') == 2
        history = calls[1]
        assert history[4:] == ['--limit', '20', '--per-place', '4', '--workers', '8']
        assert commands.count('screen-documents') == 3 and commands[-1] == 'portraits'

        log.unlink()
        environment['PNUL_TEST_FAIL'] = 'civic-history'
        result, calls = run()
        assert result.returncode == 1
        assert any(call[3] == 'screen-documents' for call in calls)
        assert calls[-1][3] == 'portraits'
    print('Locked research schedule invokes historical discovery and isolates stage failure.')


if __name__ == '__main__':
    main()
