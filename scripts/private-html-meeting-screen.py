#!/usr/bin/env python3
"""Offline private-only screening of captured, directory-bound HTML meeting pages.

No networking, no research SQLite database, no publication. A source match is
only a lead: identity of the responsible body and complete text coverage remain
unverified. CSS external to the markup cannot be interpreted by this parser.
"""
import argparse
import importlib.util
import json
import os
from html.parser import HTMLParser
from pathlib import Path
import re
import sqlite3
import signal
import sys
import time
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('research', ROOT / 'scripts/colorado-research.py')
r = importlib.util.module_from_spec(spec); spec.loader.exec_module(r)
VOID = frozenset('area base br col embed hr img input link meta param source track wbr'.split())
EXCLUDED = frozenset('script style noscript template form nav footer header del s strike'.split())
HIDDEN = re.compile(r'(?:display\s*:\s*none|visibility\s*:\s*(?:hidden|collapse)|opacity\s*:\s*0(?:\.0+)?(?=\s*(?:;|!|$)))', re.I)
def inline_hidden(style):
    # A linear scan avoids quadratic behaviour on many unterminated comments.
    style=style or ''
    if len(style)>8192:return True
    parts=[];position=0
    while True:
        start=style.find('/*',position)
        if start<0:
            parts.append(style[position:]);break
        parts.append(style[position:start]);end=style.find('*/',start+2)
        if end<0:return True
        position=end+2
    return bool(HIDDEN.search(''.join(parts)))

def hidden_attributes(attrs):
    # Duplicate attributes are common in malformed markup; any hiding wins.
    return any(k=='hidden' or (k=='aria-hidden' and (v or '').strip().lower()=='true')
               or (k=='style' and inline_hidden(v)) for k,v in attrs)

MAX_DEPTH=128
MAX_EVENTS=30000
MAX_NODES=2000
MAX_NODE_CHARS=20000
MAX_TOTAL_TEXT=500000
MAX_RECEIPTS=10000
MAX_RECEIPT_BYTES=32*1024*1024
MAX_MATCHES=100
MAX_OUTPUT=8*1024*1024

class CandidateText(HTMLParser):
    """Conservative inline exclusions; stylesheets may still hide text."""
    def __init__(self):
        super().__init__(convert_charrefs=True); self.stack=[]; self.nodes=[]; self.events=0; self.total=0
    def tick(self):
        self.events+=1
        if self.events>MAX_EVENTS:raise ValueError('HTML event limit exceeded')
    def handle_starttag(self, tag, attrs):
        self.tick()
        if tag not in VOID and len(self.stack)>=MAX_DEPTH:raise ValueError('HTML nesting limit exceeded')
        parent=self.stack[-1][1] if self.stack else False
        hide=parent or tag in EXCLUDED or hidden_attributes(attrs)
        if tag not in VOID: self.stack.append((tag,hide))
    def handle_startendtag(self, tag, attrs):
        # HTML self-closing syntax does not close non-void elements in browsers.
        self.handle_starttag(tag,attrs)
    def handle_endtag(self, tag):
        self.tick()
        for i in range(len(self.stack)-1,-1,-1):
            if self.stack[i][0]==tag:
                del self.stack[i:]; break
    def handle_data(self, data):
        self.tick()
        if (not self.stack or not self.stack[-1][1]) and data.strip():
            self.total+=len(data)
            if len(self.nodes)>=MAX_NODES or len(data)>MAX_NODE_CHARS or self.total>MAX_TOTAL_TEXT:raise ValueError('HTML text limit exceeded')
            self.nodes.append(' '.join(data.split()))

class IndexLinks(r.Links):
    """Track visibility alongside the existing parser's stable anchor ordinals."""
    def __init__(self):
        super().__init__(); self.stack=[]; self.visibility=[]; self.current_visible=False; self.current_text_seen=False; self.events=0
    def tick(self):
        self.events+=1
        if self.events>MAX_EVENTS:raise ValueError('HTML event limit exceeded')
    def handle_starttag(self, tag, attrs):
        self.tick()
        if tag not in VOID and len(self.stack)>=MAX_DEPTH:raise ValueError('HTML nesting limit exceeded')
        attrs_dict=dict(attrs); parent=self.stack[-1][1] if self.stack else False
        if tag in ('a','base','iframe') and len(attrs_dict)!=len(attrs):raise ValueError('ambiguous index link attributes')
        hide=parent or tag in EXCLUDED.difference({'nav','header','footer','form'}) or hidden_attributes(attrs)
        before=len(self.links); super().handle_starttag(tag,attrs)
        if tag=='a' and 'href' in attrs_dict:
            self.current_visible=not hide; self.current_text_seen=False
        if len(self.links)>before: self.visibility.append(not hide)
        if tag not in VOID:self.stack.append((tag,hide))
    def handle_startendtag(self, tag, attrs):
        # Browsers ignore the slash on non-void HTML elements.
        self.handle_starttag(tag,attrs)
    def handle_data(self, data):
        self.tick()
        if self.current is not None and len(self.current[1])+min(len(data),500)>2000:raise ValueError('index anchor text limit exceeded')
        if self.current is not None and data.strip() and (not self.stack or not self.stack[-1][1]):self.current_text_seen=True
        super().handle_data(data)
    def handle_endtag(self, tag):
        self.tick()
        before=len(self.links); super().handle_endtag(tag)
        if len(self.links)>before:self.visibility.append(self.current_visible and self.current_text_seen)
        for i in range(len(self.stack)-1,-1,-1):
            if self.stack[i][0]==tag:
                del self.stack[i:];break

