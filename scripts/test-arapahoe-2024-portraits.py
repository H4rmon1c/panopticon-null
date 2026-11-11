#!/usr/bin/env python3
"""Verify the 2024 county budget's labeled, original embedded portraits."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PROOF = json.loads((ROOT / 'fixtures/atlas/portrait-sources/arapahoe-2024-budget-proof.json').read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, help='also verify the complete archived PDF')
    parser.add_argument('--identity-source', type=Path, help='also verify the archived campaign identity page')
    args = parser.parse_args()
    source = args.source or (ROOT / PROOF['page_fixture'])
    expected = PROOF['source_sha256'] if args.source else PROOF['page_fixture_sha256']
    page = PROOF['source_page'] if args.source else 1
    if source.is_symlink() or not source.is_file() or source.stat().st_size > 20 * 1024 * 1024 or digest(source) != expected:
        raise ValueError('source PDF digest or file type differs')
    if args.identity_source:
        if args.identity_source.is_symlink() or digest(args.identity_source) != PROOF['alias_source_sha256']:
            raise ValueError('identity source digest differs')
        raw = args.identity_source.read_bytes()
        if b'Re-elect Jessica Campbell' not in raw or b'Jessica Campbell-Swanson' not in raw:
            raise ValueError('identity source does not contain both names')
    path = os.environ.get('PNUL_EXTRACTOR_PATH', '/usr/bin:/bin')
    executables = {name: shutil.which(name, path=path) for name in ('bwrap', 'prlimit', 'pdfimages', 'pdftotext')}
    if not all(executables.values()):
        raise ValueError('sandboxed PDF inspection tools unavailable')
    with tempfile.TemporaryDirectory(prefix='panopticon-2024-portrait-proof-') as tmp:
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
        text = text.replace('\u2010', '-')
        positions = [text.find(item['source_label']) for item in PROOF['portraits']]
        if 'Board of County Commissioners' not in text or '2024 Budget' not in text or min(positions) < 0 or positions != sorted(positions):
            raise ValueError('county board labels do not match the source page')
        for portrait in PROOF['portraits']:
            extracted = Path(tmp) / f"portrait-{portrait['image_index']:03d}.jpg"
            asset = ROOT / 'web/panopticon-terminal/research-portraits' / f"{portrait['sha256']}.jpg"
            if extracted.is_symlink() or not extracted.is_file() or digest(extracted) != portrait['sha256']:
                raise ValueError('original embedded JPEG digest differs')
            if asset.is_symlink() or not asset.is_file() or extracted.read_bytes() != asset.read_bytes():
                raise ValueError('published portrait differs from the original JPEG stream')
    print('2024 Arapahoe board labels and two original embedded portraits verified.')


if __name__ == '__main__':
    main()
