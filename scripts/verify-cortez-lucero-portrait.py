#!/usr/bin/env python3
"""Verify an exact historical city portrait against its privately captured PDF.

The pinned source's page-2 label was visually checked; this script does not
perform face recognition or establish that a photo depicts a 2021 vote.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('private_page', ROOT/'scripts/private-research-page.py')
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)
spec = importlib.util.spec_from_file_location('garden_portraits', ROOT/'scripts/colorado_garden_portraits.py')
portraits = importlib.util.module_from_spec(spec)
spec.loader.exec_module(portraits)

URL = 'https://www.cortezco.gov/DocumentCenter/View/775/2017-Budget-in-Brief?bidId='
PDF_SHA = 'ab67c9832615adda8ac2dd6e31023476c753002704bb0eedb84a926f826e3023'
IMAGE_SHA = '792f36327372957b5cd20b24fa7d138a9a15715f00b085711c9d334b154a4e0c'
ASSET = ROOT/'web/panopticon-terminal/assets/portraits/cortez-orly-lucero-2017.jpg'
# Exact object header and stream boundaries in the pinned original, validated
# independently with sandboxed pdfimages. A changed source must be re-examined.
HEADER = (b'7 0 obj\r<</BitsPerComponent 8/ColorSpace/DeviceRGB/Filter/DCTDecode/'
          b'Height 270/Interpolate true/Length 9297/Subtype/Image/Type/XObject/'
          b'Width 187>>stream\r\n')
TRAILER = b'\r\nendstream\rendobj\r'


def check_receipt(obs):
    for name in os.listdir(obs):
        try:
            raw = p.read_at(obs, name, 65536)
            if name != hashlib.sha256(raw).hexdigest()+'.json': continue
            value = json.loads(raw)
            canonical = json.dumps(value, sort_keys=True, separators=(',', ':')).encode()
            if raw not in (canonical, canonical+b'\n'): continue
            if p.r.receipt_valid(value, URL, PDF_SHA): return
        except (OSError, ValueError, TypeError):
            continue
    raise ValueError('canonical source receipt unavailable')


def check_embedded(pdf, image):
    if hashlib.sha256(pdf).hexdigest() != PDF_SHA:
        raise ValueError('source PDF digest differs')
    if hashlib.sha256(image).hexdigest() != IMAGE_SHA or len(image) != 9297:
        raise ValueError('portrait digest or size differs')
    if portraits.jpeg_dimensions(image) != (187, 270):
        raise ValueError('portrait dimensions differ')
    if pdf.count(HEADER) != 1 or pdf.count(image) != 1:
        raise ValueError('PDF object or image is ambiguous')
    start = pdf.index(HEADER) + len(HEADER)
    if pdf[start:start+len(image)] != image or pdf[start+len(image):start+len(image)+len(TRAILER)] != TRAILER:
        raise ValueError('portrait is not the pinned embedded PDF object')


def verify(archive, asset=ASSET):
    root = p.open_absolute_dir(archive)
    try:
        info = os.fstat(root)
        if info.st_uid != os.geteuid() or info.st_mode & 0o077:
            raise ValueError('archive must be owner-only')
        content = p.open_dir_at(root, 'content')
        obs = p.open_dir_at(root, 'observations')
        try:
            pdf = p.archive_blob(content, PDF_SHA, 2*1024*1024)
            original = p.archive_blob(content, IMAGE_SHA, 2*1024*1024)
            check_receipt(obs)
            # Pin the checked-in asset to the original embedded bytes.
            fd = os.open(asset, os.O_RDONLY | os.O_NOFOLLOW)
            try:
                if os.fstat(fd).st_size != len(original):
                    raise ValueError('asset size differs')
                with os.fdopen(fd, 'rb') as f:
                    fd = -1
                    image = f.read(len(original)+1)
            finally:
                if fd != -1: os.close(fd)
            if image != original:
                raise ValueError('asset differs from archived original')
            check_embedded(pdf, image)
        finally:
            os.close(obs)
            os.close(content)
    finally:
        os.close(root)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    args = parser.parse_args()
    verify(args.archive)
    print('Verified original portrait and city-source receipt')


if __name__ == '__main__':
    main()
