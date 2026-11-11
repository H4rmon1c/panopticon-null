#!/usr/bin/env python3
"""Verify Littleton's labeled portraits and their lossless display copies."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from portrait_png import ppm_to_png

ROOT = Path(__file__).resolve().parents[1]
PROOF = json.loads((ROOT / 'fixtures/atlas/portrait-sources/littleton-2023-report-proof.json').read_text())


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, help='also verify the archived complete original PDF')
    args = parser.parse_args()
    source = args.source or ROOT / PROOF['page_fixture']
    expected = PROOF['source_sha256'] if args.source else PROOF['page_fixture_sha256']
    page = PROOF['source_page'] if args.source else 1
    if source.is_symlink() or not source.is_file() or source.stat().st_size > 20 * 1024 * 1024 or digest(source.read_bytes()) != expected:
        raise ValueError('source PDF digest or file type differs')
    path = os.environ.get('PNUL_EXTRACTOR_PATH', '/usr/bin:/bin')
    commands = {name: shutil.which(name, path=path) for name in ('bwrap', 'prlimit', 'pdfimages', 'pdftotext')}
    if not all(commands.values()):
        raise ValueError('sandboxed PDF inspection tools unavailable')
    with tempfile.TemporaryDirectory(prefix='panopticon-littleton-portraits-') as tmp:
        command = [commands['prlimit'], '--as=805306368', '--cpu=30', '--fsize=52428800', '--', commands['bwrap']]
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

        base = [commands['pdfimages'], '-f', str(page), '-l', str(page)]
        run(base + ['-all', '/input.pdf', '/output/original'])
        run(base + ['/input.pdf', '/output/rgb'])
        text = run([commands['pdftotext'], '-f', str(page), '-l', str(page), '-layout', '/input.pdf', '-']).decode('utf8')
        board = text[text.rfind('Littleton City Council'):]
        positions = [board.find(item['name']) for item in PROOF['portraits']]
        if not board.startswith('Littleton City Council') or min(positions) < 0 or positions != sorted(positions):
            raise ValueError('labeled council names do not match the source page')
        for portrait in PROOF['portraits']:
            index = portrait['image_index']
            original = Path(tmp) / f'original-{index:03d}.jp2'
            rgb = Path(tmp) / f'rgb-{index:03d}.ppm'
            asset = ROOT / 'web/panopticon-terminal/research-portraits' / f"{portrait['sha256']}.png"
            if any(p.is_symlink() or not p.is_file() for p in (original, rgb, asset)):
                raise ValueError('missing or unsafe portrait file')
            if digest(original.read_bytes()) != portrait['original_sha256']:
                raise ValueError('original embedded JPEG 2000 stream differs')
            image = ppm_to_png(rgb.read_bytes())
            if digest(image) != portrait['sha256'] or image != asset.read_bytes():
                raise ValueError('display portrait differs from the lossless source conversion')
    print('Littleton council labels and seven source-bound portraits verified.')


if __name__ == '__main__':
    main()
