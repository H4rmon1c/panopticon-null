#!/usr/bin/env python3
"""Discover Colorado public meeting portals and screen their public matters.

Results are preliminary source mentions, never inferred approvals or votes.
Raw responses and receipts stay in a private, resumable local store. This
machine-led discovery does not create or impersonate source-review attestations.
"""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone, timedelta
from functools import lru_cache
import hashlib
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import sys
import threading
import time
import tomllib
from urllib.parse import parse_qs, urlencode, urljoin, urlsplit, urlunsplit
from urllib.robotparser import RobotFileParser

from public_http import USER_AGENT, BudgetExhausted, get, public_ip

ROOT = Path(__file__).resolve().parents[1]
GEOGRAPHY = ROOT / 'web/panopticon-terminal/geography/colorado-jurisdictions.json'
DIRECTORY = ROOT / 'configs/states/co-municipal-websites.v1.json'
CALENDARS = ROOT / 'configs/states/co-calendar-geography.v1.json'
COUNTY_DIRECTORIES = ('https://dmv.colorado.gov/county-offices',
                      'https://www.clerkandrecorder.org/find-your-county-clerk')
# The clerks' directory omitted this county website. The lead is the county's
# own published government homepage; it still receives the normal source checks.
COUNTY_FALLBACKS = {'us-co-county-08041': ('El Paso County', 'https://www.elpasoco.com/')}
TERMS = ('Flock', 'Axon', 'Palantir', 'ShotSpotter', 'SoundThinking', 'Clearview AI',
         'Cellebrite', 'license plate reader', 'license plate recognition',
         'facial recognition', 'drone as first responder', 'cell site simulator')
TERM_VERSION = 'colorado-legistar-title-v1'
API_HOST = 'webapi.legistar.com'
MAX_PAGE_BYTES = 2_000_000
MAX_RESULTS = 50_000
ARCHIVE_LOCK = threading.Lock()


def utcnow():
    return datetime.now(timezone.utc).isoformat()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def require_storage_headroom(path):
    capacity = os.statvfs(path)
    if capacity.f_bavail * capacity.f_frsize < 2 * 1024**3 or capacity.f_favail < 4096:
        raise ValueError('research paused: private storage has less than 2 GiB or 4096 free inodes')


def root_store(path):
    if not path.is_absolute():
        raise ValueError('private data directory must be absolute')
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    if path.is_symlink() or not path.is_dir():
        raise ValueError('private data directory must be a real directory')
    public = (ROOT / 'web/panopticon-terminal').resolve()
    if path.resolve() == public or public in path.resolve().parents:
        raise ValueError('private data directory must not be under the public web root')
    os.chmod(path, 0o700)
    content = path / 'content'
    content.mkdir(mode=0o700, exist_ok=True)
    if content.is_symlink() or not content.is_dir():
        raise ValueError('content directory must be a real directory')
    database = path / 'research.sqlite3'
    if database.is_symlink():
        raise ValueError('database cannot be a symlink')
    db = sqlite3.connect(database, timeout=20)
    os.chmod(database, 0o600)
    db.executescript('''
      PRAGMA journal_mode=WAL;
      CREATE TABLE IF NOT EXISTS discovery (
        jurisdiction_ref TEXT PRIMARY KEY, source_url TEXT NOT NULL,
        checked_at TEXT NOT NULL, result_json TEXT NOT NULL
      );
      CREATE TABLE IF NOT EXISTS discovery_attempts (
        jurisdiction_ref TEXT PRIMARY KEY, attempted_at TEXT NOT NULL,
        state TEXT NOT NULL, error TEXT
      );
      CREATE TABLE IF NOT EXISTS civic_history_checks (
        jurisdiction_ref TEXT NOT NULL, host TEXT NOT NULL, category INTEGER NOT NULL,
        year INTEGER NOT NULL, selector_json TEXT NOT NULL, attempted_at TEXT NOT NULL,
        state TEXT NOT NULL, error TEXT, receipt_json TEXT,
        PRIMARY KEY(jurisdiction_ref,host,category,year)
      );
      CREATE TABLE IF NOT EXISTS portals (
        jurisdiction_ref TEXT NOT NULL, client TEXT NOT NULL, proof_json TEXT NOT NULL,
        PRIMARY KEY(jurisdiction_ref,client)
      );
      CREATE TABLE IF NOT EXISTS scans (
        client TEXT NOT NULL, page INTEGER NOT NULL, receipt_json TEXT NOT NULL,
        PRIMARY KEY(client,page)
      );
      CREATE TABLE IF NOT EXISTS scan_progress (
        client TEXT PRIMARY KEY, next_page INTEGER NOT NULL,
        page_size INTEGER NOT NULL, attempted_at TEXT NOT NULL
      );
      CREATE TABLE IF NOT EXISTS scan_history (
        client TEXT NOT NULL, page INTEGER NOT NULL, source_sha256 TEXT NOT NULL,
        receipt_json TEXT NOT NULL, PRIMARY KEY(client,page,source_sha256)
      );
      CREATE TABLE IF NOT EXISTS leads (
        id TEXT PRIMARY KEY, jurisdiction_ref TEXT NOT NULL, client TEXT NOT NULL,
        matter_id INTEGER NOT NULL, page INTEGER NOT NULL, row_index INTEGER NOT NULL,
        source_sha256 TEXT NOT NULL, record_json TEXT NOT NULL, updated_at TEXT NOT NULL
      );
      CREATE TABLE IF NOT EXISTS document_candidates (
        jurisdiction_ref TEXT NOT NULL, url TEXT NOT NULL, candidate_json TEXT NOT NULL,
        state TEXT NOT NULL DEFAULT 'pending', attempts INTEGER NOT NULL DEFAULT 0,
        result_json TEXT, updated_at TEXT NOT NULL,
        PRIMARY KEY(jurisdiction_ref,url)
      );
      CREATE TABLE IF NOT EXISTS official_portraits (
        jurisdiction_ref TEXT NOT NULL, name TEXT NOT NULL, portrait_json TEXT NOT NULL,
        PRIMARY KEY(jurisdiction_ref,name)
      );
      CREATE TABLE IF NOT EXISTS portrait_attempts (
        jurisdiction_ref TEXT PRIMARY KEY, attempted_at TEXT NOT NULL
      );
      CREATE TABLE IF NOT EXISTS document_history (
        jurisdiction_ref TEXT NOT NULL, url TEXT NOT NULL, source_sha256 TEXT NOT NULL,
        candidate_json TEXT NOT NULL, result_json TEXT NOT NULL, captured_at TEXT NOT NULL,
        superseded_at TEXT NOT NULL, PRIMARY KEY(jurisdiction_ref,url,source_sha256)
      );
      CREATE TABLE IF NOT EXISTS matter_action_checks (
        client TEXT NOT NULL, matter_id INTEGER NOT NULL, checked_at TEXT NOT NULL,
        status TEXT NOT NULL, PRIMARY KEY(client,matter_id)
      );
      CREATE TABLE IF NOT EXISTS matter_actions (
        client TEXT NOT NULL, matter_id INTEGER NOT NULL, history_id INTEGER NOT NULL,
        captures_json TEXT NOT NULL, updated_at TEXT NOT NULL,
        PRIMARY KEY(client,matter_id,history_id)
      );
      CREATE TABLE IF NOT EXISTS matter_action_history (
        client TEXT NOT NULL, matter_id INTEGER NOT NULL, history_id INTEGER NOT NULL,
        captures_sha256 TEXT NOT NULL, captures_json TEXT NOT NULL, superseded_at TEXT NOT NULL,
        PRIMARY KEY(client,matter_id,history_id,captures_sha256)
      );
      CREATE TABLE IF NOT EXISTS suite_meetings (
        meeting_id INTEGER PRIMARY KEY, candidate_json TEXT NOT NULL, state TEXT NOT NULL,
        result_json TEXT, updated_at TEXT NOT NULL, last_attempt_at TEXT, last_error TEXT
      );
      CREATE TABLE IF NOT EXISTS suite_history (
        meeting_id INTEGER NOT NULL, source_sha256 TEXT NOT NULL, candidate_json TEXT NOT NULL,
        result_json TEXT NOT NULL, updated_at TEXT NOT NULL,
        PRIMARY KEY(meeting_id,source_sha256)
      );
      CREATE TABLE IF NOT EXISTS ocr_attempts (
        jurisdiction_ref TEXT NOT NULL, url TEXT NOT NULL, attempted_at TEXT NOT NULL, status TEXT NOT NULL,
        PRIMARY KEY(jurisdiction_ref,url)
      );
      CREATE TABLE IF NOT EXISTS ocr_history (
        jurisdiction_ref TEXT NOT NULL, url TEXT NOT NULL, attempted_at TEXT NOT NULL,
        prior_result_json TEXT NOT NULL, outcome TEXT NOT NULL,
        PRIMARY KEY(jurisdiction_ref,url,attempted_at)
      );
      CREATE TABLE IF NOT EXISTS docx_attempts (
        jurisdiction_ref TEXT NOT NULL, url TEXT NOT NULL, attempted_at TEXT NOT NULL, status TEXT NOT NULL,
        PRIMARY KEY(jurisdiction_ref,url)
      );
      CREATE TABLE IF NOT EXISTS docx_history (
        jurisdiction_ref TEXT NOT NULL, url TEXT NOT NULL, attempted_at TEXT NOT NULL,
        prior_result_json TEXT NOT NULL, outcome TEXT NOT NULL,
        PRIMARY KEY(jurisdiction_ref,url,attempted_at)
      );
    ''')
    columns = {row[1] for row in db.execute('PRAGMA table_info(document_candidates)')}
    for name in ('last_attempt_at', 'last_error'):
        if name not in columns:
            db.execute(f'ALTER TABLE document_candidates ADD COLUMN {name} TEXT')
    # Older successful screens predate the embedded anchor proof. Preserve
    # their current source-index binding before later discovery refreshes it.
    legacy = db.execute('''SELECT jurisdiction_ref,url,candidate_json,result_json FROM document_candidates
        WHERE state IN ('matched','no_match') AND CASE WHEN json_valid(result_json)
          THEN json_type(result_json,'$.candidate') END IS NULL''').fetchall()
    for ref, url, candidate_raw, result_raw in legacy:
        try:
            candidate, result = json.loads(candidate_raw), json.loads(result_raw)
            if not isinstance(result, dict) or not isinstance(candidate, dict) or candidate.get('url') != url:
                continue
            result['candidate'] = candidate
            db.execute('UPDATE document_candidates SET result_json=? WHERE jurisdiction_ref=? AND url=?',
                       (json.dumps(result), ref, url))
        except (ValueError, TypeError, KeyError, json.JSONDecodeError):
            continue
    if 'history_sha256' not in {row[1] for row in db.execute('PRAGMA table_info(matter_action_checks)')}:
        db.execute('ALTER TABLE matter_action_checks ADD COLUMN history_sha256 TEXT')
    for client, matter, raw in db.execute('''SELECT a.client,a.matter_id,a.captures_json FROM matter_actions a
            JOIN matter_action_checks c ON a.client=c.client AND a.matter_id=c.matter_id WHERE c.history_sha256 IS NULL''').fetchall():
        try:
            digest = json.loads(raw)['history']['sha256']
            if re.fullmatch(r'[0-9a-f]{64}', digest):
                db.execute('UPDATE matter_action_checks SET history_sha256=? WHERE client=? AND matter_id=? AND history_sha256 IS NULL',
                           (digest, client, matter))
        except (ValueError, TypeError, KeyError, json.JSONDecodeError):
            continue
    for client, page, raw in db.execute('SELECT client,page,receipt_json FROM scans').fetchall():
        receipt = json.loads(raw)
        db.execute('INSERT OR IGNORE INTO scan_history VALUES (?,?,?,?)',
                   (client, page, receipt['sha256'], raw))
    db.commit()
    return db


def keep(path, data):
    key = sha(data)
    target = path / 'content' / key
    try:
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    except FileExistsError:
        if sha(read_blob(target, 20 * 1024 * 1024)) != key:
            raise ValueError('preserved content collision or unsafe path')
    else:
        with os.fdopen(fd, 'wb') as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
    return key


def archive(path, observations):
    with ARCHIVE_LOCK:
        directory = path / 'observations'
        directory.mkdir(mode=0o700, exist_ok=True)
        if directory.is_symlink() or not directory.is_dir():
            raise ValueError('observation directory must be a real directory')
        for receipt, data in observations:
            if keep(path, data) != receipt['sha256']:
                raise ValueError('receipt digest differs from preserved bytes')
            encoded = (json.dumps(receipt, sort_keys=True, separators=(',', ':')) + '\n').encode()
            target = directory / (sha(encoded) + '.json')
            try:
                fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            except FileExistsError:
                if read_blob(target, 65_536) != encoded:
                    raise ValueError('observation record differs or is a symlink')
            else:
                with os.fdopen(fd, 'wb') as handle:
                    handle.write(encoded)
                    handle.flush()
                    os.fsync(handle.fileno())


