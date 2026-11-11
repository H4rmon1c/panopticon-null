#!/usr/bin/env python3
"""Acquire the city's labeled 2023 council portraits for existing Denver votes.

This is an explicit official-source adapter, not a face-recognition system. It
preserves exact response bytes and transport observations and matches one full
name (optionally including a middle initial) to one named voter in the released
Denver roll call. It never infers a vote from a photograph.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import http.client
import ipaddress
import json
import os
from pathlib import Path
import re
import socket
import ssl
import time
from html.parser import HTMLParser
from urllib.parse import urljoin, urlsplit
from urllib.robotparser import RobotFileParser

ROOT = Path(__file__).resolve().parents[1]
HOST = 'www.denvergov.org'
ORIGIN = f'https://{HOST}'
GALLERY = ORIGIN + '/Government/Agencies-Departments-Offices/Agencies-Departments-Offices-Directory/Denver-City-Council/Press-Room/2023councilofficialphotos'
IMAGE_PREFIX = '/files/assets/public/v/1/city-council/images/central/2023-member-photos/2023_16-9_web/'
AGENT = 'PanopticonNull/0.0.4 (+https://github.com/H4rmon1c/panopticon-null)'
MAX_BYTES = 2_000_000


def digest(data):
    return hashlib.sha256(data).hexdigest()


def canonical_name(value):
    """Only an optional single middle initial may differ; no fuzzy matching."""
    words = value.strip().split()
    if len(words) >= 3 and re.fullmatch(r'[A-Za-z]\.', words[1]):
        words.pop(1)
    return ' '.join(words).casefold()


def checked_path(url):
    parts = urlsplit(url)
    if (parts.scheme != 'https' or parts.hostname != HOST or parts.port not in (None, 443)
            or parts.username or parts.password or parts.query or parts.fragment
            or len(url) > 2048 or not parts.path.startswith('/') or '..' in parts.path.split('/')):
        raise ValueError('URL is not an exact HTTPS Denver source')
    return parts.path


def public_addresses():
    addresses = sorted({item[4][0] for item in socket.getaddrinfo(HOST, 443, type=socket.SOCK_STREAM)})
    if not addresses:
        raise ValueError('Denver host has no addresses')
    for address in addresses:
        ip = ipaddress.ip_address(address)
        if not ip.is_global or (ip.version == 6 and ip in ipaddress.ip_network('3fff::/20')):
            raise ValueError('Denver host has a non-public DNS answer')
    return addresses


def fetch(url, maximum=MAX_BYTES, rules=None, depth=0):
    """Resolve once, validate every answer, pin the connected IP, and verify TLS."""
    path = checked_path(url)
    addresses = public_addresses()
    last = None
    for address in addresses:
        raw = None
        conn = None
        try:
            raw = socket.create_connection((address, 443), timeout=12)
            conn = ssl.create_default_context().wrap_socket(raw, server_hostname=HOST)
            raw = None
            conn.settimeout(20)
            request = f'GET {path} HTTP/1.1\r\nHost: {HOST}\r\nUser-Agent: {AGENT}\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n'
            conn.sendall(request.encode('ascii'))
            response = http.client.HTTPResponse(conn)
            response.begin()
            if response.status in (301, 302, 307, 308):
                destination = urljoin(url, response.getheader('Location', ''))
                target_path = checked_path(destination)
                normalize = lambda value: re.sub(r'/v/[0-9]+/', '/v/VERSION/', value, count=1)
                if (depth >= 2 or normalize(path) != normalize(target_path)
                        or (rules is not None and not rules.can_fetch(AGENT, destination))):
                    raise ValueError('Redirect leaves the exact versioned Denver asset')
                conn.close()
                conn = None
                time.sleep(1)
                data, observation = fetch(destination, maximum, rules, depth + 1)
                observation.setdefault('redirects', []).insert(0, {
                    'url': url, 'status': response.status, 'target': destination,
                    'resolved_ips': addresses, 'connected_ip': address,
                })
                return data, observation
            if response.status != 200:
                raise ValueError(f'HTTP {response.status} for {path}')
            size = response.getheader('Content-Length')
            if size is not None and (not size.isdigit() or int(size) > maximum):
                raise ValueError('Response exceeds byte limit')
            data = response.read(maximum + 1)
            if len(data) > maximum:
                raise ValueError('Response exceeds byte limit')
            return data, {'url': url, 'retrieved_at': datetime.now(timezone.utc).isoformat(),
                          'status': response.status, 'resolved_ips': addresses,
                          'connected_ip': address, 'content_type': response.getheader('Content-Type', ''),
                          'bytes': len(data), 'sha256': digest(data)}
        except (OSError, ssl.SSLError, http.client.HTTPException) as error:
            last = error
        finally:
            if conn is not None:
                conn.close()
            if raw is not None:
                raw.close()
    raise RuntimeError('Denver source connection failed') from last


class Gallery(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.anchor = None
        self.rows = []

    def handle_starttag(self, tag, attrs):
        values = dict(attrs)
        if tag == 'a':
            self.anchor = values.get('href')
        elif tag == 'img' and self.anchor:
            link = urljoin(GALLERY, self.anchor)
            path = urlsplit(link).path
            if path.startswith(IMAGE_PREFIX) and path.endswith('_16-9web.jpg'):
                src = urlsplit(urljoin(GALLERY, values.get('src', '')))
                if src.hostname == HOST and src.path == path and values.get('alt'):
                    self.rows.append((link, values['alt']))

    def handle_endtag(self, tag):
        if tag == 'a':
            self.anchor = None


def preserve(directory, content):
    root = directory / 'content'
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    if root.is_symlink() or not root.is_dir():
        raise ValueError('content directory must be a real directory')
    path = root / digest(content)
    if path.exists():
        if path.is_symlink() or path.read_bytes() != content:
            raise ValueError('Existing preserved content differs or is a symlink')
    else:
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, 'wb') as out:
            out.write(content)
            out.flush()
            os.fsync(out.fileno())
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data-dir', required=True, type=Path)
    args = parser.parse_args()
    if not args.data_dir.is_absolute():
        raise ValueError('data directory must be absolute')
    args.data_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    if args.data_dir.is_symlink():
        raise ValueError('data directory cannot be a symlink')
    receipts = []
    robots, receipt = fetch(ORIGIN + '/robots.txt')
    preserve(args.data_dir, robots)
    receipts.append(receipt)
    rules = RobotFileParser()
    rules.parse(robots.decode('utf8').splitlines())
    if not rules.can_fetch(AGENT, GALLERY):
        raise ValueError('Denver robots file excludes the gallery')
    time.sleep(1)
    gallery, receipt = fetch(GALLERY)
    if 'text/html' not in receipt['content_type'].lower():
        raise ValueError('Gallery response is not HTML')
    preserve(args.data_dir, gallery)
    receipts.append(receipt)
    parsed = Gallery()
    parsed.feed(gallery.decode('utf8'))
    entries = {}
    for url, alt in parsed.rows:
        key = canonical_name(alt.split(',')[0])
        if key in entries:
            raise ValueError(f'Ambiguous gallery name: {key}')
        entries[key] = (url, alt)
    snapshot = json.loads((ROOT / 'web/panopticon-terminal/public/public-snapshot.json').read_text())
    decision = next(x for x in snapshot['entities'] if x.get('slug', '').startswith('co-denver-25-0413-'))
    voters = {x['id']: x for x in snapshot['entities'] if x.get('type') == 'RECORDED_VOTER'}
    result = {}
    fixture = ROOT / 'fixtures/atlas/portrait-sources/denver-2023-council-gallery.html'
    fixture.write_bytes(gallery)
    for vote in decision['votes']:
        voter = voters[vote['voter_entity_id']]
        if voter['name'] != vote['name'] or voter['identity_scope'] != 'legistar:denver':
            raise ValueError('Vote and official voter identity do not agree')
        url, alt = entries[canonical_name(vote['name'])]
        checked_path(url)
        if not rules.can_fetch(AGENT, url):
            raise ValueError('Denver robots file excludes a portrait')
        time.sleep(1)
        image, observation = fetch(url, rules=rules)
        if not image.startswith(b'\xff\xd8\xff') or not observation['content_type'].lower().startswith('image/jpeg'):
            raise ValueError('Portrait response is not a JPEG')
        preserve(args.data_dir, image)
        receipts.append(observation)
        filename = 'denver-' + re.sub(r'[^a-z0-9]+', '-', canonical_name(vote['name'])).strip('-') + '-original.jpg'
        (ROOT / 'web/panopticon-terminal/portraits' / filename).write_bytes(image)
        result[voter['id']] = {
            'name': vote['name'], 'jurisdiction_ref': voter['jurisdiction_ref'],
            'identity_scope': voter['identity_scope'], 'official_person_id': voter['official_person_id'],
            'src': './portraits/' + filename, 'source_url': GALLERY, 'image_url': observation['url'],
            'display_image_url': url, 'source_capture_kind': 'official_gallery',
            'source_page_fixture': 'fixtures/atlas/portrait-sources/' + fixture.name,
            'source_page_sha256': digest(gallery), 'source_image_alt': alt,
            'source_page_locator': f'a[href="{urlsplit(url).path}"] > figure img[alt="{alt}"]',
            'credit': 'City and County of Denver', 'sha256': digest(image),
            'retrieved_on': datetime.now(timezone.utc).date().isoformat(),
            'portrait_date': 'not_established', 'verification_state': 'SOURCE_BYTES_VERIFIED',
        }
        print(f"Archived official portrait: {vote['name']}", flush=True)
    receipt_path = args.data_dir / ('denver-portrait-receipts-' + digest(gallery) + '.json')
    receipt_path.write_text(json.dumps(receipts, indent=2) + '\n')
    os.chmod(receipt_path, 0o600)
    output = ROOT / 'fixtures/atlas/portrait-sources/denver-2023-portrait-registry.json'
    output.write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n')
    module = ROOT / 'web/panopticon-terminal/portraits/denver-2023.js'
    module.write_text('// Generated from the preserved official gallery and recorded Denver voters.\n'
                      + 'export const denverPortraits = Object.freeze('
                      + json.dumps(result, indent=2, ensure_ascii=False) + ');\n')
    print(f'Preserved {len(result)} labeled original portraits; registry: {output}')


if __name__ == '__main__':
    main()
