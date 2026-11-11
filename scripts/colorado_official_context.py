"""Exact official-agency technology pages; no decisions or votes are inferred."""
import json
from collections import Counter
from datetime import datetime
from html.parser import HTMLParser
import re
from urllib.parse import urlsplit

SEEDS = {
    'sterling-bwc': {
        'jurisdiction': 'Sterling',
        'jurisdiction_ref': 'us-co-place-0873935',
        'url': 'https://www.sterlingcolo.com/departments/police_department/officer_body-worn_cameras.php',
        'agency': 'The Sterling Police Department issues body worn cameras to all sworn members',
        'claim': 'The Sterling Police Department issues body worn cameras to all sworn members',
        'combined_start': 'The Sterling Police Department issues body worn cameras to all sworn members',
        'context_sha256': '21bbbc9777b42e654c5d0b6e3d468ea820461c5ac2b35acee75e2aa7ba661bde',
        'title': 'Sterling police describe their body-worn camera program.',
        'signals': ['Body-worn cameras'],
    },
    'greeley-flock': {
        'jurisdiction': 'Greeley',
        'jurisdiction_ref': 'us-co-place-0832155',
        'url': 'https://greeleyco.gov/police/technology/automated-license-plate-reader-program/',
        'agency': 'The Greeley Police Department uses Flock Safety cameras',
        'claim': 'The Greeley Police Department uses Flock Safety cameras',
        'combined_start': 'The Greeley Police Department uses Flock Safety cameras',
        'context_sha256': 'ab2246d40ce5044bac29226a5daae0ec2c51836376ede57095f76295d8cb1a80',
        'title': 'Greeley police describe their Flock Safety camera program.',
        'signals': ['Flock Safety'],
    },
    'boulder-flock': {
        'jurisdiction': 'Boulder',
        'jurisdiction_ref': 'us-co-place-0807850',
        'url': 'https://bouldercolorado.gov/services/flock-safety-cameras-and-boulder-police-department',
        'agency': 'Boulder Police Department selected Flock Safety',
        'claim': 'Boulder Police Department selected Flock Safety',
        'combined_start': 'Boulder Police Department selected Flock Safety',
        'context_sha256': 'c426a4371607293699ab501974dcbe707129282eb096a918cc4daa424e64589d',
        'title': 'Boulder police describe their selection of Flock Safety.',
        'signals': ['Flock Safety'],
    },
    'broomfield-flock': {
        'jurisdiction': 'Broomfield',
        'jurisdiction_ref': 'us-co-place-0809280',
        'url': 'https://www.broomfield.org/4545/Flock-Safety-License-Plate-Readers',
        'agency': 'The City and County of Broomfield (CCOB) Police Department',
        'claim': 'Police Department utilizes Flock Safety Automated License Plate Readers (ALPRs)',
        'combined_start': 'The City and County of Broomfield (CCOB) Police Department utilizes Flock Safety Automated License Plate Readers (ALPRs)',
        'context_sha256': 'd6ccfbbfb2b950fdf607e5663e16b448312a3ee11203dd522ec01b5e87980ed3',
        'title': 'Broomfield police describe their Flock Safety license-plate-reader program.',
        'signals': ['Flock Safety', 'Automated License Plate Readers'],
    },
    'boulder-uas': {
        'jurisdiction': 'Boulder',
        'jurisdiction_ref': 'us-co-place-0807850',
        'url': 'https://bouldercolorado.gov/services/unmanned-aircraft-system',
        'agency': 'The Boulder Police Department’s Unmanned Aircraft System (UAS) Team',
        'claim': 'The UAS team has 17 different types of Unmanned Vehicles (drones)',
        'section_id': 'section-26156',
        'context_sha256': '8d5d31dba2c4291ffc84aa6b8632be74347e954cfeedf82e295fc835e6a36d4c',
        'title': 'Boulder police describe their unmanned-aircraft program.',
        'signals': ['Police drone'],
    },
    'westminster-axon': {
        'jurisdiction': 'Westminster',
        'jurisdiction_ref': 'us-co-place-0883835',
        'url': 'https://www.westminsterco.gov/180/Engagement-Transparency',
        'agency': 'Westminster PD has fully implemented the BWC program',
        'claim': 'The planning, purchasing and deploying of 200 Axon Body 3 cameras',
        # This source's BWC text is in an initially collapsed, named tab panel.
        'tab_panel_id': 'tabb5b434ec-ec17-44c4-ada8-ee9c37d8d390_1',
        'context_sha256': '1d2c9c812d21bb40a12198a9067622b73b7adf287f7c78e84d2ec3af8844a55f',
        'title': 'Westminster police describe their Axon body-worn-camera program.',
        'signals': ['Axon', 'Body-worn cameras'],
    },
}