class Links(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.links = []
        self.current = None

    def handle_starttag(self, tag, attrs):
        values = dict(attrs)
        if tag == 'a' and 'href' in values:
            self.current = [values['href'], '']
        elif tag == 'iframe' and 'src' in values and len(self.links) < 10000:
            self.links.append((values['src'], 'iframe'))

    def handle_data(self, data):
        if self.current is not None:
            self.current[1] += data[:500]

    def handle_endtag(self, tag):
        if tag == 'a' and self.current is not None:
            if len(self.links) < 10000:
                self.links.append(tuple(self.current))
            self.current = None


def parse_links(body):
    parser = Links()
    parser.feed(body.decode('utf8', 'replace'))
    return parser.links


class Images(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.images = []

    def handle_starttag(self, tag, attrs):
        values = dict(attrs)
        if tag == 'img' and values.get('src') and values.get('alt') and len(self.images) < 10000:
            self.images.append((values['src'], values['alt']))


def parse_images(body):
    parser = Images()
    parser.feed(body.decode('utf8', 'replace'))
    return parser.images


class OfficialProfile(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.in_heading = False
        self.heading = ''
        self.headings = []
        self.images = []

    def handle_starttag(self, tag, attrs):
        if tag == 'h1':
            self.in_heading = True
            self.heading = ''
        if tag == 'img' and len(self.images) < 1000:
            values = dict(attrs)
            if values.get('alt') and values.get('srcset'):
                self.images.append((values['alt'], values['srcset']))

    def handle_data(self, data):
        if self.in_heading:
            self.heading += data[:500]

    def handle_endtag(self, tag):
        if tag == 'h1' and self.in_heading:
            self.headings.append(' '.join(self.heading.split()))
            self.in_heading = False


def official_profile_image(body, name):
    parser = OfficialProfile()
    parser.feed(body.decode('utf8', 'replace'))
    heading = re.compile(r'^Commissioner\s+' + re.escape(name) + r',\s*District\s+[IVX]+$', re.I)
    if sum(bool(heading.fullmatch(value)) for value in parser.headings) != 1:
        return None
    images = [(alt, srcset) for alt, srcset in parser.images if alt.casefold() == ('Commissioner ' + name).casefold()]
    if len(images) != 1:
        return None
    options = []
    for part in images[0][1].split(','):
        match = re.fullmatch(r'\s*(https://www\.douglasco\.gov/[^\s?#]+)\s+([1-9][0-9]{0,4})w\s*', part)
        if match:
            options.append((int(match.group(2)), match.group(1)))
    return (max(options)[1], images[0][0]) if options else None


class RobotsDenied(ValueError):
    def __init__(self, status, observations):
        super().__init__(f'robots HTTP {status}')
        self.observations = observations


def get_robots(host, hosts=None, budget=None):
    observations = get(f'https://{host}/robots.txt', hosts or {host}, maximum=MAX_PAGE_BYTES, budget=budget)
    status = observations[-1][0]['status']
    if status == 404:
        return None, observations
    if status != 200:
        raise RobotsDenied(status, observations)
    rules = RobotFileParser()
    rules.parse(observations[-1][1].decode('utf8').splitlines())
    return rules, observations


def allowed(rules, url):
    return rules is None or rules.can_fetch(USER_AGENT, url)


def html_page(url, host, rules, budget=None):
    if not allowed(rules, url):
        raise ValueError('robots disallows this path')
    observations = get(url, {host}, maximum=MAX_PAGE_BYTES, redirects=0, budget=budget)
    meta, body = observations[-1]
    if meta['status'] != 200 or 'html' not in meta['content_type'].lower():
        raise ValueError(f"not an HTML page (HTTP {meta['status']})")
    return body, observations


def bootstrap_counties(data_dir, db, budget=None):
    counties = {item['name'].casefold(): item for item in json.loads(GEOGRAPHY.read_text())['jurisdictions']
                if item['kind'] == 'county'}
    found = {}
    sources = []
    for directory in COUNTY_DIRECTORIES:
        try:
            host = urlsplit(directory).hostname
            rules, robots = get_robots(host, budget=budget)
            archive(data_dir, robots)
            body, receipts = html_page(directory, host, rules, budget)
            archive(data_dir, receipts)
        except (OSError, UnicodeError, ValueError, RuntimeError) as error:
            print(f'County directory unavailable: {urlsplit(directory).hostname} ({str(error)[:100]})', flush=True)
            continue
        sources.append({'url': directory, 'sha256': sha(body)})
        for href, text in parse_links(body):
            label = ' '.join(text.split())
            # The clerks' association uses "Visit county website — Adams";
            # the state DMV list uses "Adams County".
            if 'clerkandrecorder.org' in directory:
                match = re.fullmatch(r'Visit county website\s*↗\s*—\s*(.+)', label, re.I)
                if not match:
                    continue
                label = match.group(1).strip() + ' County'
            key = label.casefold()
            url = urljoin(directory, href)
            try:
                parsed = urlsplit(url)
                if key not in counties or parsed.scheme not in ('http', 'https') or not parsed.hostname:
                    continue
            except ValueError:
                continue
            # These are directory links, not verified county-wide source reach.
            found.setdefault(counties[key]['id'], (label, url))
    for ref, lead in COUNTY_FALLBACKS.items():
        found.setdefault(ref, lead)
    result = {'sources': sources, 'leads': found}
    (data_dir / 'county-directory.json').write_text(json.dumps(result, indent=2) + '\n')
    print(f'County directory: {len(found)} of {len(counties)} county website leads', flush=True)
    return found


def normalized_home(raw):
    value = urlsplit(raw)
    if value.scheme not in ('http', 'https') or not value.hostname or value.username or value.password:
        raise ValueError('invalid directory URL')
    # Prefer verified TLS even when the archival directory has an HTTP URL.
    if value.port not in (None, 80, 443):
        raise ValueError('nonstandard port in directory URL')
    return urlunsplit(('https', value.hostname, value.path or '/', value.query, ''))


def candidate_kind(url, index_url, label):
    parsed, index = urlsplit(url), urlsplit(index_url)
    # Admit ordinary encoded filename spaces without admitting encoded path
    # separators, dot segments, controls, or double-encoded escapes.
    if (parsed.scheme != 'https' or parsed.hostname != index.hostname or parsed.port not in (None, 443)
            or parsed.username or parsed.password or parsed.query or parsed.fragment or len(url) > 2000
            or re.search(r'%(?!20)', parsed.path, re.I) or '\\' in parsed.path or '..' in parsed.path.split('/')):
        return None
    civic = re.fullmatch(r'/AgendaCenter/ViewFile/(Agenda|Minutes)/_([0-9]{2})([0-9]{2})([0-9]{4})-[0-9]{1,12}',
                         parsed.path, re.I)
    if civic:
        return civic.group(1).lower(), f'{civic.group(4)}-{civic.group(2)}-{civic.group(3)}'
    # Small towns often link agenda/minutes PDFs directly from their own site.
    # A keyword match only schedules screening; it creates no decision claim.
    context = (parsed.path + ' ' + label).lower()
    if parsed.path.lower().endswith(('.pdf', '.docx')) and re.search(r'agenda|minutes|meeting|packet', context):
        kind = 'minutes' if 'minute' in context else 'agenda'
        return kind, None
    return None


def discovery_targets(links, page):
    """Prioritize bounded same-host agenda and meeting navigation."""
    host = urlsplit(page).hostname
    ranked = []
    for ordinal, (href, label) in enumerate(links):
        try:
            target = urljoin(page, href)
            item = urlsplit(target)
            if (item.scheme != 'https' or item.hostname != host or item.port not in (None, 443)
                    or item.username or item.password or item.fragment or len(target) > 2000):
                continue
            context = label + ' ' + item.path
            if not re.search(r'agenda|meeting|council|commission|minutes|legislat', context, re.I):
                continue
            priority = 0 if re.search(r'agenda|minutes', context, re.I) else (
                1 if re.search(r'meeting', context, re.I) else 2 if re.search(r'legislat', context, re.I) else 3)
            ranked.append((priority, ordinal, target))
        except ValueError:
            continue
    ordered = list(dict.fromkeys(target for _, _, target in sorted(ranked)))
    # CivicPlus frequently renders navigation in JavaScript. Probe its
    # standard index, but let explicit agenda links take precedence.
    if not any('/agendacenter' in urlsplit(item).path.lower() for item in ordered):
        ordered.insert(min(2, len(ordered)), f'https://{host}/AgendaCenter')
    return ordered[:6]


def discover_one(ref, name, website, budget):
    observations = []
    result = {'jurisdiction_ref': ref, 'name': name, 'website_lead': website,
              'checked_at': utcnow(), 'state': 'failed', 'portals': [], 'documents': []}
    try:
        home = normalized_home(website)
        host = urlsplit(home).hostname
        alternate = host[4:] if host.startswith('www.') else 'www.' + host
        hosts = {host, alternate}
        policies = {}

        def policy_for(target):
            target_host = urlsplit(target).hostname
            if target_host not in policies:
                rules, rows = get_robots(target_host, hosts, budget)
                observations.extend(rows)
                policies[target_host] = rules
                # A robots redirect to the www alias identifies its canonical host.
                policies[urlsplit(rows[-1][0]['url']).hostname] = rules
                result['robots_status'] = rows[-1][0]['status']
            return allowed(policies[target_host], target)

        def page_for(target):
            if not policy_for(target):
                raise ValueError('robots disallows this path')
            rows = get(target, hosts, maximum=MAX_PAGE_BYTES, redirect_guard=policy_for, budget=budget)
            observations.extend(rows)
            meta, body = rows[-1]
            if meta['status'] != 200 or 'html' not in meta['content_type'].lower():
                raise ValueError(f"not an HTML page (HTTP {meta['status']})")
            return meta['url'], body

        final_home, body = page_for(home)
        candidates = [(final_home, body)]
        followed = set()
        for page, content in candidates:
            links = parse_links(content)
            for ordinal, (href, label) in enumerate(links):
                try:
                    target = urljoin(page, href)
                    url = urlsplit(target)
                    shape = candidate_kind(target, page, label)
                    if shape and len(result['documents']) < 500:
                        result['documents'].append({'url': target, 'index_url': page,
                                                    'index_sha256': sha(content), 'anchor_index': ordinal,
                                                    'kind': shape[0], 'meeting_date': shape[1],
                                                    'label': ' '.join(label.split())[:160]})
                    subdomain = url.hostname or ''
                    client = subdomain.removesuffix('.legistar.com')
                    if (url.scheme == 'https' and subdomain.endswith('.legistar.com')
                            and re.fullmatch(r'[a-z0-9-]{1,80}', client)
                            and '.' not in client and not url.username and not url.password
                            and url.port in (None, 443)):
                        result['portals'].append({'client': client, 'proof_url': page,
                                                  'proof_sha256': sha(content),
                                                  'anchor_index': ordinal, 'portal_url': target})
                except ValueError:
                    continue
            if page != final_home:
                continue
            for target in discovery_targets(links, page):
                if target in followed:
                    continue
                followed.add(target)
                try:
                    final_target, deeper = page_for(target)
                    candidates.append((final_target, deeper))
                except BudgetExhausted:
                    raise
                except RobotsDenied as error:
                    observations.extend(error.observations)
                except (OSError, ValueError, RuntimeError):
                    continue
        result['state'] = 'directory_site_checked'
        result['pages_checked'] = len(candidates)
    except BudgetExhausted:
        result['state'] = 'deferred'
    except RobotsDenied as error:
        observations.extend(error.observations)
        result['error'] = str(error)[:180]
    except (OSError, UnicodeError, ValueError, RuntimeError) as error:
        result['error'] = str(error)[:180]
    return result, observations


def record_discovery(db, result):
    """Keep the last usable discovery when a later request fails."""
    ref = result['jurisdiction_ref']
    previous = db.execute('SELECT result_json FROM discovery WHERE jurisdiction_ref=?', (ref,)).fetchone()
    with db:
        db.execute('INSERT OR REPLACE INTO discovery_attempts VALUES (?,?,?,?)',
                   (ref, result['checked_at'], result['state'], result.get('error')))
        if result['state'] == 'failed' and previous:
            try:
                if json.loads(previous[0]).get('state') == 'directory_site_checked':
                    return
            except (TypeError, json.JSONDecodeError):
                pass
        db.execute('INSERT OR REPLACE INTO discovery VALUES (?,?,?,?)',
                   (ref, result['website_lead'], result['checked_at'], json.dumps(result)))
        # Archived first-party proofs remain useful even if an index page
        # temporarily stops linking a portal or a deeper page fails to load.
        for portal in result['portals']:
            db.execute('INSERT OR REPLACE INTO portals VALUES (?,?,?)', (ref, portal['client'], json.dumps(portal)))
        for item in result['documents']:
            db.execute('''INSERT INTO document_candidates(jurisdiction_ref,url,candidate_json,state,updated_at)
                          VALUES (?,?,?,'pending',?) ON CONFLICT(jurisdiction_ref,url)
                          DO UPDATE SET candidate_json=excluded.candidate_json''',
                       (ref, item['url'], json.dumps(item), utcnow()))


def discover(data_dir, db, limit, workers, refresh, only_usable=False, only_failed=False):
    budget = DownloadBudget(300 * 1024 * 1024)
    county = bootstrap_counties(data_dir, db, budget)
    cities = json.loads(DIRECTORY.read_text())['municipalities']
    leads = [(x['jurisdiction_ref'], x['listing_name'], x['website_url']) for x in cities if x.get('website_url')]
    leads += [(ref, name, url) for ref, (name, url) in county.items()]
    if only_usable:
        usable = {ref for ref, raw in db.execute('SELECT jurisdiction_ref,result_json FROM discovery')
                  if json.loads(raw).get('state') == 'directory_site_checked'}
        leads = [item for item in leads if item[0] in usable]
    if only_failed:
        failed = {ref for ref, raw, attempt in db.execute('''SELECT d.jurisdiction_ref,d.result_json,a.state
                  FROM discovery d LEFT JOIN discovery_attempts a USING(jurisdiction_ref)''')
                  if (attempt or json.loads(raw).get('state')) == 'failed'}
        leads = [item for item in leads if item[0] in failed]
    if not refresh:
        cutoff = (datetime.now(timezone.utc) - timedelta(hours=24)).isoformat()
        already = {row[0] for row in db.execute('''SELECT d.jurisdiction_ref FROM discovery d
                   LEFT JOIN discovery_attempts a USING(jurisdiction_ref)
                   WHERE COALESCE(a.attempted_at,d.checked_at) >= ?''', (cutoff,))}
        leads = [item for item in leads if item[0] not in already]
    leads = leads[:limit]
    print(f'Checking {len(leads)} county/municipal website leads with {workers} workers', flush=True)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        jobs = [pool.submit(discover_one, *item, budget) for item in leads]
        for count, job in enumerate(as_completed(jobs), 1):
            result, observations = job.result()
            archive(data_dir, observations)
            if result['state'] == 'deferred':
                continue
            record_discovery(db, result)
            if count % 10 == 0 or count == len(jobs):
                print(f'Discovery {count}/{len(jobs)}; linked portals '
                      f"{db.execute('SELECT count(*) FROM portals').fetchone()[0]}, document candidates "
                      f"{db.execute('SELECT count(*) FROM document_candidates').fetchone()[0]}", flush=True)


def curated_clients():
    result = {}
    mappings = json.loads(CALENDARS.read_text())['calendar_geography']
    for mapping in mappings:
        data = tomllib.loads((ROOT / mapping['config_path']).read_text())
        source = next(x for x in data['sources'] if x['id'] == mapping['source_id'])
        hostname = urlsplit(source['url']).hostname
        if not hostname or not hostname.endswith('.legistar.com'):
            raise ValueError('configured calendar is not a Legistar portal')
        client = hostname.removesuffix('.legistar.com')
        if not re.fullmatch(r'[a-z0-9-]{1,80}', client):
            raise ValueError('invalid configured Legistar client')
        result[client] = mapping['jurisdiction_ref']
    return result


def observation_keys(data_dir):
    receipts = set()
    directory = data_dir / 'observations'
    if directory.exists() and not directory.is_symlink():
        for path in directory.iterdir():
            if not re.fullmatch(r'[0-9a-f]{64}\.json', path.name):
                continue
            try:
                raw = read_blob(path, 65_536)
                if path.stem != sha(raw):
                    continue
                receipt = json.loads(raw)
                if receipt_valid(receipt, receipt['url'], receipt['sha256']):
                    receipts.add((receipt['url'], receipt['sha256']))
            except (OSError, ValueError, TypeError, KeyError, json.JSONDecodeError):
                continue
    return receipts


def clients_from_store(db, data_dir):
    clients = curated_clients()
    groups = {}
    receipts = observation_keys(data_dir)
    for ref, client, raw in db.execute('SELECT jurisdiction_ref,client,proof_json FROM portals'):
        try:
            proof = json.loads(raw)
            if not re.fullmatch(r'[a-z0-9-]{1,80}', client) or proof['client'] != client:
                continue
            page, source_digest = proof['proof_url'], proof['proof_sha256']
            if (page, source_digest) not in receipts:
                continue
            body = source_blob(data_dir, source_digest)
            links = parse_links(body)
            ordinal = proof['anchor_index']
            directory = db.execute('SELECT source_url FROM discovery WHERE jurisdiction_ref=?', (ref,)).fetchone()
            if (not directory or type(ordinal) is not int or not 0 <= ordinal < len(links)
                    or urljoin(page, links[ordinal][0]) != proof['portal_url']
                    or urlsplit(proof['portal_url']).hostname != f'{client}.legistar.com'
                    or urlsplit(page).hostname.removeprefix('www.') != urlsplit(normalized_home(directory[0])).hostname.removeprefix('www.')):
                continue
            groups.setdefault(client, set()).add(ref)
        except (OSError, UnicodeError, ValueError, TypeError, KeyError, json.JSONDecodeError):
            continue
    for client, refs in groups.items():
        if client in clients:
            continue
        if len(refs) == 1:
            clients[client] = next(iter(refs))
    return clients


def query_url(client, page, page_size):
    if not re.fullmatch(r'[a-z0-9-]{1,80}', client) or not 0 <= page < 100:
        raise ValueError('invalid bounded client/page')
    filters = ' or '.join("substringof('" + term + "',MatterTitle)" for term in TERMS)
    params = {'$filter': filters, '$select': 'MatterId,MatterFile,MatterTitle,MatterIntroDate,MatterStatusName,MatterGuid',
              '$orderby': 'MatterId asc', '$top': str(page_size), '$skip': str(page * page_size)}
    return f'https://{API_HOST}/v1/{client}/matters?' + urlencode(params)


def matching_terms(title):
    lower = title.casefold()
    return [term for term in TERMS
            if re.search(r'(?<!\w)' + re.escape(term.casefold()) + r'(?!\w)', lower)
            and not (term == 'Flock' and ('flock of birds' in lower or 'safety of the flock' in lower))]


def scan(data_dir, db, max_clients, pages, page_size, only=None):
    clients = clients_from_store(db, data_dir)
    if only:
        if only not in clients:
            raise ValueError('client is not bound to a known Colorado jurisdiction')
        clients = {only: clients[only]}
    budget = DownloadBudget(100 * 1024 * 1024)
    rules, robots = get_robots(API_HOST, budget=budget)
    archive(data_dir, robots)
    print(f'Screening official matter titles in up to {min(max_clients, len(clients))} linked portals', flush=True)
    attempts = {client: (page, size, at) for client, page, size, at in
                db.execute('SELECT client,next_page,page_size,attempted_at FROM scan_progress')}
    ordered = sorted(clients.items(), key=lambda item: (attempts.get(item[0], (0, 0, ''))[2], item[0]))
    for client, ref in ordered[:max_clients]:
        count = 0
        saved_page, saved_size, _ = attempts.get(client, (0, page_size, ''))
        next_page = saved_page if saved_size == page_size and 0 <= saved_page < 100 else 0
        for _ in range(pages):
            page = next_page
            url = query_url(client, page, page_size)
            if not allowed(rules, url):
                print(f'{client}: source path blocked by robots policy', flush=True)
                break
            try:
                observations = get(url, {API_HOST}, maximum=MAX_PAGE_BYTES, timeout=30, redirects=0, budget=budget)
                archive(data_dir, observations)
                receipt, body = observations[-1]
                if receipt['status'] != 200 or 'json' not in receipt['content_type'].lower():
                    raise ValueError(f"HTTP {receipt['status']} or non-JSON response")
                items = json.loads(body)
                if not isinstance(items, list) or len(items) > page_size:
                    raise ValueError('unexpected matter page shape')
                next_page = page + 1 if len(items) == page_size and page < 99 else 0
                with db:
                    db.execute('INSERT OR REPLACE INTO scans VALUES (?,?,?)', (client, page, json.dumps(receipt)))
                    db.execute('INSERT OR IGNORE INTO scan_history VALUES (?,?,?,?)',
                               (client, page, receipt['sha256'], json.dumps(receipt)))
                    for index, item in enumerate(items):
                        if not isinstance(item, dict):
                            continue
                        matter = item.get('MatterId')
                        title = item.get('MatterTitle')
                        terms = matching_terms(title) if isinstance(title, str) else []
                        if (type(matter) is not int or not 0 < matter < 2**53 or not terms
                                or len(title) > 20_000):
                            continue
                        record_id = f'co-legistar-{client}-{matter}'
                        record = {'matter_id': matter, 'file': item.get('MatterFile'), 'title': title,
                                  'introduced': item.get('MatterIntroDate'), 'status': item.get('MatterStatusName'),
                                  'matches': terms, 'rule_version': TERM_VERSION}
                        db.execute('INSERT OR REPLACE INTO leads VALUES (?,?,?,?,?,?,?,?,?)',
                                   (record_id, ref, client, matter, page, index, receipt['sha256'],
                                    json.dumps(record), utcnow()))
                        count += 1
                    db.execute('INSERT OR REPLACE INTO scan_progress VALUES (?,?,?,?)',
                               (client, next_page, page_size, utcnow()))
                if next_page == 0:
                    break
            except (OSError, UnicodeError, ValueError, RuntimeError, json.JSONDecodeError) as error:
                print(f'{client}: page {page + 1} could not be screened ({str(error)[:120]})', flush=True)
                break
        with db:
            db.execute('INSERT OR REPLACE INTO scan_progress VALUES (?,?,?,?)',
                       (client, next_page, page_size, utcnow()))
        print(f'{client}: {count} potential matter references', flush=True)


def action_url(client, kind, first, second=None):
    if (not re.fullmatch(r'[a-z0-9-]{1,80}', client) or type(first) is not int or
            not 0 < first < 2**53 or (second is not None and (type(second) is not int or not 0 < second < 2**53))):
        raise ValueError('invalid action source identity')
    prefix = f'https://{API_HOST}/v1/{client}'
    if kind == 'histories' and second is None:
        return f'{prefix}/Matters/{first}/Histories'
    if kind == 'item' and second is not None:
        return f'{prefix}/Events/{first}/EventItems/{second}'
    if kind == 'votes' and second is None:
        return f'{prefix}/EventItems/{first}/Votes'
    raise ValueError('invalid action source type')


def capture_action_json(data_dir, url, rules, budget, expected):
    if not allowed(rules, url):
        raise ValueError('action source disallowed by robots policy')
    rows = get(url, {API_HOST}, maximum=MAX_PAGE_BYTES, timeout=30, redirects=0, budget=budget)
    archive(data_dir, rows)
    receipt, body = rows[-1]
    if receipt['status'] != 200 or 'json' not in receipt['content_type'].lower():
        raise ValueError('action source was unavailable or not JSON')
    result = json.loads(body)
    if not isinstance(result, expected) or (isinstance(result, list) and len(result) > 1000):
        raise ValueError('unexpected action source shape')
    return receipt, result


def collect_actions(data_dir, db, limit):
    clients = clients_from_store(db, data_dir)
    cutoff = (datetime.now(timezone.utc) - timedelta(hours=24)).isoformat()
    leads = db.execute('''SELECT l.client,l.matter_id,l.jurisdiction_ref FROM leads l
                          LEFT JOIN matter_action_checks c ON c.client=l.client AND c.matter_id=l.matter_id
                          WHERE c.checked_at IS NULL OR c.checked_at<?
                          ORDER BY COALESCE(c.checked_at,''),l.client,l.matter_id LIMIT ?''', (cutoff, limit)).fetchall()
    budget = DownloadBudget(100 * 1024 * 1024)
    rules, robots = get_robots(API_HOST, budget=budget)
    archive(data_dir, robots)
    print(f'Checking recorded actions for up to {len(leads)} source-matched matters', flush=True)
    for count, (client, matter, ref) in enumerate(leads, 1):
        if clients.get(client) != ref:
            with db:
                db.execute('''INSERT INTO matter_action_checks(client,matter_id,checked_at,status)
                              VALUES (?,?,?,'unbound_client') ON CONFLICT(client,matter_id)
                              DO UPDATE SET checked_at=excluded.checked_at,status=excluded.status''',
                           (client, matter, utcnow()))
            continue
        status = 'complete'
        latest_history = None
        try:
            history_receipt, histories = capture_action_json(
                data_dir, action_url(client, 'histories', matter), rules, budget, list)
            latest_history = history_receipt['sha256']
            candidates = [(i, row) for i, row in enumerate(histories) if isinstance(row, dict)
                          and type(row.get('MatterHistoryId')) is int and row['MatterHistoryId'] > 0
                          and type(row.get('MatterHistoryEventId')) is int and row['MatterHistoryEventId'] > 0
                          and row.get('MatterHistoryActionName')]
            candidates.sort(key=lambda pair: str(pair[1].get('MatterHistoryActionDate') or ''), reverse=True)
            if len(candidates) > 10:
                status = 'bounded_to_10_actions'
            for index, history in candidates[:10]:
                item, event = history['MatterHistoryId'], history['MatterHistoryEventId']
                try:
                    item_receipt, event_item = capture_action_json(
                        data_dir, action_url(client, 'item', event, item), rules, budget, dict)
                    if (event_item.get('EventItemId') != item or event_item.get('EventItemEventId') != event
                            or event_item.get('EventItemMatterId') != matter):
                        raise ValueError('event item does not identify this matter')
                    vote_receipt, votes = capture_action_json(
                        data_dir, action_url(client, 'votes', item), rules, budget, list)
                    target_ids = {row.get('VoteEventItemId') for row in votes if isinstance(row, dict)
                                  and type(row.get('VoteEventItemId')) is int}
                    block_receipt = None
                    if len(target_ids) == 1 and item not in target_ids and history.get('MatterHistoryConsent') == 1:
                        target = next(iter(target_ids))
                        block_receipt, _ = capture_action_json(
                            data_dir, action_url(client, 'item', event, target), rules, budget, dict)
                    captures = {'history': history_receipt, 'history_index': index, 'item': item_receipt,
                                'votes': vote_receipt, 'block': block_receipt}
                    with db:
                        previous = db.execute('SELECT captures_json FROM matter_actions WHERE client=? AND matter_id=? AND history_id=?',
                                              (client, matter, item)).fetchone()
                        if previous and json.loads(previous[0]) != captures:
                            db.execute('INSERT OR IGNORE INTO matter_action_history VALUES (?,?,?,?,?,?)',
                                       (client, matter, item, sha(previous[0].encode()), previous[0], utcnow()))
                        db.execute('INSERT OR REPLACE INTO matter_actions VALUES (?,?,?,?,?)',
                                   (client, matter, item, json.dumps(captures), utcnow()))
                except (OSError, UnicodeError, ValueError, RuntimeError, json.JSONDecodeError) as error:
                    status = 'partial'
                    print(f'{client} matter {matter} action {item}: {str(error)[:100]}', flush=True)
        except (OSError, UnicodeError, ValueError, RuntimeError, json.JSONDecodeError) as error:
            status = 'error'
            print(f'{client} matter {matter}: {str(error)[:100]}', flush=True)
        with db:
            db.execute('''INSERT INTO matter_action_checks(client,matter_id,checked_at,status,history_sha256)
                          VALUES (?,?,?,?,?) ON CONFLICT(client,matter_id) DO UPDATE SET
                          checked_at=excluded.checked_at,status=excluded.status,
                          history_sha256=COALESCE(excluded.history_sha256,matter_action_checks.history_sha256)''',
                       (client, matter, utcnow(), status, latest_history))
        if count % 5 == 0 or count == len(leads):
            print(f'Actions checked {count}/{len(leads)}; preserved {db.execute("SELECT count(*) FROM matter_actions").fetchone()[0]}', flush=True)


SUITE_HOST = 'www.agendasuite.org'
SUITE_LIST = f'https://{SUITE_HOST}/iip/elpaso/meeting/list'
SUITE_AUTHORITY = 'https://www.elpasoco.com/public-meetings/'
SUITE_REF = 'us-co-county-08041'


class VisibleText(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.depth = 0
        self.nodes = []

    def handle_starttag(self, tag, attrs):
        if tag.lower() in ('script', 'style', 'form', 'nav', 'footer', 'header'):
            self.depth += 1

    def handle_endtag(self, tag):
        if tag.lower() in ('script', 'style', 'form', 'nav', 'footer', 'header') and self.depth:
            self.depth -= 1

    def handle_data(self, data):
        if not self.depth and data.strip() and len(self.nodes) < 20000:
            self.nodes.append(' '.join(data.split())[:20000])


def visible_text(body):
    parser = VisibleText()
    parser.feed(body.decode('utf8', 'replace'))
    return parser.nodes


def collect_suite(data_dir, db, limit, pages):
    """Screen the county-linked recent AgendaSuite meeting pages, not arbitrary URLs."""
    budget = DownloadBudget(100 * 1024 * 1024)
    county_rules, observations = get_robots('www.elpasoco.com', budget=budget)
    archive(data_dir, observations)
    county, observations = html_page(SUITE_AUTHORITY, 'www.elpasoco.com', county_rules, budget)
    archive(data_dir, observations)
    county_links = parse_links(county)
    authority_anchor = next((i for i, (href, _) in enumerate(county_links)
                             if urljoin(SUITE_AUTHORITY, href) == SUITE_LIST), None)
    if authority_anchor is None:
        raise ValueError('county site no longer links the expected meeting calendar')
    rules, robots = get_robots(SUITE_HOST, budget=budget)
    archive(data_dir, robots)
    root, observations = html_page(SUITE_LIST, SUITE_HOST, rules, budget)
    archive(data_dir, observations)
    root_links = parse_links(root)
    for page in range(1, pages + 1):
        page_url = SUITE_LIST if page == 1 else f'{SUITE_LIST}/{page}'
        page_anchor = None if page == 1 else next((i for i, (href, _) in enumerate(root_links)
                                                  if urljoin(SUITE_LIST, href) == page_url), None)
        if page > 1 and page_anchor is None:
            continue
        if page == 1:
            body = root
        else:
            body, observations = html_page(page_url, SUITE_HOST, rules, budget)
            archive(data_dir, observations)
        for anchor, (href, _) in enumerate(parse_links(body)):
            url = urljoin(page_url, href)
            match = re.fullmatch(r'https://www\.agendasuite\.org/iip/elpaso/meeting/details/([1-9][0-9]{0,10})', url)
            if not match:
                continue
            meeting = int(match.group(1))
            candidate = {'url': url, 'authority_sha256': sha(county), 'authority_anchor': authority_anchor,
                         'root_sha256': sha(root), 'page_url': page_url, 'page_sha256': sha(body),
                         'page_anchor': page_anchor, 'meeting_anchor': anchor}
            with db:
                db.execute("INSERT OR IGNORE INTO suite_meetings(meeting_id,candidate_json,state,updated_at) VALUES (?,?,'pending',?)",
                           (meeting, json.dumps(candidate), utcnow()))
    week_ago = (datetime.now(timezone.utc) - timedelta(days=7)).isoformat()
    day_ago = (datetime.now(timezone.utc) - timedelta(days=1)).isoformat()
    rows = db.execute('''SELECT meeting_id,candidate_json,state,result_json,updated_at FROM suite_meetings
                         WHERE state='pending' OR (state='error' AND (last_attempt_at IS NULL OR last_attempt_at<=?))
                           OR (state IN ('matched','no_match') AND updated_at<=?
                             AND (last_attempt_at IS NULL OR last_attempt_at<=?))
                         ORDER BY CASE state WHEN 'pending' THEN 0 ELSE 1 END,
                           COALESCE(last_attempt_at,''),meeting_id DESC LIMIT ?''',
                      (day_ago, week_ago, day_ago, limit)).fetchall()
    print(f'Screening up to {len(rows)} county-linked AgendaSuite meeting pages', flush=True)
    for count, (meeting, raw, old_state, old_raw, updated) in enumerate(rows, 1):
        candidate = json.loads(raw)
        try:
            body, observations = html_page(candidate['url'], SUITE_HOST, rules, budget)
            archive(data_dir, observations)
            receipt = observations[-1][0]
            nodes = visible_text(body)
            finding = next(((i, matching_terms(node)) for i, node in enumerate(nodes) if matching_terms(node)), None)
            result = {'receipt': receipt, 'candidate': candidate, 'node': finding[0] if finding else None}
            new_state = 'matched' if finding else 'no_match'
            with db:
                if old_state == 'matched' and old_raw:
                    old = json.loads(old_raw)
                    if new_state != 'matched' or old['receipt']['sha256'] != receipt['sha256']:
                        db.execute('INSERT OR IGNORE INTO suite_history VALUES (?,?,?,?,?)',
                                   (meeting, old['receipt']['sha256'], json.dumps(old.get('candidate', candidate)), old_raw, updated))
                db.execute('''UPDATE suite_meetings SET state=?,result_json=?,updated_at=?,last_attempt_at=?,last_error=NULL
                              WHERE meeting_id=?''', (new_state, json.dumps(result), utcnow(), utcnow(), meeting))
        except (OSError, UnicodeError, ValueError, RuntimeError, json.JSONDecodeError) as error:
            with db:
                db.execute('UPDATE suite_meetings SET last_attempt_at=?,last_error=? WHERE meeting_id=?',
                           (utcnow(), str(error)[:180], meeting))
                if old_state == 'pending':
                    db.execute("UPDATE suite_meetings SET state='error',updated_at=? WHERE meeting_id=?", (utcnow(), meeting))
        if count % 10 == 0 or count == len(rows):
            matched = db.execute("SELECT count(*) FROM suite_meetings WHERE state='matched'").fetchone()[0]
            print(f'AgendaSuite checked {count}/{len(rows)}; matches {matched}', flush=True)


class DownloadBudget:
    def __init__(self, maximum):
        self.remaining = maximum
        self.lock = threading.Lock()

    def reserve(self, maximum=20 * 1024 * 1024):
        with self.lock:
            if self.remaining < 1_048_576:
                return 0
            amount = min(self.remaining, maximum)
            self.remaining -= amount
            return amount

    def release_unused(self, amount):
        with self.lock:
            self.remaining += amount


def cached_robots(host, cache):
    if host not in cache:
        rules, observations = get_robots(host)
        cache[host] = rules
        return rules, observations
    return cache[host], []


def extract_captured_pdf(data_dir, ref, candidate, binary, receipt, ocr):
    if not receipt_valid(receipt, candidate['url'], receipt.get('sha256')) or not source_blob(data_dir, receipt['sha256'], 20 * 1024 * 1024).startswith(b'%PDF-'):
        raise ValueError('archived PDF does not match its public capture receipt')
    cmd = [binary, '--data-dir', str(data_dir), '--rules', str(ROOT / 'rules/surveillance.yml'),
           'source', 'screen-research-pdf', '--digest', receipt['sha256'], '--source-url', candidate['url'],
           '--jurisdiction-ref', ref, '--retrieved-at', receipt['retrieved_at']]
    if ocr:
        cmd.append('--ocr')
    environment = {'PATH': os.environ.get('PNUL_EXTRACTOR_PATH', '/usr/bin:/bin'),
                   'LANG': 'C.UTF-8', 'TMPDIR': '/tmp'}
    if os.environ.get('PNUL_TESSDATA_DIR'):
        environment['PNUL_TESSDATA_DIR'] = os.environ['PNUL_TESSDATA_DIR']
    execution = subprocess.run(cmd, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               timeout=180 if ocr else 90, check=False, env=environment)
    if execution.returncode or len(execution.stdout) > 2_000_000:
        detail = execution.stderr.decode('utf8', 'replace').strip().splitlines()
        clean = ' '.join((detail[-1] if detail else 'extractor failed').split())[:160]
        raise ValueError('sandboxed PDF extraction or screening failed: ' + clean)
    result = json.loads(execution.stdout)
    if (result.get('state') not in ('matched', 'no_match')
            or result.get('document_sha256') != receipt['sha256']):
        raise ValueError('screening output does not match the archived PDF')
    if (result.get('extraction_status') == 'complete_with_ocr'
            and (result.get('ocr_page_set_validated') is not True or result.get('ocr_page_text_mapping_version') != 2)):
        raise ValueError('OCR worker has not validated per-page text mapping')
    result['receipt'] = receipt
    result['candidate'] = candidate
    return result


def checked_document_candidate(data_dir, url, candidate):
    index_url = candidate['index_url']
    links = parse_links(source_blob(data_dir, candidate['index_sha256']))
    anchor = candidate['anchor_index']
    if (candidate['url'] != url or type(anchor) is not int or not 0 <= anchor < len(links)
            or urljoin(index_url, links[anchor][0]) != url or not candidate_kind(url, index_url, links[anchor][1])):
        raise ValueError('document is not bound to its preserved source index')


def screen_one(data_dir, ref, candidate, binary, host_lock, budget, ocr, robots_cache):
    """Rebind the index anchor, then fetch and sandbox-extract one document."""
    observations = []
    try:
        url = candidate['url']
        checked_document_candidate(data_dir, url, candidate)
        parsed = urlsplit(url)
        host = parsed.hostname
        # Serialize calls for the same host, including its robots check.
        with host_lock:
            rules, robots = cached_robots(host, robots_cache)
            observations.extend(robots)
            if not allowed(rules, url):
                raise ValueError('robots disallows this document')
            reserved = budget.reserve()
            if not reserved:
                return {'state': 'deferred'}, observations
            rows = get(url, {host}, maximum=reserved, timeout=30, redirects=0)
            observations.extend(rows)
        receipt, pdf = rows[-1]
        budget.release_unused(reserved - len(pdf))
        if receipt['status'] != 200:
            raise ValueError(f"not a PDF (HTTP {receipt['status']})")
        archive(data_dir, observations)
        observations = []
        if pdf.startswith(b'%PDF-'):
            result = extract_captured_pdf(data_dir, ref, candidate, binary, receipt, ocr)
        elif pdf.startswith(b'PK\x03\x04'):
            result = extract_saved_docx(data_dir, ref, candidate, receipt)
        else:
            raise ValueError('not a PDF (HTTP 200)')
        return result, []
    except RobotsDenied as error:
        observations.extend(error.observations)
        return {'state': 'error', 'error': str(error)[:180]}, observations
    except (OSError, UnicodeError, ValueError, RuntimeError, subprocess.TimeoutExpired, KeyError) as error:
        return {'state': 'error', 'error': str(error)[:180]}, observations


def screen_documents(data_dir, db, binary, limit, per_place, workers, retry_errors=False, ocr=False, refresh_existing=False):
    if not Path(binary).is_absolute() or not Path(binary).is_file():
        raise ValueError('pnull binary must be an absolute path to an existing file')
    now = datetime.now(timezone.utc)
    day_ago = (now - timedelta(days=1)).isoformat()
    week_ago = (now - timedelta(days=7)).isoformat()
    if retry_errors:
        condition, parameters = "state='error' AND (last_attempt_at IS NULL OR last_attempt_at<=?)", (day_ago,)
    elif refresh_existing:
        condition, parameters = ("state IN ('matched','no_match') AND updated_at<=? AND (last_attempt_at IS NULL OR last_attempt_at<=?) "
                                 "AND NOT (COALESCE(json_extract(result_json,'$.extraction_status'),'')='complete_with_ocr' "
                                 "AND (COALESCE(json_extract(result_json,'$.ocr_page_set_validated'),0)!=1 "
                                 "OR COALESCE(json_extract(result_json,'$.ocr_page_text_mapping_version'),0)!=2))"), (week_ago, day_ago)
    else:
        condition, parameters = "state='pending'", ()
    rows = db.execute(f'''
      SELECT jurisdiction_ref,candidate_json,state FROM (
        SELECT jurisdiction_ref,candidate_json,url,state,last_attempt_at,
          ROW_NUMBER() OVER (PARTITION BY jurisdiction_ref ORDER BY
            COALESCE(last_attempt_at,''),
            CASE WHEN lower(url) LIKE '%/minutes/%' THEN 0 ELSE 1 END,
            COALESCE(json_extract(candidate_json,'$.meeting_date'),'') DESC, url DESC) AS turn
        FROM document_candidates WHERE {condition}
      ) WHERE turn <= ? ORDER BY turn,COALESCE(last_attempt_at,''),jurisdiction_ref LIMIT ?
    ''', (*parameters, per_place, limit)).fetchall()
    hosts = {urlsplit(json.loads(raw)['url']).hostname for _, raw, _ in rows}
    locks = {host: threading.Lock() for host in hosts}
    robots_cache = {}
    budget = DownloadBudget(200 * 1024 * 1024)
    print(f'Screening up to {len(rows)} official agenda/minutes documents across '
          f'{len({ref for ref, _, _ in rows})} jurisdictions', flush=True)
    started = time.monotonic()
    completed = errors = deferred = 0

    def progress(finished, total):
        print(f'Document jobs {finished}/{total}; completed screens {completed}; '
              f'errors {errors}; deferred {deferred}; elapsed {int(time.monotonic()-started)}s', flush=True)

    with ThreadPoolExecutor(max_workers=workers) as pool:
        jobs = {pool.submit(screen_one, data_dir, ref, json.loads(raw), binary,
                            locks[urlsplit(json.loads(raw)['url']).hostname], budget, ocr, robots_cache): (ref, json.loads(raw), state)
                for ref, raw, state in rows}
        for count, job in enumerate(as_completed(jobs), 1):
            ref, candidate, old_state = jobs[job]
            result, observations = job.result()
            if observations:
                archive(data_dir, observations)
            if result['state'] == 'deferred':
                deferred += 1
                if count == len(jobs):
                    progress(count, len(jobs))
                continue
            if result['state'] in ('matched', 'no_match'):
                completed += 1
            else:
                errors += 1
            with db:
                if old_state == 'matched' and result['state'] in ('matched', 'no_match'):
                    previous = db.execute('SELECT candidate_json,result_json,updated_at FROM document_candidates WHERE jurisdiction_ref=? AND url=?',
                                          (ref, candidate['url'])).fetchone()
                    prior_result = json.loads(previous[1])
                    if (result['state'] != 'matched'
                            or result.get('document_sha256') != prior_result.get('document_sha256')):
                        old_candidate = prior_result.get('candidate', json.loads(previous[0]))
                        db.execute('INSERT OR IGNORE INTO document_history VALUES (?,?,?,?,?,?,?)',
                                   (ref, candidate['url'], prior_result['document_sha256'], json.dumps(old_candidate), previous[1], previous[2], utcnow()))
                if result['state'] == 'error' and old_state in ('matched', 'no_match'):
                    db.execute('''UPDATE document_candidates SET attempts=attempts+1,
                                  last_attempt_at=?,last_error=? WHERE jurisdiction_ref=? AND url=?''',
                               (utcnow(), result['error'], ref, candidate['url']))
                else:
                    db.execute('''UPDATE document_candidates SET state=?,attempts=CASE WHEN ?='error' THEN attempts+1 ELSE 0 END,
                                  result_json=?,updated_at=?,last_attempt_at=?,last_error=? WHERE jurisdiction_ref=? AND url=?''',
                               (result['state'], result['state'], json.dumps(result), utcnow(), utcnow(),
                                result.get('error'), ref, candidate['url']))
            if count % 10 == 0 or count == len(jobs):
                progress(count, len(jobs))


def civic_year_selectors(body, current_year):
    """Read bounded year/category values without evaluating portal JavaScript."""
    pattern = re.compile(r'''javascript:changeYear\(\s*(\d{4})\s*,\s*(\d{1,6})\s*,\s*['"]a\d+['"]\s*\);?''', re.I)
    found = {}
    for ordinal, (href, _label) in enumerate(parse_links(body)):
        match = pattern.fullmatch(href.strip()) if isinstance(href, str) else None
        if not match:
            continue
        year, category = map(int, match.groups())
        if 2000 <= year < current_year and 0 < category <= 999999:
            found.setdefault((year, category), ordinal)
    return [(year, category, ordinal) for (year, category), ordinal in found.items()]


def civic_history(data_dir, db, limit, per_place, workers, only=None, pending_cap=1000):
    """Discover older CivicPlus records from captured, official year selectors."""
    pending = db.execute("SELECT count(*) FROM document_candidates WHERE state='pending'").fetchone()[0]
    if pending >= pending_cap:
        print(f'Historical discovery deferred: document queue has {pending} pending', flush=True)
        return
    receipts = observation_keys(data_dir)
    attempts = {(ref, host, category, year): (at, state) for ref, host, category, year, at, state in
                db.execute('SELECT jurisdiction_ref,host,category,year,attempted_at,state FROM civic_history_checks')}
    now = datetime.now(timezone.utc)
    choices = {}
    for ref, directory, raw in db.execute('SELECT jurisdiction_ref,source_url,result_json FROM discovery'):
        if only and ref != only:
            continue
        try:
            entry = json.loads(raw)
            if entry.get('state') != 'directory_site_checked':
                continue
            directory_host = urlsplit(normalized_home(directory)).hostname.removeprefix('www.')
            indexes = {}
            for document in entry.get('documents', []):
                page, digest = document['index_url'], document['index_sha256']
                parsed = urlsplit(page)
                if (parsed.scheme != 'https' or not parsed.hostname or parsed.username or parsed.password
                        or parsed.port not in (None, 443) or parsed.query or parsed.fragment
                        or parsed.path.rstrip('/').lower() != '/agendacenter'
                        or parsed.hostname.removeprefix('www.') != directory_host or (page, digest) not in receipts):
                    continue
                indexes[(page, digest)] = parsed.hostname
            for (page, digest), host in indexes.items():
                for year, category, ordinal in civic_year_selectors(source_blob(data_dir, digest), now.year):
                    key = (ref, host, category, year)
                    at, state = attempts.get(key, ('', ''))
                    cutoff = now - (timedelta(days=1) if state == 'error' else timedelta(days=7))
                    if at and datetime.fromisoformat(at) > cutoff:
                        continue
                    choices.setdefault(key, {'jurisdiction_ref': ref, 'host': host, 'year': year, 'category': category,
                                             'selector_url': page, 'selector_sha256': digest, 'selector_anchor': ordinal,
                                             'last_attempt': at})
        except (OSError, UnicodeError, ValueError, TypeError, KeyError, json.JSONDecodeError):
            continue
    by_place = {}
    for item in choices.values():
        by_place.setdefault(item['jurisdiction_ref'], []).append(item)
    for items in by_place.values():
        items.sort(key=lambda item: (item['last_attempt'], -item['year'], item['category'], item['host']))
    selected = []
    latest_by_place = dict(db.execute('SELECT jurisdiction_ref,max(attempted_at) FROM civic_history_checks GROUP BY jurisdiction_ref'))
    place_order = sorted(by_place, key=lambda ref: (latest_by_place.get(ref, ''), ref))
    for turn in range(per_place):
        for ref in place_order:
            if turn < len(by_place[ref]) and len(selected) < limit:
                selected.append(by_place[ref][turn])
    print(f'Checking {len(selected)} older CivicPlus indexes across {len({x["jurisdiction_ref"] for x in selected})} jurisdictions', flush=True)
    budget = DownloadBudget(150 * 1024 * 1024)
    locks = {item['host']: threading.Lock() for item in selected}
    robots_cache = {}
    checked = errors = deferred = 0

    def progress(finished, total):
        pending = db.execute("SELECT count(*) FROM document_candidates WHERE state='pending'").fetchone()[0]
        print(f'Historical index jobs {finished}/{total}; checked {checked}; errors {errors}; '
              f'deferred {deferred}; pending documents {pending}', flush=True)

    def fetch(item):
        observations = []
        try:
            host = item['host']
            query = urlencode({'year': item['year'], 'catID': item['category'], 'startDate': '',
                               'endDate': '', 'term': '', 'prevVersionScreen': 'false'})
            url = f'https://{host}/AgendaCenter/UpdateCategoryList?{query}'
            with locks[host]:
                if host not in robots_cache:
                    rules, robots = get_robots(host, budget=budget)
                    robots_cache[host] = rules
                    observations.extend(robots)
                rules = robots_cache[host]
                body, rows = html_page(url, host, rules, budget)
                observations.extend(rows)
            meta = rows[-1][0]
            documents = []
            for ordinal, (href, label) in enumerate(parse_links(body)):
                try:
                    target = urljoin(url, href)
                    shape = candidate_kind(target, url, label)
                    if shape and shape[1] and not shape[1].startswith(f"{item['year']}-"):
                        continue
                    if shape and len(documents) < 500:
                        documents.append({'url': target, 'index_url': url, 'index_sha256': sha(body),
                                          'anchor_index': ordinal, 'kind': shape[0], 'meeting_date': shape[1],
                                          'label': ' '.join(label.split())[:160]})
                except (TypeError, ValueError):
                    continue
            return {'state': 'ok', 'receipt': meta, 'documents': documents}, observations
        except BudgetExhausted:
            return {'state': 'deferred'}, observations
        except RobotsDenied as error:
            observations.extend(error.observations)
            return {'state': 'error', 'error': str(error)[:180]}, observations
        except (OSError, UnicodeError, ValueError, RuntimeError) as error:
            return {'state': 'error', 'error': str(error)[:180]}, observations

    with ThreadPoolExecutor(max_workers=workers) as pool:
        jobs = {pool.submit(fetch, item): item for item in selected}
        for count, job in enumerate(as_completed(jobs), 1):
            item = jobs[job]
            result, observations = job.result()
            if observations:
                archive(data_dir, observations)
            if result['state'] == 'deferred':
                deferred += 1
                if count == len(jobs):
                    progress(count, len(jobs))
                continue
            if result['state'] == 'ok':
                checked += 1
            else:
                errors += 1
            ref = item['jurisdiction_ref']
            selector = {key: item[key] for key in ('selector_url', 'selector_sha256', 'selector_anchor')}
            with db:
                db.execute('INSERT OR REPLACE INTO civic_history_checks VALUES (?,?,?,?,?,?,?,?,?)',
                           (ref, item['host'], item['category'], item['year'], json.dumps(selector), utcnow(),
                            result['state'], result.get('error'), json.dumps(result['receipt']) if result.get('receipt') else None))
                for document in result.get('documents', []):
                    db.execute('''INSERT OR IGNORE INTO document_candidates(jurisdiction_ref,url,candidate_json,state,updated_at)
                                  VALUES (?,?,?,'pending',?)''', (ref, document['url'], json.dumps(document), utcnow()))
            if count % 10 == 0 or count == len(jobs):
                progress(count, len(jobs))


def saved_receipts(data_dir, wanted):
    latest = {}
    available = set()
    if not wanted:
        return latest, available
    directory = data_dir / 'observations'
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError('unsafe observation directory')
    for path in directory.iterdir():
        if not re.fullmatch(r'[0-9a-f]{64}\.json', path.name):
            continue
        try:
            raw = read_blob(path, 65_536)
            if sha(raw) != path.stem:
                continue
            receipt = json.loads(raw)
            url = receipt.get('url')
            if (url in wanted and receipt_valid(receipt, url, receipt.get('sha256'))
                    and receipt.get('sha256')):
                available.add((url, receipt['sha256'], receipt['retrieved_at']))
                if receipt['retrieved_at'] > latest.get(url, {}).get('retrieved_at', ''):
                    latest[url] = receipt
        except (OSError, ValueError, TypeError, KeyError, json.JSONDecodeError):
            continue
    return latest, available


def ocr_saved_documents(data_dir, db, binary, limit, workers, retry_unavailable=False, revalidate=False,
                        retry_previous_limit=False):
    """Retry already archived scanned PDFs without making another network request."""
    if not Path(binary).is_absolute() or not Path(binary).is_file():
        raise ValueError('pnull binary must be an absolute path to an existing file')
    cutoff = (datetime.now(timezone.utc) - timedelta(days=7)).isoformat()
    condition = ("d.state IN ('matched','no_match') AND json_extract(d.result_json,'$.extraction_status')='complete_with_ocr' "
                 "AND (COALESCE(json_extract(d.result_json,'$.ocr_page_set_validated'),0)!=1 "
                 "OR COALESCE(json_extract(d.result_json,'$.ocr_page_text_mapping_version'),0)!=2)") if revalidate else (
                 "d.state='error' AND d.result_json LIKE '%scanned_pdf_ocr_disabled%' "
                 "AND (o.attempted_at IS NULL OR o.attempted_at<=? "
                 "OR (? AND (lower(o.status) LIKE '%could not initialize tesseract%' OR lower(o.status) LIKE '%sandbox_output%')) "
                 "OR (? AND o.status LIKE '%(ocr_page_limit): OCR is limited to 5 pages%'))" )
    parameters = (limit,) if revalidate else (cutoff, retry_unavailable, retry_previous_limit, limit)
    rows = db.execute(f'''SELECT jurisdiction_ref,url,candidate_json,old_result,old_state FROM (
        SELECT d.jurisdiction_ref,d.url,d.candidate_json,d.state AS old_state,
          d.result_json AS old_result,o.attempted_at AS last_attempt,
          ROW_NUMBER() OVER (PARTITION BY d.jurisdiction_ref ORDER BY COALESCE(o.attempted_at,''),d.url DESC) AS turn
        FROM document_candidates d LEFT JOIN ocr_attempts o ON o.jurisdiction_ref=d.jurisdiction_ref AND o.url=d.url
        WHERE {condition}
      ) ORDER BY turn,COALESCE(last_attempt,''),jurisdiction_ref LIMIT ?''', parameters).fetchall()
    latest, available = saved_receipts(data_dir, {url for _, url, _, _, _ in rows})

    def retry(ref, url, raw, old_result):
        try:
            old = json.loads(old_result) if revalidate else {}
            candidate = old.get('candidate', json.loads(raw))
            checked_document_candidate(data_dir, url, candidate)
            captured = old.get('receipt') if revalidate else latest.get(url)
            if (not captured or not receipt_valid(captured, url, captured.get('sha256'))
                    or (url, captured['sha256'], captured['retrieved_at']) not in available):
                raise ValueError('saved PDF capture receipt not found')
            return extract_captured_pdf(data_dir, ref, candidate, binary, captured, True), None
        except (OSError, UnicodeError, ValueError, RuntimeError, subprocess.TimeoutExpired, KeyError, json.JSONDecodeError) as error:
            return None, str(error)[:180]

    print(f'OCR retry of up to {len(rows)} already archived scanned PDFs', flush=True)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        jobs = {pool.submit(retry, ref, url, raw, old_result): (ref, url, old_result, old_state)
                for ref, url, raw, old_result, old_state in rows}
        for count, job in enumerate(as_completed(jobs), 1):
            ref, url, old_result, old_state = jobs[job]
            result, error = job.result()
            with db:
                if revalidate:
                    db.execute('INSERT INTO ocr_history VALUES (?,?,?,?,?)',
                               (ref, url, utcnow(), old_result, 'ok' if result else error))
                db.execute('INSERT OR REPLACE INTO ocr_attempts VALUES (?,?,?,?)',
                           (ref, url, utcnow(), 'ok' if result else error))
                if result:
                    db.execute('''UPDATE document_candidates SET state=?,result_json=?,updated_at=?,last_error=NULL
                                  WHERE jurisdiction_ref=? AND url=? AND state=?''',
                               (result['state'], json.dumps(result), utcnow(), ref, url, old_state))
                elif revalidate:
                    failed = {'state': 'error', 'error': 'scanned_pdf_ocr_disabled; OCR page recheck failed: ' + error}
                    db.execute('''UPDATE document_candidates SET state='error',result_json=?,updated_at=?,last_error=?
                                  WHERE jurisdiction_ref=? AND url=? AND state=?''',
                               (json.dumps(failed), utcnow(), error, ref, url, old_state))
            if count % 10 == 0 or count == len(jobs):
                print(f'Saved OCR attempts {count}/{len(jobs)}', flush=True)


def extract_saved_docx(data_dir, ref, candidate, receipt):
    url, digest = candidate['url'], receipt['sha256']
    checked_document_candidate(data_dir, url, candidate)
    if not receipt_valid(receipt, url, digest) or not source_blob(data_dir, digest, 20 * 1024 * 1024).startswith(b'PK\x03\x04'):
        raise ValueError('archived response is not a DOCX candidate')
    path = os.environ.get('PNUL_EXTRACTOR_PATH', '/usr/bin:/bin')
    bwrap, prlimit = shutil.which('bwrap', path=path), shutil.which('prlimit', path=path)
    script = ROOT / 'scripts/extract-docx.py'
    read_blob(script, 100_000)
    if not bwrap or not prlimit or not Path('/usr/bin/python3').is_file():
        raise ValueError('DOCX sandbox tools are unavailable')
    command = [prlimit, '--as=805306368', '--cpu=30', '--fsize=10485760', '--', bwrap]
    for runtime in ('/usr', '/bin', '/lib', '/lib64', '/nix/store'):
        if Path(runtime).exists():
            command.extend(['--ro-bind', runtime, runtime])
    command.extend(['--ro-bind', str(script), '/extract.py', '--ro-bind', str(data_dir / 'content' / digest), '/input.docx',
                    '--dev', '/dev', '--proc', '/proc', '--tmpfs', '/tmp', '--unshare-net', '--unshare-pid',
                    '--unshare-ipc', '--unshare-uts', '--new-session', '--clearenv', '--setenv', 'LANG', 'C.UTF-8',
                    '--', '/usr/bin/python3', '-I', '-S', '/extract.py', '/input.docx'])
    execution = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=45, check=False,
                               env={'PATH': path, 'LANG': 'C.UTF-8', 'TMPDIR': '/tmp'})
    if execution.returncode or not execution.stdout or len(execution.stdout) > 5 * 1024 * 1024:
        raise ValueError('sandboxed DOCX extraction failed or exceeded its bounds')
    text = execution.stdout.decode('utf8')
    text_digest = sha(execution.stdout)
    extracted = data_dir / 'extracted'
    extracted.mkdir(mode=0o700, exist_ok=True)
    if extracted.is_symlink() or not extracted.is_dir():
        raise ValueError('unsafe extracted text directory')
    target = extracted / text_digest
    try:
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    except FileExistsError:
        if read_blob(target, 5 * 1024 * 1024) != execution.stdout:
            raise ValueError('preserved DOCX extraction differs')
    else:
        with os.fdopen(fd, 'wb') as handle:
            handle.write(execution.stdout)
            handle.flush()
            os.fsync(handle.fileno())
    evidence = 'docx:' + digest
    finding = None
    ambiguous = None
    for number, line in enumerate(text.splitlines(), 1):
        labels = labels_at_quote(RULE_LABELS, line)
        if labels:
            rule_ids = [rule for rule in RULE_LABELS if labels_at_quote([rule], line)]
            current = {'evidence_id': evidence, 'matched_rule_ids': rule_ids,
                       'citations': [{'evidence_id': evidence, 'locator': {'kind': 'line', 'start': number}, 'quote': line}]}
            if labels == ['Flock (identity not established)']:
                ambiguous = ambiguous or current
            else:
                finding = current
                break
    finding = finding or ambiguous
    return {'state': 'matched' if finding else 'no_match', 'document_format': 'docx', 'document_sha256': digest,
            'extracted_sha256': text_digest, 'evidence_id': evidence, 'finding': finding, 'extraction_status': 'complete',
            'docx_deleted_text_excluded': True,
            'rules_digest': sha(read_blob(ROOT / 'rules/surveillance.yml', 1_000_000)), 'rule_engine': 'docx-inline-v1',
            'receipt': receipt, 'candidate': candidate}


def docx_saved_documents(data_dir, db, limit, workers, revalidate=False):
    cutoff = (datetime.now(timezone.utc) - timedelta(days=7)).isoformat()
    condition = ("d.state IN ('matched','no_match') AND json_extract(d.result_json,'$.document_format')='docx' "
                 "AND COALESCE(json_extract(d.result_json,'$.docx_deleted_text_excluded'),0)!=1") if revalidate else (
                 "d.state='error' AND d.result_json LIKE '%not a PDF (HTTP 200)%' "
                 "AND (a.attempted_at IS NULL OR a.attempted_at<=?)")
    parameters = (limit,) if revalidate else (cutoff, limit)
    rows = db.execute(f'''SELECT jurisdiction_ref,url,candidate_json,old_result,old_state FROM (
        SELECT d.jurisdiction_ref,d.url,d.candidate_json,d.result_json AS old_result,d.state AS old_state,
          a.attempted_at AS last_attempt,
          ROW_NUMBER() OVER (PARTITION BY d.jurisdiction_ref ORDER BY COALESCE(a.attempted_at,''),d.url DESC) AS turn
        FROM document_candidates d LEFT JOIN docx_attempts a ON a.jurisdiction_ref=d.jurisdiction_ref AND a.url=d.url
        WHERE {condition}
      ) ORDER BY turn,COALESCE(last_attempt,''),jurisdiction_ref LIMIT ?''', parameters).fetchall()
    latest, available = saved_receipts(data_dir, {url for _, url, _, _, _ in rows})

    def attempt(ref, url, raw, old_raw):
        try:
            old = json.loads(old_raw) if revalidate else {}
            candidate = old.get('candidate', json.loads(raw))
            captured = old.get('receipt') if revalidate else latest.get(url)
            if (not captured or not receipt_valid(captured, url, captured.get('sha256'))
                    or (url, captured['sha256'], captured['retrieved_at']) not in available):
                raise ValueError('saved DOCX capture receipt not found')
            return extract_saved_docx(data_dir, ref, candidate, captured), None
        except (OSError, UnicodeError, ValueError, RuntimeError, subprocess.TimeoutExpired, KeyError, json.JSONDecodeError) as error:
            return None, str(error)[:180]

    print(f'Screening up to {len(rows)} archived Word-document candidates', flush=True)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        jobs = {pool.submit(attempt, ref, url, raw, old_result): (ref, url, old_result, old_state)
                for ref, url, raw, old_result, old_state in rows}
        for count, job in enumerate(as_completed(jobs), 1):
            ref, url, old_result, old_state = jobs[job]
            result, error = job.result()
            with db:
                if revalidate:
                    db.execute('INSERT INTO docx_history VALUES (?,?,?,?,?)',
                               (ref, url, utcnow(), old_result, 'ok' if result else error))
                db.execute('INSERT OR REPLACE INTO docx_attempts VALUES (?,?,?,?)', (ref, url, utcnow(), 'ok' if result else error))
                if result:
                    db.execute('''UPDATE document_candidates SET state=?,result_json=?,updated_at=?,last_error=NULL
                                  WHERE jurisdiction_ref=? AND url=? AND state=?''',
                               (result['state'], json.dumps(result), utcnow(), ref, url, old_state))
                elif revalidate:
                    failed = {'state': 'error', 'error': 'not a PDF (HTTP 200); DOCX recheck failed: ' + error}
                    db.execute('''UPDATE document_candidates SET state='error',result_json=?,updated_at=?,last_error=?
                                  WHERE jurisdiction_ref=? AND url=? AND state=?''',
                               (json.dumps(failed), utcnow(), error, ref, url, old_state))
            if count % 10 == 0 or count == len(jobs):
                print(f'Word-document attempts {count}/{len(jobs)}', flush=True)


def read_blob(path, maximum):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, 'rb') as handle:
        meta = os.fstat(handle.fileno())
        if not __import__('stat').S_ISREG(meta.st_mode) or meta.st_size > maximum:
            raise ValueError('preserved content is not a bounded regular file')
        data = handle.read(maximum + 1)
        if len(data) != meta.st_size:
            raise ValueError('preserved content changed during read')
        return data


def source_blob(data_dir, digest, maximum=MAX_PAGE_BYTES):
    if not re.fullmatch(r'[0-9a-f]{64}', digest):
        raise ValueError('invalid source digest')
    data = read_blob(data_dir / 'content' / digest, maximum)
    if sha(data) != digest:
        raise ValueError('source bytes differ from preserved digest')
    return data


def receipt_valid(receipt, url, digest):
    try:
        addresses = receipt['resolved_ips']
        return (receipt['url'] == url and receipt['status'] == 200 and receipt['sha256'] == digest
                and isinstance(addresses, list) and bool(addresses)
                and all(public_ip(address) for address in addresses)
                and receipt['connected_ip'] in addresses
                and bool(datetime.fromisoformat(receipt['retrieved_at'])))
    except (ValueError, TypeError, KeyError):
        return False


def public_text(value, limit=400):
    if not isinstance(value, str):
        return ''
    value = re.sub(r'<[^>]{0,500}>', ' ', value)
    value = ''.join(' ' if char.isspace() else char for char in value
                    if (char.isprintable() or char.isspace()) and char not in '\u202a\u202b\u202c\u202d\u202e\u2066\u2067\u2068\u2069')
    value = re.sub(r'\b[\w.+%-]+@[\w.-]+\.[A-Za-z]{2,}\b', '[email omitted]', value)
    value = re.sub(r'(?<!\d)(?:\+1[ .-]?)?\(?\d{3}\)?[ .-]\d{3}[ .-]\d{4}(?!\d)', '[phone omitted]', value)
    value = re.sub(r'\b\d{1,6}\s+(?:[A-Za-z0-9.\'-]+\s+){1,5}(?:Street|St\.?|Avenue|Ave\.?|Road|Rd\.?|Drive|Dr\.?|Lane|Ln\.?|Boulevard|Blvd\.?)\b',
                   '[street address omitted]', value, flags=re.I)
    return ' '.join(value.split())[:limit]


def reference_title(value, labels, private_context=False):
    if private_context or re.search(r'\b(?:Mr|Mrs|Ms|v|vs)\.|\b(?:resident|citizen|complainant|victim|juvenile|applicant|petitioner)\b|^\s*\d+\.\s+[A-Z][a-z]+\s+[A-Z][a-z]+\s*[-–—]', value, re.I):
        return 'Possible ' + ', '.join(labels) + ' reference in a public record.'
    return public_text(value)


def public_comment_context(lines, number):
    """Avoid repeating speaker names from a nearby public-comment section."""
    for line in reversed(lines[max(0, number - 16):number]):
        heading = re.sub(r'^\s*(?:\d+[.)]|[A-Za-z][.)])\s*', '', line).strip()
        if len(heading) > 120:
            continue
        if re.match(r'^(?:public|citizen|resident|community|audience)(?:s|\u2019s|\'s)?\s+(?:[a-z]+\s+){0,3}comments?\b', heading, re.I):
            return True
        if re.match(r'^(?:approval of|adjournment|new business|old business|consent agenda|ordinances|resolutions|staff reports)\b', heading, re.I):
            return False
    return False


def jurisdiction_name(item):
    return re.sub(r'\s+(?:city|town)$', '', item['name'], flags=re.I)


RULE_LABELS = {
    'technology.alpr': 'License plate readers', 'vendor.flock-safety': 'Flock Safety',
    'vendor.vigilant-solutions': 'Vigilant Solutions', 'vendor.motorola-solutions': 'Motorola Solutions',
    'vendor.palantir': 'Palantir', 'vendor.axon': 'Axon', 'vendor.clearview-ai': 'Clearview AI',
    'vendor.cellebrite': 'Cellebrite', 'vendor.soundthinking-shotspotter': 'ShotSpotter / SoundThinking',
    'technology.facial-recognition': 'Facial recognition', 'technology.biometric-identification': 'Biometric identification',
    'technology.real-time-crime-center': 'Real-time crime centers', 'technology.predictive-policing': 'Predictive policing',
    'technology.social-media-monitoring': 'Social-media monitoring', 'technology.cell-site-simulator': 'Cell-site simulators',
    'practice.geofence-warrant': 'Geofence warrants', 'technology.drone-surveillance': 'Drone surveillance',
    'industry.data-broker': 'Data brokers', 'technology.investigative-platform': 'Investigative platforms',
}


@lru_cache(maxsize=1)
def public_rule_terms():
    """Read the repository's bounded inline term arrays without a YAML dependency."""
    rules = {}
    current = None
    for line in read_blob(ROOT / 'rules/surveillance.yml', 1_000_000).decode('utf8').splitlines():
        match = re.fullmatch(r'  - id: ([a-z0-9.-]+)', line)
        if match:
            current = match.group(1)
            if current in rules:
                raise ValueError('duplicate research rule')
            rules[current] = {}
        elif current:
            match = re.fullmatch(r'    (terms|false_positive_phrases): (\[.*\])', line)
            if match:
                values = json.loads(match.group(2))
                if not isinstance(values, list) or any(not isinstance(value, str) or len(value) > 200 for value in values):
                    raise ValueError('invalid research rule terms')
                rules[current][match.group(1)] = values
    return rules


def labels_at_quote(rule_ids, quote):
    rules = public_rule_terms()
    labels = set()
    for rule_id in rule_ids:
        rule = rules.get(rule_id, {})
        if (rule_id in RULE_LABELS and 'terms' in rule and 'false_positive_phrases' in rule
                and not any(phrase.casefold() in quote.casefold() for phrase in rule['false_positive_phrases'])
                and any(re.search(r'(?<!\w)' + re.escape(term) + r'(?!\w)', quote, re.I) for term in rule['terms'])):
            if rule_id == 'vendor.soundthinking-shotspotter':
                labels.update(term for term in rule['terms'] if re.search(r'(?<!\w)' + re.escape(term) + r'(?!\w)', quote, re.I))
            elif rule_id == 'vendor.flock-safety':
                explicit = re.search(r'\bflock\s+(?:safety|group|vehicle.intelligence)\b', quote, re.I)
                technology = re.search(r'\b(?:cameras?|alpr|lpr|licen[cs]e[ -]plate|plate[ -]reader|surveillance)\b', quote, re.I)
                labels.add('Flock Safety' if explicit or technology else 'Flock (identity not established)')
            else:
                labels.add(RULE_LABELS[rule_id])
    return sorted(labels)


def non_surveillance_flock_context(lines, number, labels):
    if labels != ['Flock (identity not established)']:
        return False
    context = ' '.join(lines[max(0, number - 5):min(len(lines), number + 2)])
    art = re.search(r'\b(?:artist|artwork|sculpture|public art|roundabout submissions)\b', context, re.I)
    technology = re.search(r'\b(?:flock\s+(?:safety|group|cameras?)|alpr|lpr|licen[cs]e[ -]plate|plate[ -]reader|surveillance)\b|\bflock\b.{0,100}\bcameras?\b', context, re.I)
    return bool(art and not technology)


def officials_near_reference(lines, number):
    """Conservative name context; this is not an individual vote parser."""
    heading = ' '.join(lines[:12]).upper()
    if not any(term in heading for term in ('BOARD OF COUNTY COMMISSIONERS', 'CITY COUNCIL', 'BOARD OF TRUSTEES')):
        return []
    names = []
    pattern = re.compile(r"^([A-Z][A-Za-z.'’ -]{2,90}),\s*(Chairperson|Vice-Chairperson|Commissioner|Trustee|Mayor|Councilmember)\s*$", re.I)
    for index, line in enumerate(lines[:40]):
        if 'absent' in line.lower():
            continue
        match = pattern.fullmatch(line.strip())
        if not match:
            continue
        fullname, role = match.groups()
        words = fullname.split()
        if len(words) < 2 or any(len(word) > 40 for word in words):
            continue
        names.append((fullname, role, index + 1))
    found = []
    nearby = range(max(0, number - 6), min(len(lines), number + 16))
    for fullname, role, line in names:
        parts = fullname.split()
        alternatives = [' '.join(parts[-2:]), parts[-1]] if len(parts) > 2 else [parts[-1]]
        for alias in alternatives:
            if len(alias) < 4 or sum(other[0].casefold().endswith(alias.casefold()) for other in names) != 1:
                continue
            mention = re.compile(r'\b(?:Commissioner|Trustee|Mayor|Council(?:member|or))\s+' + re.escape(alias) + r'\b', re.I)
            match_index = next((i for i in nearby if mention.search(lines[i])), None)
            if match_index is not None:
                found.append({'name': fullname, 'role_in_source': role, 'name_locator': f'extracted line {line}',
                              'context_locator': f'extracted line {match_index + 1}', 'individual_vote': 'not_established'})
                break
    return found[:15]


def portrait_name_matches(fullname, caption, alias_text=''):
    match = re.fullmatch(r'(?:Profile picture of|Portrait of|Headshot of)\s+(.+)', caption, re.I)
    if not match:
        return False
    pictured = match.group(1).strip()
    normalize = lambda value: re.sub(r'\s+[A-Za-z]\.\s+', ' ', value.strip()).casefold()
    if normalize(pictured) == normalize(fullname):
        return True
    full, short = fullname.split(), pictured.split()
    if len(full) != 2 or len(short) != 2 or full[-1].casefold() != short[-1].casefold():
        return False
    # Nicknames are accepted only when a second official page explicitly
    # spells out the same first name, nickname, and surname together.
    pattern = r'\b' + re.escape(full[0]) + r'\s*\(\s*' + re.escape(short[0]) + r'\s*\)\s*' + re.escape(full[-1]) + r'\b'
    return bool(re.search(pattern, alias_text, re.I))


def collect_douglas_profiles(data_dir, db, ref, people, rules, budget):
    board_url = 'https://www.douglasco.gov/board-county-commissioners/'
    board, observations = html_page(board_url, 'www.douglasco.gov', rules, budget)
    archive(data_dir, observations)
    links = parse_links(board)
    stored = 0
    for name in sorted(people):
        expected = re.compile(r'^Commissioner\s+' + re.escape(name) + r',\s*District\s+[IVX]+\b', re.I)
        matches = [(index, urljoin(board_url, href)) for index, (href, label) in enumerate(links)
                   if expected.match(' '.join(label.split()))]
        matches = [(index, url) for index, url in matches if re.fullmatch(
            r'https://www\.douglasco\.gov/board-county-commissioners/commissioner-[a-z-]+-district-[ivx]+/', url)]
        if len(matches) != 1:
            continue
        anchor, profile_url = matches[0]
        try:
            profile, observations = html_page(profile_url, 'www.douglasco.gov', rules, budget)
            archive(data_dir, observations)
            image = official_profile_image(profile, name)
            if not image or not allowed(rules, image[0]):
                continue
            image_url, caption = image
            observations = get(image_url, {'www.douglasco.gov'}, maximum=2_000_000, timeout=20, redirects=0, budget=budget)
            archive(data_dir, observations)
            image_receipt, content = observations[-1]
            if image_receipt['status'] != 200 or not content.startswith(b'\xff\xd8\xff'):
                continue
            info = {'kind': 'commissioner_profile', 'board_url': board_url, 'board_sha256': sha(board),
                    'board_anchor': anchor, 'source_url': profile_url, 'source_sha256': sha(profile),
                    'image_url': image_url, 'image_sha256': sha(content), 'caption': caption,
                    'extension': 'jpg', 'image_receipt': image_receipt}
            with db:
                db.execute('INSERT OR REPLACE INTO official_portraits VALUES (?,?,?)', (ref, name, json.dumps(info)))
            stored += 1
        except (OSError, UnicodeError, ValueError, RuntimeError, json.JSONDecodeError):
            continue
    return stored


def collect_portraits(data_dir, db, limit):
    names = {}
    for ref, raw in db.execute("SELECT jurisdiction_ref,result_json FROM document_candidates WHERE state='matched'"):
        try:
            result = json.loads(raw)
            # OCR currently flattens an entire page into one line; it cannot
            # establish that a named official is near a particular reference.
            if result.get('extraction_status') == 'complete_with_ocr':
                continue
            digest = result['extracted_sha256']
            if not re.fullmatch(r'[0-9a-f]{64}', digest):
                continue
            content = read_blob(data_dir / 'extracted' / digest, 5 * 1024 * 1024)
            if sha(content) != digest:
                continue
            number = result['finding']['citations'][0]['locator']['start']
            for person in officials_near_reference(content.decode('utf8').splitlines(), number):
                names.setdefault(ref, set()).add(person['name'])
        except (OSError, UnicodeError, ValueError, TypeError, KeyError, IndexError):
            continue
    receipts = observation_keys(data_dir)
    clients = clients_from_store(db, data_dir)
    for client, matter, history_id, raw, ref in db.execute('''SELECT a.client,a.matter_id,a.history_id,
            a.captures_json,l.jurisdiction_ref FROM matter_actions a JOIN leads l
            ON a.client=l.client AND a.matter_id=l.matter_id'''):
        if clients.get(client) != ref:
            continue
        try:
            action = materialize_action(data_dir, receipts, client, matter, history_id, raw)
            for vote in action['votes']:
                names.setdefault(ref, set()).add(vote['name'])
        except (OSError, UnicodeError, ValueError, TypeError, KeyError, json.JSONDecodeError):
            continue
    budget = DownloadBudget(50 * 1024 * 1024)
    stored = 0
    attempts = dict(db.execute('SELECT jurisdiction_ref,attempted_at FROM portrait_attempts'))
    ordered = sorted(names.items(), key=lambda item: (attempts.get(item[0], ''), item[0]))
    for ref, people in ordered[:limit]:
        try:
            row = db.execute('SELECT source_url FROM discovery WHERE jurisdiction_ref=?', (ref,)).fetchone()
            if not row:
                continue
            host = urlsplit(normalized_home(row[0])).hostname
            rules, robots = get_robots(host, budget=budget)
            archive(data_dir, robots)
            if ref == 'us-co-county-08035' and host == 'www.douglasco.gov':
                stored += collect_douglas_profiles(data_dir, db, ref, people, rules, budget)
                continue
            directory = f'https://{host}/m/Directory'
            body, observations = html_page(directory, host, rules, budget)
            archive(data_dir, observations)
            images = parse_images(body)
            alias_url = None
            alias_body = b''
            # The county's public commissioners page spells out nicknames;
            # never infer that Liz and Elizabeth are the same person by guess.
            if host.removeprefix('www.') == 'gunnisoncounty.org':
                alias_url = f'https://{host}/127/County-Commissioners'
                alias_body, observations = html_page(alias_url, host, rules, budget)
                archive(data_dir, observations)
            alias_text = alias_body.decode('utf8', 'replace')
            for name in sorted(people):
                candidates = [(urljoin(directory, src), alt) for src, alt in images
                              if portrait_name_matches(name, alt, alias_text)]
                if len(candidates) != 1:
                    continue
                image_url, caption = candidates[0]
                parsed = urlsplit(image_url)
                if parsed.scheme != 'https' or parsed.hostname != host or parsed.username or parsed.password or parsed.fragment or not allowed(rules, image_url):
                    continue
                observations = get(image_url, {host}, maximum=2_000_000, timeout=20, redirects=0, budget=budget)
                archive(data_dir, observations)
                image_receipt, image = observations[-1]
                if image_receipt['status'] != 200:
                    continue
                if image.startswith(b'\xff\xd8\xff'):
                    extension = 'jpg'
                elif image.startswith(b'\x89PNG\r\n\x1a\n'):
                    extension = 'png'
                elif image.startswith(b'RIFF') and image[8:12] == b'WEBP':
                    extension = 'webp'
                else:
                    continue
                info = {'source_url': directory, 'source_sha256': sha(body), 'image_url': image_url,
                        'image_sha256': sha(image), 'caption': caption, 'extension': extension,
                        'image_receipt': image_receipt, 'alias_url': alias_url,
                        'alias_sha256': sha(alias_body) if alias_body else None}
                with db:
                    db.execute('INSERT OR REPLACE INTO official_portraits VALUES (?,?,?)', (ref, name, json.dumps(info)))
                stored += 1
        except (OSError, UnicodeError, ValueError, RuntimeError, json.JSONDecodeError) as error:
            print(f'{ref}: official portrait search incomplete ({str(error)[:100]})', flush=True)
        finally:
            with db:
                db.execute('INSERT OR REPLACE INTO portrait_attempts VALUES (?,?)', (ref, utcnow()))
    print(f'Preserved {stored} identity-labeled official portrait(s)', flush=True)


def checked_portrait(data_dir, db, receipts, ref, name, assets_dir):
    if assets_dir is None:
        return None
    row = db.execute('SELECT portrait_json FROM official_portraits WHERE jurisdiction_ref=? AND name=?', (ref, name)).fetchone()
    if not row:
        return None
    info = json.loads(row[0])
    page_url, page_digest = info['source_url'], info['source_sha256']
    image_url, image_digest = info['image_url'], info['image_sha256']
    if ((page_url, page_digest) not in receipts or (image_url, image_digest) not in receipts
            or not receipt_valid(info['image_receipt'], image_url, image_digest)):
        raise ValueError('missing official portrait receipt')
    page = source_blob(data_dir, page_digest)
    if info.get('kind') == 'commissioner_profile':
        board_url = 'https://www.douglasco.gov/board-county-commissioners/'
        if ref != 'us-co-county-08035' or info['board_url'] != board_url or (board_url, info['board_sha256']) not in receipts:
            raise ValueError('missing official commissioner roster')
        board = parse_links(source_blob(data_dir, info['board_sha256']))
        anchor = info['board_anchor']
        expected = re.compile(r'^Commissioner\s+' + re.escape(name) + r',\s*District\s+[IVX]+\b', re.I)
        if (type(anchor) is not int or not 0 <= anchor < len(board)
                or urljoin(board_url, board[anchor][0]) != page_url
                or not expected.match(' '.join(board[anchor][1].split()))
                or official_profile_image(page, name) != (image_url, info['caption'])):
            raise ValueError('commissioner profile does not bind the person and image')
    else:
        if not any(urljoin(page_url, src) == image_url and alt == info['caption'] for src, alt in parse_images(page)):
            raise ValueError('portrait is not bound to the official directory image')
        alias_text = ''
        if info.get('alias_url'):
            if (info['alias_url'], info['alias_sha256']) not in receipts:
                raise ValueError('missing official nickname source receipt')
            alias_text = source_blob(data_dir, info['alias_sha256']).decode('utf8', 'replace')
            if urlsplit(info['alias_url']).hostname != urlsplit(page_url).hostname:
                raise ValueError('nickname source is not on the official host')
        if not portrait_name_matches(name, info['caption'], alias_text):
            raise ValueError('portrait caption does not identify the named person')
    host = urlsplit(page_url).hostname
    directory = db.execute('SELECT source_url FROM discovery WHERE jurisdiction_ref=?', (ref,)).fetchone()
    if (not directory or urlsplit(normalized_home(directory[0])).hostname.removeprefix('www.') != host.removeprefix('www.')
            or urlsplit(image_url).hostname != host):
        raise ValueError('portrait source is outside the jurisdiction website')
    image = source_blob(data_dir, image_digest, 2_000_000)
    extension = info['extension']
    if not ((extension == 'jpg' and image.startswith(b'\xff\xd8\xff'))
            or (extension == 'png' and image.startswith(b'\x89PNG\r\n\x1a\n'))
            or (extension == 'webp' and image.startswith(b'RIFF') and image[8:12] == b'WEBP')):
        raise ValueError('official portrait file type mismatch')
    assets_dir.mkdir(parents=True, exist_ok=True)
    if assets_dir.is_symlink() or not assets_dir.is_dir():
        raise ValueError('portrait output directory is unsafe')
    target = assets_dir / f'{image_digest}.{extension}'
    try:
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
    except FileExistsError:
        if sha(read_blob(target, 2_000_000)) != image_digest:
            raise ValueError('existing portrait output differs')
    else:
        with os.fdopen(fd, 'wb') as handle:
            handle.write(image)
            handle.flush()
            os.fsync(handle.fileno())
    return {'src': f'./research-portraits/{image_digest}.{extension}', 'sha256': image_digest,
            'source_url': page_url, 'image_url': image_url, 'caption': info['caption'],
            'portrait_date': 'not_established'}


def arapahoe_budget_portrait(data_dir, receipts, ref, client, vote, assets_dir):
    """Bind a scoped recorded voter to a pinned JPEG from the county budget."""
    if assets_dir is None or ref != 'us-co-county-08005' or client != 'arapahoe':
        return None
    proof = json.loads(read_blob(ROOT / 'fixtures/atlas/portrait-sources/arapahoe-2022-budget-proof.json', 100_000))
    url = 'https://cms3.revize.com/revize/arapahoe/Finance/Complete%20Budget%20Document/2023%20Recommended%20Budget.pdf'
    source_digest = 'dd6476e64941674e310110b375d90dcf9f81ef904f0866183337b2b0fb867c43'
    page_digest = '23fc3217eaa1024751d62fde024dcf73f9f0ba6560fb4f86d7df1534f9fc6777'
    if (proof['source_url'] != url or proof['source_sha256'] != source_digest or proof['source_page'] != 7
            or proof['page_fixture'] != 'fixtures/atlas/portrait-sources/arapahoe-2022-budget-page.pdf'
            or proof['page_fixture_sha256'] != page_digest
            or sha(read_blob(ROOT / proof['page_fixture'], 2_000_000)) != page_digest):
        raise ValueError('county budget portrait proof differs')
    matches = [item for item in proof['portraits'] if item['name'] == vote['name'] and item['person_id'] == vote['person_id']]
    if len(matches) != 1:
        return None
    if (url, source_digest) not in receipts:
        raise ValueError('county budget source receipt is missing')
    source_blob(data_dir, source_digest, 20 * 1024 * 1024)
    item = matches[0]
    image_digest = item['sha256']
    if not re.fullmatch(r'[0-9a-f]{64}', image_digest) or type(item['image_index']) is not int:
        raise ValueError('invalid budget portrait identity')
    image = read_blob(ROOT / 'web/panopticon-terminal/research-portraits' / f'{image_digest}.jpg', 2_000_000)
    if not image.startswith(b'\xff\xd8\xff') or sha(image) != image_digest:
        raise ValueError('county budget portrait bytes differ')
    assets_dir.mkdir(parents=True, exist_ok=True)
    if assets_dir.is_symlink() or not assets_dir.is_dir():
        raise ValueError('portrait output directory is unsafe')
    target = assets_dir / f'{image_digest}.jpg'
    try:
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
    except FileExistsError:
        if sha(read_blob(target, 2_000_000)) != image_digest:
            raise ValueError('existing portrait output differs')
    else:
        with os.fdopen(fd, 'wb') as handle:
            handle.write(image)
            handle.flush()
            os.fsync(handle.fileno())
    return {'src': f'./research-portraits/{image_digest}.jpg', 'sha256': image_digest,
            'source_kind': 'embedded_pdf', 'source_url': url, 'source_document_sha256': source_digest,
            'source_page': 7, 'embedded_image_index': item['image_index'],
            'caption': f"2022 county board; source labels this portrait {item['source_label']}.",
            'portrait_date': 'not_established'}


def littleton_report_portrait(data_dir, receipts, ref, client, vote, assets_dir):
    """Bind a recorded voter to a labeled portrait in a city newsletter."""
    if assets_dir is None or ref != 'us-co-place-0845255' or client != 'littletongov':
        return None
    later = vote.get('person_id') in (502, 503)
    year = 2024 if later else 2023
    proof_path = f'fixtures/atlas/portrait-sources/littleton-{year}-report-proof.json'
    fixture = f'fixtures/atlas/portrait-sources/littleton-{year}-report-page.pdf'
    proof = json.loads(read_blob(ROOT / proof_path, 100_000))
    if later:
        url = 'https://www.littletonco.gov/files/sharedassets/public/v/1/communications/littleton-reports/2024-littleton-reports/2024-12-dec-jan-littleton-report.pdf'
        source_digest = 'f8264f7fc25a40afdc6c34ba41ea1864bbc5ac219f2485f91bf2c6ae9ba7e394'
        page_digest = '0bb29b5d44dae80db18632b843b3f97f71a32f525d34df84f072088385ecdf50'
    else:
        url = 'https://www.littletonco.gov/files/sharedassets/public/v/1/communications/littleton-reports/2023-littleton-reports/2023-02-feb-littleton-report.pdf'
        source_digest = '34a8dc30a20a37c23179b122722175be6cf3c253c825009db6fbdeb4cff6ea7e'
        page_digest = 'b6be37f13111fc566f28900396619154179a4dce635529b538c67fe1a83c6ada'
    if (proof['source_url'] != url or proof['source_sha256'] != source_digest or proof['source_page'] != 3
            or proof['page_fixture'] != fixture or proof['page_fixture_sha256'] != page_digest
            or sha(read_blob(ROOT / proof['page_fixture'], 2_000_000)) != page_digest):
        raise ValueError('Littleton portrait proof differs')
    matches = [item for item in proof['portraits'] if item['name'] == vote['name'] and item['person_id'] == vote['person_id']]
    if len(matches) != 1:
        return None
    if (url, source_digest) not in receipts:
        raise ValueError('Littleton portrait source receipt is missing')
    source_blob(data_dir, source_digest, 20 * 1024 * 1024)
    item = matches[0]
    image_digest, original_digest = item['sha256'], item.get('original_sha256')
    if (not re.fullmatch(r'[0-9a-f]{64}', image_digest)
            or (not later and not re.fullmatch(r'[0-9a-f]{64}', original_digest or ''))
            or type(item['image_index']) is not int):
        raise ValueError('invalid Littleton portrait identity')
    extension = 'jpg' if later else 'png'
    signature = b'\xff\xd8\xff' if later else b'\x89PNG\r\n\x1a\n'
    image = read_blob(ROOT / 'web/panopticon-terminal/research-portraits' / f'{image_digest}.{extension}', 2_000_000)
    if not image.startswith(signature) or sha(image) != image_digest:
        raise ValueError('Littleton portrait display bytes differ')
    assets_dir.mkdir(parents=True, exist_ok=True)
    if assets_dir.is_symlink() or not assets_dir.is_dir():
        raise ValueError('portrait output directory is unsafe')
    target = assets_dir / f'{image_digest}.{extension}'
    try:
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
    except FileExistsError:
        if sha(read_blob(target, 2_000_000)) != image_digest:
            raise ValueError('existing portrait output differs')
    else:
        with os.fdopen(fd, 'wb') as handle:
            handle.write(image)
            handle.flush()
            os.fsync(handle.fileno())
    result = {'src': f'./research-portraits/{image_digest}.{extension}', 'sha256': image_digest,
              'source_kind': 'embedded_pdf', 'source_url': url, 'source_document_sha256': source_digest,
              'source_page': 3, 'embedded_image_index': item['image_index'],
              'caption': f"{year} city council; source labels this portrait {item['name']}.",
              'portrait_date': 'not_established'}
    if not later:
        result.update(embedded_original_sha256=original_digest, image_transform='lossless_png_from_jpeg2000')
    return result


def materialize_action(data_dir, receipts, client, matter, history_id, raw):
    """Reopen every exact source before associating a recorded vote with a matter."""
    captures = json.loads(raw)

    def reopen(receipt, url, expected):
        if (not receipt or not receipt_valid(receipt, url, receipt.get('sha256'))
                or (url, receipt['sha256']) not in receipts):
            raise ValueError('missing or mismatched action source receipt')
        value = json.loads(source_blob(data_dir, receipt['sha256']))
        if not isinstance(value, expected) or (isinstance(value, list) and len(value) > 1000):
            raise ValueError('invalid action source shape')
        return value

    histories = reopen(captures['history'], action_url(client, 'histories', matter), list)
    index = captures['history_index']
    if type(index) is not int or not 0 <= index < len(histories):
        raise ValueError('invalid action locator')
    history = histories[index]
    event = history.get('MatterHistoryEventId')
    if history.get('MatterHistoryId') != history_id or type(event) is not int:
        raise ValueError('action no longer identifies its meeting')
    item_url = action_url(client, 'item', event, history_id)
    item = reopen(captures['item'], item_url, dict)
    if (item.get('EventItemId') != history_id or item.get('EventItemEventId') != event
            or item.get('EventItemMatterId') != matter
            or item.get('EventItemActionName') != history.get('MatterHistoryActionName')
            or item.get('EventItemPassedFlag') != history.get('MatterHistoryPassedFlag')
            or item.get('EventItemConsent') != history.get('MatterHistoryConsent')):
        raise ValueError('meeting item and matter action are inconsistent')
    votes_url = action_url(client, 'votes', history_id)
    votes = reopen(captures['votes'], votes_url, list)
    ids = {row.get('VoteEventItemId') for row in votes if isinstance(row, dict)
           and type(row.get('VoteEventItemId')) is int}
    scope, block_url = 'not_established', None
    if ids == {history_id}:
        if history.get('MatterHistoryConsent') == 1:
            scope = 'consent_designated_item'
        elif history.get('MatterHistoryConsent') == 0:
            scope = 'item'
    elif len(ids) == 1 and history.get('MatterHistoryConsent') == 1:
        block = next(iter(ids))
        block_url = action_url(client, 'item', event, block)
        block_item = reopen(captures.get('block'), block_url, dict)
        label = str(block_item.get('EventItemTitle') or '') + ' ' + str(block_item.get('EventItemActionName') or '')
        if (block_item.get('EventItemId') == block and block_item.get('EventItemEventId') == event
                and block_item.get('EventItemMatterId') is None and re.search(r'\bconsent\b', label, re.I)):
            scope = 'consent_block'
        else:
            block_url = None
    values = {'aye': 'Aye', 'ayes': 'Aye', 'yes': 'Yes', 'no': 'No', 'noes': 'No',
              'nay': 'Nay', 'nays': 'Nay', 'abstain': 'Abstain', 'abstained': 'Abstain',
              'absent': 'Absent', 'recused': 'Recused'}
    public_votes, seen = [], set()
    for ordinal, vote in enumerate(votes if scope != 'not_established' else []):
        if not isinstance(vote, dict) or vote.get('VoteEventItemId') not in ids:
            continue
        name, value, person, vote_id = vote.get('VotePersonName'), vote.get('VoteValueName'), vote.get('VotePersonId'), vote.get('VoteId')
        if (not isinstance(name, str) or public_text(name, 100) != name.strip() or not name.strip()
                or not isinstance(value, str) or value.strip().casefold() not in values
                or type(person) is not int or person <= 0 or type(vote_id) is not int or vote_id <= 0):
            continue
        if person in seen:
            raise ValueError('duplicate person in recorded action')
        seen.add(person)
        public_votes.append({'name': name.strip(), 'value': values[value.strip().casefold()],
                             'locator': f'$[{ordinal}]', 'person_id': person, 'vote_id': vote_id})
    date = str(history.get('MatterHistoryActionDate') or '')[:10]
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', date):
        date = 'not_established'
    result = {'date': date, 'body': public_text(history.get('MatterHistoryActionBodyName'), 200),
              'action': public_text(history.get('MatterHistoryActionName'), 200),
              'outcome': public_text(history.get('MatterHistoryPassedFlagName'), 100) or 'not_established',
              'vote_scope': scope, 'history_url': captures['history']['url'],
              'history_sha256': captures['history']['sha256'], 'history_locator': f'$[{index}]',
              'item_url': item_url, 'item_sha256': captures['item']['sha256'],
              'votes_url': votes_url, 'votes_sha256': captures['votes']['sha256'],
              'captured_at': captures['votes']['retrieved_at'], 'votes': public_votes}
    if block_url:
        result['block_url'] = block_url
        result['block_sha256'] = captures['block']['sha256']
    return result


def materialize_suite(data_dir, receipts, meeting, candidate_raw, result_raw, updated, historical):
    result = json.loads(result_raw)
    candidate = result.get('candidate', json.loads(candidate_raw))
    url = f'https://{SUITE_HOST}/iip/elpaso/meeting/details/{meeting}'
    if candidate['url'] != url or not receipt_valid(result['receipt'], url, result['receipt']['sha256']):
        raise ValueError('AgendaSuite meeting identity or receipt mismatch')
    for source, digest in [(SUITE_AUTHORITY, candidate['authority_sha256']),
                           (SUITE_LIST, candidate['root_sha256']),
                           (candidate['page_url'], candidate['page_sha256']),
                           (url, result['receipt']['sha256'])]:
        if (source, digest) not in receipts:
            raise ValueError('missing AgendaSuite capture receipt')
    authority = parse_links(source_blob(data_dir, candidate['authority_sha256']))
    anchor = candidate['authority_anchor']
    if type(anchor) is not int or not 0 <= anchor < len(authority) or urljoin(SUITE_AUTHORITY, authority[anchor][0]) != SUITE_LIST:
        raise ValueError('county site does not bind the meeting portal')
    root = parse_links(source_blob(data_dir, candidate['root_sha256']))
    page_url = candidate['page_url']
    if page_url == SUITE_LIST:
        if candidate['page_sha256'] != candidate['root_sha256'] or candidate['page_anchor'] is not None:
            raise ValueError('invalid root calendar capture')
    else:
        page_anchor = candidate['page_anchor']
        if (page_url not in (SUITE_LIST + '/2', SUITE_LIST + '/3') or type(page_anchor) is not int
                or not 0 <= page_anchor < len(root) or urljoin(SUITE_LIST, root[page_anchor][0]) != page_url):
            raise ValueError('calendar page not linked from its index')
    page = parse_links(source_blob(data_dir, candidate['page_sha256']))
    anchor = candidate['meeting_anchor']
    if type(anchor) is not int or not 0 <= anchor < len(page) or urljoin(page_url, page[anchor][0]) != url:
        raise ValueError('meeting not linked from its calendar page')
    nodes = visible_text(source_blob(data_dir, result['receipt']['sha256']))
    index = result['node']
    if type(index) is not int or not 0 <= index < len(nodes):
        raise ValueError('invalid AgendaSuite text locator')
    terms = matching_terms(nodes[index])
    if not terms:
        raise ValueError('AgendaSuite text no longer matches')
    digest = result['receipt']['sha256']
    return {'id': f'co-agendasuite-elpaso-{meeting}-{digest[:12]}',
            'kind': 'agenda_portal_reference', 'confidence': 'unconfirmed_source_reference',
            'jurisdiction_ref': SUITE_REF, 'jurisdiction': 'El Paso County portal',
            'file': f'Meeting {meeting}', 'title': 'Possible ' + ', '.join(terms) + ' reference in a public meeting page.',
            'signals': terms, 'source_status': '', 'source_url': url, 'source_sha256': digest,
            'locator': f'HTML text node {index + 1}', 'captured_at': result['receipt']['retrieved_at'],
            'last_screened_at': updated, 'historical': bool(historical)}


def export_report(data_dir, db, output, assets_dir=None):
    geography = {item['id']: item for item in json.loads(GEOGRAPHY.read_text())['jurisdictions']}
    clients = clients_from_store(db, data_dir)
    receipts = observation_keys(data_dir)
    records = []
    omitted = 0
    non_surveillance_withheld = 0
    omission_reasons = Counter()
    for identifier, ref, client, matter, page, row_index, source_digest, raw, updated in db.execute(
            'SELECT id,jurisdiction_ref,client,matter_id,page,row_index,source_sha256,record_json,updated_at FROM leads ORDER BY id'):
        try:
            if clients.get(client) != ref or ref not in geography or identifier != f'co-legistar-{client}-{matter}':
                raise ValueError('unbound jurisdiction or matter')
            record = json.loads(raw)
            row = db.execute('SELECT receipt_json FROM scan_history WHERE client=? AND page=? AND source_sha256=?',
                             (client, page, source_digest)).fetchone()
            if row is None:
                raise ValueError('missing archived source receipt')
            receipt = json.loads(row[0])
            url = receipt['url']
            parsed = urlsplit(url)
            params = parse_qs(parsed.query, strict_parsing=True)
            top = int(params['$top'][0])
            if (parsed.scheme != 'https' or parsed.hostname != API_HOST or parsed.path != f'/v1/{client}/matters'
                    or not 1 <= top <= 100 or url != query_url(client, page, top)
                    or not receipt_valid(receipt, url, source_digest)):
                raise ValueError('source receipt is not the expected official query')
            original = json.loads(source_blob(data_dir, source_digest))
            if not isinstance(original, list) or type(row_index) is not int or not 0 <= row_index < len(original):
                raise ValueError('source row is missing')
            item = original[row_index]
            title = item.get('MatterTitle')
            terms = matching_terms(title) if isinstance(title, str) else []
            if item.get('MatterId') != matter or record.get('title') != title or not terms:
                raise ValueError('source row no longer binds the matter and title')
            entry = {'id': identifier, 'kind': 'matter_reference', 'confidence': 'unconfirmed_source_reference', 'jurisdiction_ref': ref,
                'jurisdiction': jurisdiction_name(geography[ref]), 'file': public_text(item.get('MatterFile'), 100),
                'title': reference_title(title, terms), 'signals': terms, 'source_status': public_text(item.get('MatterStatusName'), 100),
                'source_url': url, 'matter_url': f'https://{API_HOST}/v1/{client}/Matters/{matter}',
                'source_sha256': source_digest, 'locator': f'$[{row_index}].MatterTitle',
                'captured_at': receipt['retrieved_at'], 'last_screened_at': updated}
            motions = []
            current_history = db.execute('SELECT history_sha256 FROM matter_action_checks WHERE client=? AND matter_id=?', (client, matter)).fetchone()
            for action_id, action_raw in db.execute('SELECT history_id,captures_json FROM matter_actions WHERE client=? AND matter_id=? ORDER BY history_id DESC LIMIT 10', (client, matter)):
                try:
                    motion = materialize_action(data_dir, receipts, client, matter, action_id, action_raw)
                    motion['historical'] = bool(current_history and current_history[0] and current_history[0] != motion['history_sha256'])
                    for vote in motion['votes']:
                        try:
                            portrait = checked_portrait(data_dir, db, receipts, ref, vote['name'], assets_dir)
                            if not portrait:
                                portrait = arapahoe_budget_portrait(data_dir, receipts, ref, client, vote, assets_dir)
                            if not portrait:
                                portrait = littleton_report_portrait(data_dir, receipts, ref, client, vote, assets_dir)
                            if portrait:
                                vote['portrait'] = portrait
                        except (OSError, UnicodeError, ValueError, TypeError, KeyError, json.JSONDecodeError):
                            pass
                    motions.append(motion)
                except (OSError, UnicodeError, ValueError, TypeError, KeyError, json.JSONDecodeError) as error:
                    omission_reasons['action: ' + str(error)[:90]] += 1
            if motions:
                entry['actions'] = motions
            records.append(entry)
        except (OSError, ValueError, TypeError, KeyError, json.JSONDecodeError) as error:
            omitted += 1
            omission_reasons[str(error)[:100]] += 1
    seen_documents = set()
    for ref, url, candidate_raw, result_raw, updated, historical in db.execute('''
            SELECT jurisdiction_ref,url,candidate_json,result_json,updated_at,0 AS historical
            FROM document_candidates WHERE state='matched'
            UNION ALL
            SELECT jurisdiction_ref,url,candidate_json,result_json,captured_at,1 AS historical
            FROM document_history ORDER BY historical,jurisdiction_ref,url'''):
        try:
            if ref not in geography:
                raise ValueError('unknown geography')
            result = json.loads(result_raw)
            if (result.get('extraction_status') == 'complete_with_ocr' and
                    (result.get('ocr_page_set_validated') is not True or result.get('ocr_page_text_mapping_version') != 2)):
                raise ValueError('OCR page set has not been validated')
            if result.get('document_format') == 'docx' and result.get('docx_deleted_text_excluded') is not True:
                raise ValueError('Word document deleted text has not been excluded')
            candidate = result.get('candidate', json.loads(candidate_raw))
            if candidate.get('url') != url:
                raise ValueError('document URL differs from the captured index candidate')
            receipt = result['receipt']
            doc_digest = result['document_sha256']
            format_name = result.get('document_format', 'pdf')
            prefix = {'pdf': b'%PDF-', 'docx': b'PK\x03\x04'}.get(format_name)
            if not prefix or not receipt_valid(receipt, url, doc_digest) or not source_blob(data_dir, doc_digest, 20 * 1024 * 1024).startswith(prefix):
                raise ValueError('document receipt mismatch')
            proof = source_blob(data_dir, candidate['index_sha256'])
            if (candidate['index_url'], candidate['index_sha256']) not in receipts:
                raise ValueError('missing captured index receipt')
            links = parse_links(proof)
            anchor = candidate['anchor_index']
            if type(anchor) is not int or not 0 <= anchor < len(links) or urljoin(candidate['index_url'], links[anchor][0]) != url:
                raise ValueError('document is not bound to its preserved index anchor')
            if urlsplit(url).hostname != urlsplit(candidate['index_url']).hostname:
                raise ValueError('index and document hosts differ')
            directory_row = db.execute('SELECT source_url FROM discovery WHERE jurisdiction_ref=?', (ref,)).fetchone()
            directory_host = urlsplit(normalized_home(directory_row[0])).hostname.removeprefix('www.') if directory_row else ''
            if not directory_host or directory_host != urlsplit(url).hostname.removeprefix('www.'):
                raise ValueError('document is not attached to its directory website lead')
            finding = result['finding']
            text_digest = result['extracted_sha256']
            if not re.fullmatch(r'[0-9a-f]{64}', text_digest):
                raise ValueError('invalid extracted text digest')
            text = read_blob(data_dir / 'extracted' / text_digest, 5 * 1024 * 1024)
            if sha(text) != text_digest:
                raise ValueError('extracted text differs from digest')
            lines = text.decode('utf8').splitlines()
            citation = finding['citations'][0]
            number = citation['locator']['start']
            if (citation['locator']['kind'] != 'line' or type(number) is not int or not 1 <= number <= len(lines)
                    or citation['quote'] != lines[number - 1]
                    or citation['evidence_id'] != result['evidence_id'] or finding['evidence_id'] != result['evidence_id']):
                raise ValueError('finding does not bind to its extracted line')
            labels = labels_at_quote(finding['matched_rule_ids'], citation['quote'])
            if not labels:
                raise ValueError('no supported rule is present at the cited line')
            if non_surveillance_flock_context(lines, number, labels):
                non_surveillance_withheld += 1
                continue
            key = (ref, doc_digest)
            if key in seen_documents:
                continue
            seen_documents.add(key)
            quote = citation['quote']
            # A public comment can contain a private person's name or address.
            private_context = public_comment_context(lines, number)
            ocr_page = result.get('extraction_status') == 'complete_with_ocr'
            title = reference_title(quote, labels, private_context or ocr_page)
            people = [] if private_context or ocr_page else officials_near_reference(lines, number)
            for person in people:
                try:
                    portrait = checked_portrait(data_dir, db, receipts, ref, person['name'], assets_dir)
                    if portrait:
                        person['portrait'] = portrait
                except (OSError, UnicodeError, ValueError, TypeError, KeyError, json.JSONDecodeError):
                    # A missing or mismatched portrait cannot suppress a
                    # separate, valid source reference.
                    continue
            records.append({'id': 'co-document-' + sha((ref + '\0' + doc_digest).encode())[:24],
                'kind': 'meeting_document', 'confidence': 'unconfirmed_source_reference', 'jurisdiction_ref': ref, 'jurisdiction': jurisdiction_name(geography[ref]),
                'file': public_text(candidate.get('label'), 120), 'title': title,
                'signals': labels, 'source_status': '', 'source_url': url,
                'source_sha256': doc_digest, 'locator': f'extracted line {number}',
                'captured_at': receipt['retrieved_at'], 'last_screened_at': updated,
                'people_context': people,
                'document_format': format_name,
                'historical': bool(historical),
                'ocr': result.get('extraction_status') == 'complete_with_ocr'})
        except (OSError, UnicodeError, ValueError, TypeError, KeyError, json.JSONDecodeError) as error:
            omitted += 1
            omission_reasons[str(error)[:100]] += 1
    seen_suite = set()
    for meeting, candidate, result, updated, historical in db.execute('''
            SELECT meeting_id,candidate_json,result_json,updated_at,0 AS historical FROM suite_meetings WHERE state='matched'
            UNION ALL SELECT meeting_id,candidate_json,result_json,updated_at,1 AS historical FROM suite_history
            ORDER BY historical,meeting_id'''):
        try:
            item = materialize_suite(data_dir, receipts, meeting, candidate, result, updated, historical)
            if item['id'] not in seen_suite:
                seen_suite.add(item['id'])
                records.append(item)
        except (OSError, UnicodeError, ValueError, TypeError, KeyError, json.JSONDecodeError) as error:
            omitted += 1
            omission_reasons['meeting page: ' + str(error)[:85]] += 1
    if len(records) > MAX_RESULTS:
        raise ValueError('research report exceeds record limit')
    records.sort(key=lambda item: (item['jurisdiction'], item['id']))
    reported_state = """CASE WHEN state IN ('matched','no_match') AND (
        (json_extract(result_json,'$.extraction_status')='complete_with_ocr'
         AND (COALESCE(json_extract(result_json,'$.ocr_page_set_validated'),0)!=1
              OR COALESCE(json_extract(result_json,'$.ocr_page_text_mapping_version'),0)!=2))
        OR (json_extract(result_json,'$.document_format')='docx'
         AND COALESCE(json_extract(result_json,'$.docx_deleted_text_excluded'),0)!=1))
        THEN 'error' ELSE state END"""
    status = dict(db.execute(f'SELECT {reported_state} AS reported,count(*) FROM document_candidates GROUP BY reported').fetchall())
    document_counts = {}
    for ref, state, count in db.execute(f'SELECT jurisdiction_ref,{reported_state} AS reported,count(*) FROM document_candidates GROUP BY jurisdiction_ref,reported'):
        document_counts.setdefault(ref, {})[state] = count
    ocr_rechecks = dict(db.execute('''SELECT jurisdiction_ref,count(*) FROM document_candidates
        WHERE state IN ('matched','no_match') AND json_extract(result_json,'$.extraction_status')='complete_with_ocr'
        AND (COALESCE(json_extract(result_json,'$.ocr_page_set_validated'),0)!=1
             OR COALESCE(json_extract(result_json,'$.ocr_page_text_mapping_version'),0)!=2)
        GROUP BY jurisdiction_ref'''))
    portal_counts = Counter()
    for client, count in db.execute('SELECT client,count(*) FROM scans GROUP BY client'):
        if client in clients:
            portal_counts[clients[client]] += count
    history_counts = {}
    for ref, state, count in db.execute('SELECT jurisdiction_ref,state,count(*) FROM civic_history_checks GROUP BY jurisdiction_ref,state'):
        history_counts.setdefault(ref, {})[state] = count
    suite_screened = db.execute("SELECT count(*) FROM suite_meetings WHERE state IN ('matched','no_match')").fetchone()[0]
    attempts = []
    for ref, checked, raw, attempted, attempt_state in db.execute('''SELECT d.jurisdiction_ref,d.checked_at,
            d.result_json,a.attempted_at,a.state FROM discovery d LEFT JOIN discovery_attempts a USING(jurisdiction_ref)
            ORDER BY d.jurisdiction_ref'''):
        try:
            entry = json.loads(raw)
            if ref not in geography or entry['jurisdiction_ref'] != ref:
                continue
            checked = attempted or checked
            datetime.fromisoformat(checked)
            if entry['state'] not in ('directory_site_checked', 'failed'):
                continue
            counts = document_counts.get(ref, {})
            attempts.append({'jurisdiction_ref': ref, 'checked_at': checked,
                             'website_check': 'site_checked' if (attempt_state or entry['state']) == 'directory_site_checked' else 'unavailable',
                             'documents_screened': counts.get('matched', 0) + counts.get('no_match', 0),
                             'documents_failed': counts.get('error', 0), 'documents_queued': counts.get('pending', 0),
                             'ocr_revalidation_pending': ocr_rechecks.get(ref, 0),
                             'portal_pages_checked': portal_counts[ref],
                             'historical_index_pages_checked': history_counts.get(ref, {}).get('ok', 0),
                             'historical_index_errors': history_counts.get(ref, {}).get('error', 0),
                             'agenda_meeting_pages_screened': suite_screened if ref == SUITE_REF else 0,
                             'lookback_period': None})
        except (ValueError, TypeError, KeyError, json.JSONDecodeError):
            continue
    report = {'schema_version': 'pnull-research-v1', 'state': 'preliminary', 'generated_at': utcnow(),
              'notice': 'Possible references from public sources. A mention does not establish an approval, individual vote, contract execution, or deployment.',
              'coverage': {'lookback_period': None, 'jurisdictions_with_mentions': len({x['jurisdiction_ref'] for x in records}),
                           'matter_references': sum(x['kind'] == 'matter_reference' for x in records),
                           'document_references': sum(x['kind'] == 'meeting_document' for x in records),
                           'agenda_portal_references': sum(x['kind'] == 'agenda_portal_reference' for x in records),
                           'agenda_portal_screening': dict(db.execute('SELECT state,count(*) FROM suite_meetings GROUP BY state').fetchall()),
                           'search_attempts': attempts,
                           'recorded_actions': sum(len(x.get('actions', [])) for x in records),
                           'recorded_vote_entries': sum(len(motion['votes']) for x in records for motion in x.get('actions', [])),
                           'document_screening': status,
                           'ocr_revalidation_pending': sum(ocr_rechecks.values()),
                           'refresh_failures_preserving_prior_results': db.execute("SELECT count(*) FROM document_candidates WHERE state IN ('matched','no_match') AND last_error IS NOT NULL").fetchone()[0],
                           'non_surveillance_mentions_withheld': non_surveillance_withheld,
                           'omitted_failed_provenance': omitted}, 'records': records}
    encoded = (json.dumps(report, ensure_ascii=True, separators=(',', ':')) + '\n').encode()
    if len(encoded) > 8 * 1024 * 1024:
        raise ValueError('research report exceeds output byte limit')
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_name('.' + output.name + f'.{os.getpid()}.tmp')
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
    try:
        with os.fdopen(fd, 'wb') as handle:
            handle.write(encoded)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, output)
    finally:
        temporary.unlink(missing_ok=True)
    print(f"Exported {len(records)} preliminary references in {report['coverage']['jurisdictions_with_mentions']} jurisdictions; {omitted} omitted by validation", flush=True)
    if omission_reasons:
        print('Omission reasons:', dict(omission_reasons), flush=True)


def export_consistent(data_dir, db, output, assets_dir=None):
    """Keep counts and source rows on one SQLite snapshot during live work."""
    with db:
        db.execute('BEGIN')
        export_report(data_dir, db, output, assets_dir)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data-dir', required=True, type=Path)
    subs = parser.add_subparsers(dest='command', required=True)
    d = subs.add_parser('discover')
    d.add_argument('--limit', type=int, default=320)
    d.add_argument('--workers', type=int, default=8)
    d.add_argument('--refresh', action='store_true')
    d.add_argument('--only-usable', action='store_true', help='revisit sites with a prior accessible index')
    d.add_argument('--only-failed', action='store_true', help='retry sites that previously failed')
    history = subs.add_parser('civic-history')
    history.add_argument('--limit', type=int, default=100)
    history.add_argument('--per-place', type=int, default=4)
    history.add_argument('--workers', type=int, default=8)
    history.add_argument('--pending-cap', type=int, default=1000, help='pause discovery when the document queue reaches this size')
    history.add_argument('--jurisdiction', help='one previously discovered Colorado jurisdiction')
    s = subs.add_parser('scan')
    s.add_argument('--max-clients', type=int, default=100)
    s.add_argument('--pages', type=int, default=10)
    s.add_argument('--page-size', type=int, default=100)
    s.add_argument('--client', help='one previously bound Legistar client')
    a = subs.add_parser('actions')
    a.add_argument('--limit', type=int, default=100, help='maximum matters to check per run')
    suite = subs.add_parser('agendasuite')
    suite.add_argument('--limit', type=int, default=60, help='maximum recent meeting pages to screen')
    suite.add_argument('--pages', type=int, default=3, help='maximum linked calendar pages')
    p = subs.add_parser('screen-documents')
    p.add_argument('--pnull-bin', default=os.environ.get('PNUL_BIN', str(ROOT / 'target/debug/pnull')))
    p.add_argument('--limit', type=int, default=100)
    p.add_argument('--per-place', type=int, default=10)
    p.add_argument('--workers', type=int, default=4)
    p.add_argument('--retry-errors', action='store_true')
    p.add_argument('--refresh-existing', action='store_true', help='recheck documents not successfully refreshed in seven days')
    p.add_argument('--ocr', action='store_true', help='enable bounded OCR for scanned PDFs of at most 20 pages')
    ocr = subs.add_parser('ocr-saved')
    ocr.add_argument('--pnull-bin', default=os.environ.get('PNUL_BIN', str(ROOT / 'target/debug/pnull')))
    ocr.add_argument('--limit', type=int, default=30)
    ocr.add_argument('--workers', type=int, default=2)
    ocr.add_argument('--retry-unavailable', action='store_true', help='retry immediately after repairing OCR data or sandbox output')
    ocr.add_argument('--retry-previous-limit', action='store_true', help='retry documents rejected by the former five-page limit')
    ocr.add_argument('--revalidate-ocr', action='store_true', help='recheck older OCR results without validated page sets')
    docx = subs.add_parser('docx-saved')
    docx.add_argument('--limit', type=int, default=50)
    docx.add_argument('--workers', type=int, default=2)
    docx.add_argument('--revalidate-docx', action='store_true', help='recheck earlier Word extractions without deleted-text validation')
    portraits = subs.add_parser('portraits')
    portraits.add_argument('--limit', type=int, default=32, help='maximum jurisdictions with named official context')
    e = subs.add_parser('export')
    e.add_argument('--output', required=True, type=Path)
    e.add_argument('--assets-dir', type=Path, help='directory to copy validated public portraits')
    args = parser.parse_args()
    if args.command == 'discover' and not (1 <= args.limit <= 500 and 1 <= args.workers <= 12):
        parser.error('discovery bounds exceeded')
    if args.command == 'civic-history' and not (1 <= args.limit <= 200 and 1 <= args.per_place <= 20 and 1 <= args.workers <= 8
            and 100 <= args.pending_cap <= 10000
            and (args.jurisdiction is None or re.fullmatch(r'us-co-(?:place|county)-[0-9]{5,7}', args.jurisdiction))):
        parser.error('historical index bounds exceeded')
    if args.command == 'scan' and not (1 <= args.max_clients <= 500 and 1 <= args.pages <= 100 and 1 <= args.page_size <= 100):
        parser.error('scan bounds exceeded')
    if args.command == 'actions' and not 1 <= args.limit <= 200:
        parser.error('action collection bound exceeded')
    if args.command == 'agendasuite' and not (1 <= args.limit <= 100 and 1 <= args.pages <= 3):
        parser.error('AgendaSuite collection bounds exceeded')
    if args.command == 'screen-documents' and not (1 <= args.limit <= 500 and 1 <= args.per_place <= 50 and 1 <= args.workers <= 8):
        parser.error('document screening bounds exceeded')
    if args.command == 'screen-documents' and args.retry_errors and args.refresh_existing:
        parser.error('choose either error retries or existing-document refreshes')
    if args.command == 'ocr-saved' and not (1 <= args.limit <= 100 and 1 <= args.workers <= 8):
        parser.error('saved OCR bounds exceeded')
    if args.command == 'ocr-saved' and args.revalidate_ocr and (args.retry_unavailable or args.retry_previous_limit):
        parser.error('choose OCR revalidation or a retry mode')
    if args.command == 'docx-saved' and not (1 <= args.limit <= 100 and 1 <= args.workers <= 4):
        parser.error('saved Word-document bounds exceeded')
    if args.command == 'portraits' and not 1 <= args.limit <= 100:
        parser.error('portrait search bounds exceeded')
    os.umask(0o077)
    db = root_store(args.data_dir)
    try:
        if args.command != 'export':
            require_storage_headroom(args.data_dir)
        if args.command == 'discover':
            discover(args.data_dir, db, args.limit, args.workers, args.refresh, args.only_usable, args.only_failed)
        elif args.command == 'civic-history':
            civic_history(args.data_dir, db, args.limit, args.per_place, args.workers, args.jurisdiction, args.pending_cap)
        elif args.command == 'scan':
            scan(args.data_dir, db, args.max_clients, args.pages, args.page_size, args.client)
        elif args.command == 'actions':
            collect_actions(args.data_dir, db, args.limit)
        elif args.command == 'agendasuite':
            collect_suite(args.data_dir, db, args.limit, args.pages)
        elif args.command == 'screen-documents':
            screen_documents(args.data_dir, db, args.pnull_bin, args.limit, args.per_place, args.workers,
                             args.retry_errors, args.ocr, args.refresh_existing)
        elif args.command == 'ocr-saved':
            ocr_saved_documents(args.data_dir, db, args.pnull_bin, args.limit, args.workers,
                                args.retry_unavailable, args.revalidate_ocr, args.retry_previous_limit)
        elif args.command == 'docx-saved':
            docx_saved_documents(args.data_dir, db, args.limit, args.workers, args.revalidate_docx)
        elif args.command == 'portraits':
            collect_portraits(args.data_dir, db, args.limit)
        elif args.command == 'export':
            export_consistent(args.data_dir, db, args.output, args.assets_dir)
    finally:
        db.close()


if __name__ == '__main__':
    main()