def safe_url(value, hosts):
    p=urlsplit(value)
    if (p.scheme!='https' or not p.hostname or p.username or p.password or p.port not in (None,443)
        or p.hostname.removeprefix('www.') not in hosts or p.query or p.fragment or len(value)>2048):
        raise ValueError('URL outside the captured directory host')

def receipts(root):
    output={}; total=0
    # Stream directory entries; Path.iterdir may eagerly allocate all names.
    with os.scandir(root/'observations') as entries:
        for number,entry in enumerate(entries,1):
            if number>MAX_RECEIPTS:raise ValueError('receipt count limit exceeded')
            path=Path(entry.path)
            try:
                raw=r.read_blob(path,65536);total+=len(raw)
                if total>MAX_RECEIPT_BYTES:raise OverflowError('receipt byte limit exceeded')
                value=json.loads(raw); digest=r.sha(raw)
                if path.name != digest+'.json': continue
                if not r.receipt_valid(value,value['url'],value['sha256']): continue
                output.setdefault((value['url'],value['sha256']),[]).append((digest,value))
            except (OSError,ValueError,KeyError,TypeError): continue
    return output


def analyze(root, discovery, entry, receipt_map):
    ref=entry['jurisdiction_ref']; directory={x['jurisdiction_ref']:x for x in json.loads(r.DIRECTORY.read_text())['municipalities'] if x.get('website_url')}
    lead=directory.get(ref); found=[x for x in discovery if x.get('jurisdiction_ref')==ref]
    if not lead or len(found)!=1 or found[0].get('state')!='directory_site_checked' or found[0].get('website_lead')!=lead['website_url']:
        raise ValueError('directory website identity missing')
    db=sqlite3.connect(':memory:')
    try:
        db.execute('CREATE TABLE discovery (jurisdiction_ref TEXT, source_url TEXT, result_json TEXT)')
        db.execute('INSERT INTO discovery VALUES (?,?,?)',(ref,lead['website_url'],json.dumps(found[0])))
        hosts=r.discovery_hosts(root,db,ref)
    finally: db.close()
    source=entry['source_anchor']; target=source['url']; index=source['index_url']; index_sha=source['index_sha256']; doc_sha=entry['source_sha256']; anchor=source['anchor_index']
    safe_url(index,hosts); safe_url(target,hosts)
    if entry.get('state')!='screened' or type(anchor) is not int or anchor<0: raise ValueError('invalid candidate')
    ir=[x for x in receipt_map.get((index,index_sha),[]) if isinstance(x[1].get('content_type'),str) and 'html' in x[1]['content_type'].lower()]
    dr=[x for x in receipt_map.get((target,doc_sha),[]) if isinstance(x[1].get('content_type'),str) and 'html' in x[1]['content_type'].lower()]
    if not ir or not dr: raise ValueError('HTML index or page receipt unavailable')
    index_bytes=r.source_blob(root,index_sha)
    visibility=IndexLinks();visibility.feed(index_bytes.decode('utf8','replace'))
    links,base=r.page_links(index_bytes,index)
    if len(visibility.visibility)!=len(links):raise ValueError('ambiguous source index markup')
    if anchor>=len(links) or not visibility.visibility[anchor] or r.resolve_page_link(index,base,links[anchor][0])!=target: raise ValueError('index anchor does not establish target')
    body=r.source_blob(root,doc_sha); text=CandidateText();text.feed(body.decode('utf8','strict'));matches=[]
    for n,node in enumerate(text.nodes):
        labels=r.labels_at_quote(list(r.RULE_LABELS),node)
        if labels:
            if len(matches)>=MAX_MATCHES:raise ValueError('match count limit exceeded')
            matches.append({'text_node':n,'signals':labels,'text':node})
    return {'jurisdiction_ref':ref,'directory_url':lead['website_url'],'discovery':found[0],
            'index_url':index,'index_sha256':index_sha,'index_receipt_sha256':min(d for d,_ in ir),
            'index_anchor':anchor,'url':target,'sha256':doc_sha,'receipt_sha256':min(d for d,_ in dr),
            'text_nodes':len(text.nodes),'matches':matches}