class VisibleNodes(HTMLParser):
    """Exclude explicit hidden markup in addition to non-content elements."""
    VOID = {'area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'param', 'source', 'track', 'wbr'}

    def __init__(self, tab_panel_id=None):
        super().__init__(convert_charrefs=True)
        self.stack = []
        self.hidden = 0
        self.nodes = []
        self.paths = []
        self.identifiers = Counter()
        self.valid_panels = Counter()
        self.tab_panel_id = tab_panel_id

    def handle_starttag(self, tag, attrs):
        tag = tag.lower()
        names = [name for name, _ in attrs]
        if len(names) != len(set(names)):
            raise ValueError('official page has ambiguous duplicate attributes')
        if tag in self.VOID:
            return
        if len(self.stack) >= 256:
            raise ValueError('official page markup nesting exceeds limit')
        attrs = dict(attrs)
        if attrs.get('id'):
            self.identifiers[attrs['id']] += 1
        trusted_tab = (tag == 'div' and self.tab_panel_id and attrs.get('id') == self.tab_panel_id
                     and {'tabbedWidget', 'cpTabPanel'}.issubset((attrs.get('class') or '').split())
                     and attrs.get('role') == 'region' and attrs.get('aria-labelledby') == 'tabBtn' + self.tab_panel_id[3:]
                     and 'hidden' not in attrs and not attrs.get('aria-hidden'))
        if trusted_tab:
            self.valid_panels[self.tab_panel_id] += 1
        valid_tab = trusted_tab and re.fullmatch(r'\s*display\s*:\s*none\s*;?\s*', attrs.get('style') or '', re.I)
        concealed = (tag in ('script', 'style', 'form', 'nav', 'footer', 'header', 'template')
                     or 'hidden' in attrs or (attrs.get('aria-hidden') or '').lower() == 'true'
                     or bool({'hidden', 'hide', 'd-none'}.intersection((attrs.get('class') or '').split()))
                     or (not valid_tab and bool(re.search(r'(?:display\s*:\s*none|visibility\s*:\s*hidden)', attrs.get('style') or '', re.I))))
        self.stack.append((tag, concealed, attrs.get('id')))
        self.hidden += int(concealed)

    def handle_startendtag(self, tag, attrs):
        pass

    def handle_endtag(self, tag):
        tag = tag.lower()
        for i in range(len(self.stack) - 1, -1, -1):
            if self.stack[i][0] == tag:
                self.hidden -= sum(int(x[1]) for x in self.stack[i:])
                del self.stack[i:]
                break

    def handle_data(self, data):
        if not self.hidden and data.strip() and len(self.nodes) < 20000:
            self.nodes.append(' '.join(data.split())[:20000])
            self.paths.append(tuple((tag, identifier) for tag, _, identifier in self.stack))


def locate(research, seed, body):
    nodes = research.visible_text(body)
    agencies = [n for n in nodes if seed['agency'] in n]
    matches = [i for i, n in enumerate(nodes) if seed['claim'] in n]
    parser = VisibleNodes(seed.get('tab_panel_id'))
    parser.feed(body.decode('utf8', 'replace'))
    visible_agencies = [i for i, n in enumerate(parser.nodes) if seed['agency'] in n]
    visible_claims = [i for i, n in enumerate(parser.nodes) if seed['claim'] in n]
    if len(agencies) != 1 or len(matches) != 1 or len(visible_agencies) != 1 or len(visible_claims) != 1:
        raise ValueError('official page agency or technology text not uniquely established')
    agency_i, claim_i = visible_agencies[0], visible_claims[0]
    first, last = sorted((agency_i, claim_i))
    context = '\n'.join(parser.nodes[first:last + 1]).encode('utf8')
    if research.sha(context) != seed['context_sha256']:
        raise ValueError('official agency technology passage changed')
    if seed.get('combined_start'):
        if agency_i != claim_i or not parser.nodes[claim_i].startswith(seed['combined_start']):
            raise ValueError('official agency and technology are not in the same statement')
    else:
        scope = seed.get('section_id') or seed.get('tab_panel_id')
        if (not scope or parser.identifiers[scope] != 1 or
                not all(('div', scope) in parser.paths[i] for i in (agency_i, claim_i)) or
                not parser.nodes[claim_i].startswith(seed['claim'])):
            raise ValueError('official agency and technology are not in the same scoped section')
        if seed.get('section_id'):
            if (not parser.paths[agency_i] or parser.paths[agency_i][-1][0] != 'h2' or
                    parser.nodes[agency_i] != seed['agency'] or agency_i >= claim_i or
                    any(any(tag in ('h1', 'h2', 'h3', 'h4', 'h5', 'h6') for tag, _ in parser.paths[i])
                        for i in range(agency_i + 1, claim_i))):
                raise ValueError('official section heading does not bind the technology passage')
        elif (parser.valid_panels[scope] != 1 or agency_i != claim_i + 1 or
              not parser.nodes[agency_i].startswith(seed['agency']) or
              not parser.paths[agency_i] or parser.paths[agency_i][-1][0] != 'p' or
              not parser.paths[claim_i] or parser.paths[claim_i][-1][0] != 'p'):
            raise ValueError('official tab statements are not adjacent')
    return matches[0]


def capture(research, data_dir, db):
    budget = research.DownloadBudget(12 * 1024 * 1024)
    stored = 0
    for name, seed in SEEDS.items():
        url, ref = seed['url'], seed['jurisdiction_ref']
        host = urlsplit(url).hostname
        try:
            if host.removeprefix('www.') not in research.discovery_hosts(data_dir, db, ref):
                raise ValueError('official page not bound to directory jurisdiction')
            rules, observations = research.get_robots(host, budget=budget)
            research.archive(data_dir, observations)
            if not research.allowed(rules, url):
                raise ValueError('official page disallowed')
            observations = research.get(url, {host}, maximum=research.MAX_PAGE_BYTES, redirects=0, timeout=30, budget=budget)
            research.archive(data_dir, observations)
            receipt, body = observations[-1]
            if (not research.receipt_valid(receipt, url, research.sha(body)) or
                    'html' not in receipt.get('content_type', '').lower()):
                raise ValueError('official page unavailable')
            try:
                locate(research, seed, body)
            except (ValueError, UnicodeError):
                # A changed successful source should not keep an obsolete claim alive.
                with db:
                    old = db.execute('SELECT receipt_json,state FROM official_context_pages WHERE seed_id=?', (name,)).fetchone()
                    if old and old[1] == 'active':
                        previous = json.loads(old[0])
                        db.execute('INSERT INTO official_context_history VALUES (?,?,?,?,?)',
                                   (name, previous['sha256'], old[0], 'withdrawn_after_source_change', research.utcnow()))
                    if old:
                        db.execute('UPDATE official_context_pages SET receipt_json=?,updated_at=?,state=? WHERE seed_id=?',
                                   (json.dumps(receipt), research.utcnow(), 'withdrawn', name))
                raise
            with db:
                old = db.execute('SELECT receipt_json,state FROM official_context_pages WHERE seed_id=?', (name,)).fetchone()
                if old and json.loads(old[0])['sha256'] != receipt['sha256']:
                    db.execute('INSERT INTO official_context_history VALUES (?,?,?,?,?)',
                               (name, json.loads(old[0])['sha256'], old[0], 'reinstated' if old[1] == 'withdrawn' else 'superseded', research.utcnow()))
                db.execute('INSERT OR REPLACE INTO official_context_pages (seed_id,receipt_json,updated_at,state) VALUES (?,?,?,?)',
                           (name, json.dumps(receipt), research.utcnow(), 'active'))
            stored += 1
        except (OSError, UnicodeError, ValueError, RuntimeError) as error:
            print(f'{name}: official context capture incomplete ({type(error).__name__})', flush=True)
    print(f'Captured {stored}/{len(SEEDS)} official technology context pages', flush=True)
    return stored


def materialize(research, data_dir, db, receipts, name, raw, updated, state='active'):
    seed = SEEDS.get(name)
    if not seed:
        raise ValueError('unknown official context seed')
    ref, url = seed['jurisdiction_ref'], seed['url']
    if urlsplit(url).hostname.removeprefix('www.') not in research.discovery_hosts(data_dir, db, ref):
        raise ValueError('official context source no longer binds to jurisdiction')
    receipt = json.loads(raw)
    digest = receipt['sha256']
    if (url, digest) not in receipts or not research.receipt_valid(receipt, url, digest):
        raise ValueError('missing official context source receipt')
    if 'html' not in receipt.get('content_type', '').lower():
        raise ValueError('official context source is not HTML')
    body = research.source_blob(data_dir, digest)
    if state not in ('active', 'withdrawn'):
        raise ValueError('invalid official context state')
    if state == 'withdrawn':
        old = db.execute('''SELECT receipt_json FROM official_context_history WHERE seed_id=?
            AND event='withdrawn_after_source_change' ORDER BY event_at DESC LIMIT 1''', (name,)).fetchone()
        if not old:
            raise ValueError('withdrawal has no prior official context')
        previous = json.loads(old[0])
        previous_digest = previous['sha256']
        if ((url, previous_digest) not in receipts or not research.receipt_valid(previous, url, previous_digest) or
                datetime.fromisoformat(receipt['retrieved_at']) < datetime.fromisoformat(previous['retrieved_at'])):
            raise ValueError('withdrawal previous source receipt is invalid')
        locate(research, seed, research.source_blob(data_dir, previous_digest))
        try:
            locate(research, seed, body)
        except (ValueError, UnicodeError):
            pass
        else:
            raise ValueError('withdrawal source still matches official context')
        title, signals, locator = ('Official agency page changed; prior technology context withdrawn.',
                                   ['Source changed'], 'Source response (withdrawal)')
    else:
        node = locate(research, seed, body)
        title, signals, locator = seed['title'], seed['signals'], f'HTML text node {node + 1}'
    return {'id': 'co-official-context-' + name, 'kind': 'official_page_context',
            'context_state': state,
            'confidence': 'unconfirmed_source_reference', 'jurisdiction_ref': ref,
            'jurisdiction': seed['jurisdiction'],
            'file': 'Official police technology page', 'title': title, 'signals': signals,
            'source_status': '', 'source_url': url, 'source_sha256': digest,
            'locator': locator, 'captured_at': receipt['retrieved_at'],
            'last_screened_at': updated}
