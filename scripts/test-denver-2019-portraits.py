#!/usr/bin/env python3
"""Verify Denver's labeled council layout and original JPEG streams."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
PROOF = json.loads((ROOT / 'fixtures/atlas/portrait-sources/denver-2019-budget-proof.json').read_text())


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
    commands = {name: shutil.which(name, path=path) for name in ('bwrap', 'prlimit', 'pdfimages', 'pdftotext', 'pdftohtml')}
    if not all(commands.values()):
        raise ValueError('sandboxed PDF inspection tools unavailable')
    with tempfile.TemporaryDirectory(prefix='panopticon-denver-portraits-') as tmp:
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
        base = ['-f', str(page), '-l', str(page)]
        run([commands['pdfimages'], *base, '-j', '/input.pdf', '/output/original'])
        run([commands['pdftohtml'], *base, '-q', '-xml', '-hidden', '/input.pdf', '/output/layout'])
        text = ' '.join(run([commands['pdftotext'], *base, '-layout', '/input.pdf', '-']).decode('utf8').split())
        captions = [
            'Jolon Clark, President Stacie Gilmore, Pro-Tem Rafael Espinoza',
            'Kevin Flynn Paul D. López Kendra Black',
            'Mary Beth Susman Paul Kashmann Christopher Herndon',
            'Albus Brooks Wayne New Robin Kniech Deborah "Debbie" Ortega',
        ]
        if not all(line in text for line in captions):
            raise ValueError('Denver council portrait labels or row order differ')
        names = [['Jolon Clark, President', 'Stacie Gilmore, Pro-Tem', 'Rafael Espinoza'],
                 ['Kevin Flynn', 'Paul D. López', 'Kendra Black'],
                 ['Mary Beth Susman', 'Paul Kashmann', 'Christopher Herndon'],
                 ['Albus Brooks', 'Wayne New', 'Robin Kniech', 'Deborah "Debbie" Ortega']]
        layout = ET.parse(Path(tmp) / 'layout.xml').getroot()
        images = sorted(layout.findall('.//image'), key=lambda node: int(node.attrib['top']))
        if len(images) != 13:
            raise ValueError('Denver council roster must have thirteen portrait images')
        bounds = [(0, 3), (3, 6), (6, 9), (9, 13)]
        rows = [sorted(images[start:end], key=lambda node: int(node.attrib['left'])) for start, end in bounds]
        for row in rows:
            if max(int(node.attrib['top']) for node in row) - min(int(node.attrib['top']) for node in row) > 3:
                raise ValueError('Denver council portrait row placement differs')
        for portrait in PROOF['portraits']:
            row, column = portrait['row'], portrait['column']
            if type(row) is not int or type(column) is not int or not 0 <= row < len(rows) or not 0 <= column < len(rows[row]):
                raise ValueError('invalid portrait position')
            if names[row][column] != portrait['source_label']:
                raise ValueError('portrait label does not match the source row position')
            original = Path(tmp) / f"original-{portrait['image_index']:03d}.jpg"
            positioned = Path(tmp) / Path(rows[row][column].attrib['src']).name
            asset = ROOT / 'web/panopticon-terminal/research-portraits' / f"{portrait['sha256']}.jpg"
            if any(item.is_symlink() or not item.is_file() for item in (original, positioned, asset)):
                raise ValueError('missing or unsafe portrait file')
            if any(digest(item.read_bytes()) != portrait['sha256'] for item in (original, positioned, asset)):
                raise ValueError('labeled portrait position or original JPEG stream differs')
    print('Denver council row labels and ten original portrait streams verified.')


if __name__ == '__main__':
    main()
