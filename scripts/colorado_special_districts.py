"""Build conservative special-district website leads from a pinned DOLA extract.

The state dataset is a provisional directory, not proof that any district made
a decision. Shared hosts and non-root sites need a tenant-aware adapter before
their records can be attributed to a particular district.
"""
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
from urllib.parse import urlsplit, urlunsplit

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'web/panopticon-terminal/geography/source-archives/dola_websites.json'
SOURCE_URL = ('https://data.colorado.gov/resource/7rar-v5z8.json?'
              '%24select=lgid%2Cname%2Curl%2Clgstatus_i&%24limit=5000&%24order=lgid')
SOURCE_SHA256 = '9976b7a50d23cbb0883eab140e11e4df3276c0882d61387760f492b2de6f8c6b'
SHARED_HOSTS = {'sites.google.com', 'facebook.com', 'www.facebook.com', 'colorado.gov'}


def website(value):
    """Parse a directory URL without accepting credentials or unsafe syntax."""
    if not isinstance(value, str):
        return None
    value = value.strip()
    if not value or len(value) > 2000 or re.search(r'[\s\\\x00-\x1f\x7f]', value):
        return None
    if '://' not in value:
        value = 'https://' + value
    try:
        part = urlsplit(value)
        host = (part.hostname or '').lower()
        if (part.scheme.lower() not in ('http', 'https') or not host or part.username or part.password
                or part.port not in (None, 80, 443)
                or not re.fullmatch(r'(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}', host)):
            return None
        return part, host.removeprefix('www.')
    except ValueError:
        return None


def build(source, geography, other_hosts=()):
    rows = json.loads(source)
    if not isinstance(rows, list) or len(rows) > 5000 or not all(isinstance(row, dict) for row in rows):
        raise ValueError('invalid bounded DOLA website extract')
    districts = {item['code']: item for item in geography if item['kind'] == 'district_provisional'}
    if len(districts) != sum(item['kind'] == 'district_provisional' for item in geography):
        raise ValueError('duplicate district geography ID')
    ids = [row.get('lgid') for row in rows if row.get('lgid')]
    if any(not isinstance(value, str) or not re.fullmatch(r'\d{5}', value) for value in ids) or len(ids) != len(set(ids)):
        raise ValueError('invalid or duplicate DOLA district ID')
    # Count all advertised hosts, including non-root and unmapped records. A
    # shared domain is never assigned to one district as if it were exclusive.
    host_counts = Counter(parsed[1] for row in rows if (parsed := website(row.get('url'))))
    excluded = {str(host).lower().removeprefix('www.') for host in other_hosts}
    excluded.update(host.removeprefix('www.') for host in SHARED_HOSTS)
    leads = []
    for row in rows:
        code = row.get('lgid')
        item = districts.get(code)
        parsed = website(row.get('url'))
        if (not item or not parsed or row.get('lgstatus_i') != '1.0'
                or row.get('name') != item['name'] or item['id'] != f'dola-district-{code}'):
            continue
        part, host = parsed
        if (host_counts[host] != 1 or host in excluded or part.path not in ('', '/')
                or part.query or part.fragment or host.endswith('.legistar.com')):
            continue
        url = urlunsplit(('https', part.hostname.lower(), '/', '', ''))
        leads.append((item['id'], item['name'], url))
    leads.sort()
    return leads


def pinned_source():
    if SOURCE.is_symlink() or not SOURCE.is_file() or SOURCE.stat().st_size > 2_000_000:
        raise ValueError('DOLA website extract is missing or unsafe')
    data = SOURCE.read_bytes()
    if hashlib.sha256(data).hexdigest() != SOURCE_SHA256:
        raise ValueError('DOLA website extract differs from its pinned digest')
    return data
