#!/usr/bin/env python3
"""Verify the city's row labels, image placement, and original JPEG streams."""
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
PROOF = json.loads((ROOT / 'fixtures/atlas/portrait-sources/commerce-city-2022-report-proof.json').read_text())


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
    with tempfile.TemporaryDirectory(prefix='panopticon-commerce-portraits-') as tmp:
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
        text = run([commands['pdftotext'], *base, '-layout', '/input.pdf', '-']).decode('utf8')
        normalized = ' '.join(text.split())
        captions = [
            'Top Row: Benjamin Huseman, Mayor; Jennifer Allen-Thomas, Mayor Pro Tem; Oscar Madera, Ward I',
            'Middle Row: Renée Chacon, Ward III; Susan Noble, Ward IV; Craig Hurst, At-Large',
            'Bottom Row: Sean Ford, At-Large; Kristi Douglas, At-Large; Craig Kim, At-Large',
        ]
        if not all(line in normalized for line in captions):
            raise ValueError('city report portrait row captions differ')
        names = [['Benjamin Huseman', 'Jennifer Allen-Thomas', 'Oscar Madera'],
                 ['Renée Chacon', 'Susan Noble', 'Craig Hurst'],
                 ['Sean Ford', 'Kristi Douglas', 'Craig Kim']]
        layout = ET.parse(Path(tmp) / 'layout.xml').getroot()
        images = sorted(layout.findall('.//image'), key=lambda x: int(x.attrib['top']))
        if len(images) != 9:
            raise ValueError('city report must have nine portrait images')
        rows = [sorted(images[i:i + 3], key=lambda x: int(x.attrib['left'])) for i in (0, 3, 6)]
        for row in rows:
            if max(int(x.attrib['top']) for x in row) - min(int(x.attrib['top']) for x in row) > 3:
                raise ValueError('city report portrait row placement differs')
        for portrait in PROOF['portraits']:
            row, column = portrait['row'], portrait['column']
            if type(row) is not int or type(column) is not int or not 0 <= row <= 2 or not 0 <= column <= 2:
                raise ValueError('invalid portrait position')
            if names[row][column] != portrait['name']:
                raise ValueError('portrait name does not match the labeled row position')
            original = Path(tmp) / f"original-{portrait['image_index']:03d}.jpg"
            positioned = Path(tmp) / Path(rows[row][column].attrib['src']).name
            asset = ROOT / 'web/panopticon-terminal/research-portraits' / f"{portrait['sha256']}.jpg"
            if any(p.is_symlink() or not p.is_file() for p in (original, positioned, asset)):
                raise ValueError('missing or unsafe portrait file')
            if any(digest(p.read_bytes()) != portrait['sha256'] for p in (original, positioned, asset)):
                raise ValueError('labeled portrait position or embedded JPEG stream differs')
    print('Commerce City row labels and five original source-bound portraits verified.')


if __name__ == '__main__':
    main()
