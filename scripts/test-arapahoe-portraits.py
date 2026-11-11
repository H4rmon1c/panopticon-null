#!/usr/bin/env python3
"""Verify the named budget-page portraits against extracted original JPEG bytes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PROOF = json.loads((ROOT / 'fixtures/atlas/portrait-sources/arapahoe-2022-budget-proof.json').read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, help='also verify the archived complete original PDF')
    args = parser.parse_args()
    source = args.source or (ROOT / PROOF['page_fixture'])
    expected = PROOF['source_sha256'] if args.source else PROOF['page_fixture_sha256']
    page = PROOF['source_page'] if args.source else 1
    if source.is_symlink() or not source.is_file() or source.stat().st_size > 20 * 1024 * 1024 or digest(source) != expected:
        raise ValueError('source PDF digest or file type differs')
    path = os.environ.get('PNUL_EXTRACTOR_PATH', '/usr/bin:/bin')
    executables = {name: shutil.which(name, path=path) for name in ('bwrap', 'prlimit', 'pdfimages', 'pdftotext')}
    if not all(executables.values()):
        raise ValueError('sandboxed PDF inspection tools unavailable')
    with tempfile.TemporaryDirectory(prefix='panopticon-portrait-proof-') as tmp:
        command = [executables['prlimit'], '--as=805306368', '--cpu=30', '--fsize=52428800', '--', executables['bwrap']]
        for runtime in ('/usr', '/bin', '/lib', '/lib64', '/nix/store'):
            if Path(runtime).exists():
                command.extend(['--ro-bind', runtime, runtime])
        command.extend(['--ro-bind', str(source.resolve()), '/input.pdf', '--bind', tmp, '/output', '--proc', '/proc',
                        '--dev', '/dev', '--tmpfs', '/tmp', '--unshare-net', '--unshare-pid', '--new-session',
                        '--clearenv', '--setenv', 'LANG', 'C.UTF-8', '--'])
        environment = {'PATH': path, 'LANG': 'C.UTF-8', 'TMPDIR': '/tmp'}
        def run(arguments):
            return subprocess.run(command + arguments, cwd=tmp, env=environment, stdin=subprocess.DEVNULL,
                                  stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=True, timeout=60).stdout
        run([executables['pdfimages'], '-f', str(page), '-l', str(page), '-j', '/input.pdf', '/output/portrait'])
        text = run([executables['pdftotext'], '-f', str(page), '-l', str(page), '-layout', '/input.pdf', '-']).decode('utf8')
        positions = [text.find(item['source_label']) for item in PROOF['portraits']]
        if '2022 BOARD OF COUNTY COMMISSIONERS' not in text or min(positions) < 0 or positions != sorted(positions):
            raise ValueError('labeled county board names do not match the source page')
        for portrait in PROOF['portraits']:
            extracted = Path(tmp) / f"portrait-{portrait['image_index']:03d}.jpg"
            asset = ROOT / 'web/panopticon-terminal/research-portraits' / f"{portrait['sha256']}.jpg"
            if extracted.is_symlink() or not extracted.is_file() or digest(extracted) != portrait['sha256']:
                raise ValueError('original embedded JPEG digest differs')
            if asset.is_symlink() or not asset.is_file() or extracted.read_bytes() != asset.read_bytes():
                raise ValueError('published portrait differs from the original JPEG stream')
    print('Arapahoe board labels and five original embedded portraits verified.')


if __name__ == '__main__':
    main()