def bounded_json(payload):
    # Bound the allocation while serializing repeated shared source metadata.
    output=bytearray()
    for chunk in json.JSONEncoder(separators=(',',':')).iterencode(payload):
        encoded=chunk.encode('utf-8')
        if len(output)+len(encoded)+1>MAX_OUTPUT:raise ValueError('output byte limit exceeded')
        output.extend(encoded)
    output.extend(b'\n')
    return output

class DeadlineExpired(BaseException):
    pass

def deadline_handler(signum,frame):
    raise DeadlineExpired()

def atomic_output(out,encoded,deadline=None):
    """Publish one fresh private result only if the CLI deadline is satisfied."""
    created=False;tmp=out/'.results.tmp';final=out/'results.json'
    try:
        if deadline is None:
            os.mkdir(out,0o700);created=True
        else:
            previous=signal.pthread_sigmask(signal.SIG_BLOCK,{signal.SIGALRM})
            try:os.mkdir(out,0o700);created=True
            finally:signal.pthread_sigmask(signal.SIG_SETMASK,previous)
        fd=os.open(tmp,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
        with os.fdopen(fd,'wb') as f:f.write(encoded);f.flush();os.fsync(f.fileno())
        if deadline is None:
            os.replace(tmp,final)
        else:
            previous=signal.pthread_sigmask(signal.SIG_BLOCK,{signal.SIGALRM})
            try:
                if time.monotonic()>=deadline or signal.SIGALRM in signal.sigpending():raise DeadlineExpired()
                os.replace(tmp,final)
                if time.monotonic()>=deadline or signal.SIGALRM in signal.sigpending():raise DeadlineExpired()
                signal.setitimer(signal.ITIMER_REAL,0)
                if signal.SIGALRM in signal.sigpending():raise DeadlineExpired()
            finally:
                # Consume any pending timeout before unmasking it.
                signal.setitimer(signal.ITIMER_REAL,0)
                if signal.SIGALRM in signal.sigpending():signal.sigwait({signal.SIGALRM})
                signal.pthread_sigmask(signal.SIG_SETMASK,previous)
    except BaseException:
        if created:
            tmp.unlink(missing_ok=True);final.unlink(missing_ok=True);out.rmdir()
        raise

def main(deadline=None):
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--archive',type=Path,required=True);parser.add_argument('--output-name',default='html-meeting-offline-screen');a=parser.parse_args()
    root=a.archive
    if (not root.is_absolute() or root.is_symlink() or root.resolve()!=root or not root.is_dir() or root.stat().st_uid!=os.geteuid() or root.stat().st_mode&0o077):
        parser.error('archive must be an owner-only real absolute directory')
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,63}',a.output_name): parser.error('invalid output directory name')
    for name in ('content','observations'):
        p=root/name
        if p.is_symlink() or not p.is_dir():parser.error('invalid archive child')
    discovery=json.loads(r.read_blob(root/'private-results.json',8*1024*1024))['discovery']
    entries=json.loads(r.read_blob(root/'private-html-screen-results.json',8*1024*1024))['results']
    if not isinstance(entries,list) or len(entries)>100:parser.error('at most 100 captured pages allowed')
    lookup=receipts(root); results=[];errors=[]
    for entry in entries:
        try:results.append(analyze(root,discovery,entry,lookup))
        except (OSError,ValueError,KeyError,TypeError,UnicodeError) as e:errors.append({'jurisdiction_ref':entry.get('jurisdiction_ref'),'reason':str(e)[:160]})
    # Fresh owner-only directory; no ordinary exporter consumes this JSON.
    out=root/a.output_name
    payload={'schema_version':'pnull-private-html-recon-v1','notice':'Private leads only. Responsible body, all document parts, stylesheet visibility, and complete coverage are not established.', 'results':results,'errors':errors}
    encoded=bounded_json(payload)
    atomic_output(out,encoded,deadline)
    print(f'Validated {len(results)} captured pages; {len(errors)} withheld; {sum(len(x["matches"]) for x in results)} preliminary matches.')
def cli():
    if sys.platform!='linux' or not hasattr(signal,'pthread_sigmask'):
        raise SystemExit('private HTML CLI requires Linux signal deadlines')
    prior=signal.signal(signal.SIGALRM,deadline_handler)
    deadline=time.monotonic()+60.0
    signal.setitimer(signal.ITIMER_REAL,60.0)
    try:
        main(deadline)
    except DeadlineExpired:
        raise SystemExit('private HTML screening timed out; output withheld')
    finally:
        signal.setitimer(signal.ITIMER_REAL,0);signal.signal(signal.SIGALRM,prior)
if __name__=='__main__':cli()
