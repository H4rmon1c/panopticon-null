#!/usr/bin/env python3
"""Bounded source-linked CivicClerk list and published-file reconnaissance.

Creates a fresh private archive, not a research database or a publication.
Vendor metadata is a lead and does not establish a responsible body or vote.
"""
import argparse
from datetime import date
from html.parser import HTMLParser
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import sqlite3
import sys
import time
from urllib.parse import quote,urlsplit,urlunsplit

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('research',ROOT/'scripts/colorado-research.py');r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
spec=importlib.util.spec_from_file_location('private_html',ROOT/'scripts/private-html-meeting-screen.py');h=importlib.util.module_from_spec(spec);spec.loader.exec_module(h)
ASSETS={'/assets/apis-WKEMuBZZ.js':'e0b03d739157d2cb40bd40ade958dd75f4ddea4c27787239d25dbc27f96d92fe',
        '/assets/actions-CMJ3lQwM.js':'1070b512225da0f7b6fd5182b0d60f8cf5bb8dcce2e164c307c1fe002e478ab8'}
UTILITY_PATH='/assets/utils-C7X9g4QK.js'
UTILITY_SHA='9e1d60411447198f1964447303ec0c73d98170dc91c0f81474f910d31d00b1fd'
MAX_TENANTS=3;MAX_EVENTS=100;MAX_FILES=20;MAX_JSON=2*1024*1024;MAX_PDF=8*1024*1024

class Assets(HTMLParser):
    def __init__(self):super().__init__();self.paths=[];self.events=0
    def handle_starttag(self,tag,attrs):
        self.events+=1
        if self.events>30000:raise ValueError('portal markup event limit exceeded')
        if len(dict(attrs))!=len(attrs):raise ValueError('ambiguous portal asset attributes')
        a=dict(attrs)
        if tag=='script' and 'src' in a:self.paths.append(a['src'])
        if tag=='link' and a.get('rel')=='modulepreload' and 'href' in a:self.paths.append(a['href'])
        if len(self.paths)>10000:raise ValueError('portal asset count exceeded')

def authority_url(url,hosts):
    p=urlsplit(url)
    if p.query and not re.fullmatch(r'EID=[0-9]{1,12}',p.query):raise ValueError('unsupported government index query')
    h.safe_url(urlunsplit((p.scheme,p.netloc,p.path,'',p.fragment)),hosts)

def choose_receipt(lookup,url,digest,mime=None):
    rows=lookup.get((url,digest),[])
    if mime:rows=[x for x in rows if isinstance(x[1].get('content_type'),str) and mime in x[1]['content_type'].lower()]
    if not rows:raise ValueError('capture receipt missing')
    return min(rows,key=lambda x:x[0])

def bind_source(root,entry,lookup):
    ref=entry['jurisdiction_ref'];directory={x['jurisdiction_ref']:x for x in json.loads(r.DIRECTORY.read_text())['municipalities'] if x.get('website_url')}
    lead=directory.get(ref);origin=entry['directory'];source=entry['source_anchor'];portal=entry['portal_url']
    if not isinstance(origin,dict) or not isinstance(source,dict) or not isinstance(portal,str):raise ValueError('malformed source seed')
    if not lead or origin.get('jurisdiction_ref')!=ref or origin.get('state')!='directory_site_checked' or origin.get('website_lead')!=lead['website_url'] or entry.get('state')!='html_screened' or source.get('url')!=portal:
        raise ValueError('directory binding differs')
    db=sqlite3.connect(':memory:')
    try:
        db.execute('CREATE TABLE discovery (jurisdiction_ref TEXT,source_url TEXT,result_json TEXT)');db.execute('INSERT INTO discovery VALUES (?,?,?)',(ref,lead['website_url'],json.dumps(origin)));hosts=r.discovery_hosts(root,db,ref)
    finally:db.close()
    index=source['index_url'];authority_url(index,hosts);anchor=source['anchor_index'];idxsha=source['index_sha256'];idxreceipt=choose_receipt(lookup,index,idxsha,'html');idxbody=r.source_blob(root,idxsha)
    parsed=h.IndexLinks();parsed.feed(idxbody.decode('utf8','replace'));links,base=r.page_links(idxbody,index)
    if type(anchor) is not int or anchor<0 or anchor>=len(links) or len(parsed.visibility)!=len(links) or not parsed.visibility[anchor] or r.resolve_page_link(index,base,links[anchor][0])!=portal:raise ValueError('official link anchor differs')
    p=urlsplit(portal)
    if p.scheme!='https' or p.port not in (None,443) or p.username or p.password or p.query or p.fragment or not p.hostname or not re.fullmatch(r'[a-z0-9-]{1,80}\.portal\.civicclerk\.com',p.hostname) or not (p.path=='/' or re.fullmatch(r'/event/[0-9]{1,9}/(?:files|overview)',p.path)):
        raise ValueError('unsupported portal URL')
    portalsha=entry['portal_sha256'];portalreceipt=choose_receipt(lookup,portal,portalsha,'html');shell=r.source_blob(root,portalsha);html=Assets();html.feed(shell.decode('utf8','strict'))
    assetproof=[]
    for path,digest in ASSETS.items():
        if html.paths.count(path)!=1:raise ValueError('portal asset reference differs')
        url='https://'+p.hostname+path;proof=choose_receipt(lookup,url,digest,'javascript');body=r.source_blob(root,digest,2*1024*1024);assetproof.append((url,digest,proof,body))
    proof=[(index,idxsha,idxreceipt,idxbody),(portal,portalsha,portalreceipt,shell),*assetproof]
    safe_origin={'jurisdiction_ref':ref,'website_lead':lead['website_url'],'state':'directory_site_checked'}
    original_home=r.normalized_home(lead['website_url']);original_host=urlsplit(original_home).hostname.removeprefix('www.')
    index_host=urlsplit(index).hostname.removeprefix('www.')
    if index_host!=original_host:
        current=original_home;redirects=origin.get('canonical_redirects');final=origin.get('canonical_receipt')
        if not isinstance(redirects,list) or not 1<=len(redirects)<=2 or not isinstance(final,dict):raise ValueError('canonical redirect chain missing')
        for redirect in redirects:
            target=redirect.get('redirect_target')
            if not isinstance(target,str) or not r.verified_redirect_receipt(root,redirect,current,target):raise ValueError('canonical redirect receipt differs')
            proof.append((current,redirect['sha256'],(None,redirect),r.source_blob(root,redirect['sha256'])));current=target
        if origin.get('canonical_url')!=current or urlsplit(current).hostname.removeprefix('www.')!=index_host:raise ValueError('canonical destination differs')
        finalrows=[x for x in lookup.get((current,final.get('sha256')),[]) if x[1]==final and isinstance(x[1].get('content_type'),str) and 'html' in x[1]['content_type'].lower()]
        if not finalrows:raise ValueError('canonical homepage receipt differs')
        finalproof=min(finalrows,key=lambda x:x[0])
        proof.append((current,final['sha256'],finalproof,r.source_blob(root,final['sha256'])));safe_origin['canonical_url']=current
    return {'jurisdiction_ref':ref,'portal_url':portal,'tenant':p.hostname.split('.')[0],'source_anchor':source,'directory':safe_origin,'capture_proofs':proof}

def bind_group(root,ref,entries,lookup):
    """Validate every seed; never silently merge conflicting tenant bindings."""
    valid=[];errors=[]
    for entry in entries:
        try:valid.append(bind_source(root,entry,lookup))
        except (OSError,ValueError,TypeError,KeyError,UnicodeError) as e:errors.append({'jurisdiction_ref':ref,'reason':str(e)[:150]})
    if len({x['tenant'] for x in valid})>1:
        errors.append({'jurisdiction_ref':ref,'reason':'conflicting source-linked CivicClerk tenants'});return None,errors
    if not valid:return None,errors
    selected=valid[0];selected['additional_same_tenant_sources_skipped']=len(valid)-1
    return selected,errors

def unique_object(pairs):
    result={}
    for key,value in pairs:
        if key in result:raise ValueError('duplicate JSON key')
        result[key]=value
    return result

def summarize(body,cutoff):
    obj=json.loads(body,object_pairs_hook=unique_object);items=obj.get('items') if isinstance(obj,dict) else None
    if not isinstance(items,list) or len(items)>MAX_EVENTS or type(obj.get('hasMore')) is not bool:raise ValueError('unexpected event list shape')
    events=[];seen_events=set()
    for n,e in enumerate(items):
        if not isinstance(e,dict) or type(e.get('id')) is not int or not 0<e['id']<1000000000:raise ValueError('invalid event ID')
        if e['id'] in seen_events:raise ValueError('duplicate event ID')
        seen_events.add(e['id'])
        stamp=e.get('eventDate');name=e.get('eventName');category=e.get('categoryName')
        raw_files=e.get('publishedFiles');file_state='not_provided' if raw_files is None else 'provided'
        if raw_files is not None and not isinstance(raw_files,list):raise ValueError('invalid publishedFiles type')
        files=[] if raw_files is None else raw_files
        if not isinstance(stamp,str) or not re.match(r'^\d{4}-\d{2}-\d{2}T',stamp):raise ValueError('invalid event date')
        if date.fromisoformat(stamp[:10])>=cutoff:raise ValueError('non-past event date')
        if not isinstance(name,str) or len(name)>500 or not isinstance(category,str) or len(category)>500 or not isinstance(files,list) or len(files)>MAX_FILES:raise ValueError('invalid event metadata')
        saved=[];seen_files=set()
        for i,f in enumerate(files):
            if not isinstance(f,dict) or type(f.get('fileId')) is not int or not 0<f['fileId']<1000000000 or not isinstance(f.get('type'),str) or len(f['type'])>80:raise ValueError('invalid published file')
            if f['fileId'] in seen_files:raise ValueError('duplicate file ID')
            seen_files.add(f['fileId'])
            filetype=f.get('fileType');mode=e.get('cpMediaOnly');stream=f.get('streamUrl');wanted={'agenda':1,'agenda packet':2,'minutes':4}.get(f['type'].strip().lower())
            if filetype is not None and type(filetype) is not int:raise ValueError('invalid fileType')
            if mode is not None and type(mode) is not bool:raise ValueError('invalid cpMediaOnly')
            if stream is not None and not isinstance(stream,str):raise ValueError('invalid streamUrl')
            if isinstance(stream,str) and len(stream)>4096:raise ValueError('streamUrl too long')
            if wanted is None or mode is True or (isinstance(stream,str) and bool(stream)):eligible=False
            elif mode is None or filetype is None or 'streamUrl' not in f:eligible=None
            else:eligible=(wanted==filetype)
            saved.append({'file_id':f['fileId'],'type':f['type'],'file_type':filetype if type(filetype) is int else None,'file_endpoint_eligible':eligible,'json_path':f'$.items[{n}].publishedFiles[{i}]'})
        events.append({'event_id':e['id'],'event_name':name,'category':category,'date':stamp[:10],'json_path':f'$.items[{n}]','published_files_state':file_state,'published_files':saved})
    return {'items_count':len(items),'has_more':obj['hasMore'],'events':events}

def list_url(tenant,cutoff):
    if not re.fullmatch(r'[a-z0-9-]{1,80}',tenant):raise ValueError('invalid tenant')
    path="/Events/GetEventsList(fromDate='',toDate='"+cutoff.isoformat()+"',categoryIds=[],skip=0,sortDescending=true,cursorId=0)"
    host=tenant+'.api.civicclerk.com';return host,'https://'+host+'/v1'+quote(path,safe="/(),=:'")

def capture_file(out,item,row,rules,host,budget,ignore_robots):
    try:
        # Only source-listed, published agenda/minutes IDs and the
        # portal's exact pinned utility route are eligible.
        choices=[(event,file) for event in row['metadata']['events'] for file in event['published_files'] if file['file_endpoint_eligible']]
        choices.sort(key=lambda pair:(pair[1]['type'].lower()!='minutes',-date.fromisoformat(pair[0]['date']).toordinal(),pair[1]['file_id']))
        unknown=any(e['published_files_state']=='not_provided' or any(f['file_endpoint_eligible'] is None for f in e['published_files']) for e in row['metadata']['events'])
        probe={'state':'file_metadata_incomplete' if unknown else 'no_supported_file_in_bounded_list'}
        if choices:
            event,file=choices[0];portal_host=urlsplit(item['portal_url']).hostname
            shell=next(proof[3] for proof in item['capture_proofs'] if proof[0]==item['portal_url']);assets=Assets();assets.feed(shell.decode('utf8','strict'))
            if assets.paths.count(UTILITY_PATH)!=1:raise ValueError('utility asset is not bound to portal shell')
            portal_rules,portal_obs=r.get_robots(portal_host,{portal_host},budget);r.archive(out,portal_obs)
            utility_url='https://'+portal_host+UTILITY_PATH;utility_allowed=r.allowed(portal_rules,utility_url)
            if not utility_allowed and not ignore_robots:raise ValueError('robots disallows utility request')
            utility_capture=r.get(utility_url,{portal_host},maximum=MAX_JSON,timeout=12,redirects=0,stop_at_redirect=True,budget=budget);r.archive(out,utility_capture);utility_meta,utility_body=utility_capture[-1]
            if utility_meta['status']!=200 or utility_meta['sha256']!=UTILITY_SHA:raise ValueError('utility asset differs')
            doc_url='https://'+host+'/v1/Meetings/GetMeetingFileStream(fileId='+str(file['file_id'])+',plainText=false)';doc_allowed=r.allowed(rules,doc_url)
            if not doc_allowed and not ignore_robots:raise ValueError('robots disallows file request')
            doc_capture=r.get(doc_url,{host},maximum=MAX_PDF,timeout=20,redirects=0,stop_at_redirect=True,budget=budget);r.archive(out,doc_capture);doc_meta,doc_body=doc_capture[-1]
            probe={'state':'http_error','event_id':event['event_id'],'event_json_path':event['json_path'],'file_id':file['file_id'],'file_type':file['type'],'file_json_path':file['json_path'],'url':doc_url,'robots_allowed':doc_allowed,'robots_override':not doc_allowed and ignore_robots,'status':doc_meta['status'],'sha256':doc_meta['sha256'],'receipt_sha256':r.sha((json.dumps(doc_meta,sort_keys=True,separators=(',',':'))+'\n').encode()),'utility_url':utility_url,'utility_robots_allowed':utility_allowed,'utility_robots_override':not utility_allowed and ignore_robots,'utility_robots_receipts':[r.sha((json.dumps(meta,sort_keys=True,separators=(',',':'))+'\n').encode()) for meta,_ in portal_obs],'utility_sha256':utility_meta['sha256'],'utility_receipt_sha256':r.sha((json.dumps(utility_meta,sort_keys=True,separators=(',',':'))+'\n').encode())}
            if doc_meta['status']==200 and doc_body.startswith(b'%PDF-'):probe['state']='pdf_captured_unscreened'
            elif doc_meta['status']==200:probe['state']='unsupported_content'
        return probe
    except r.RobotsDenied as e:
        r.archive(out,e.observations);return {'state':'error','reason':str(e)[:150]}
    except (OSError,ValueError,TypeError,KeyError,RuntimeError,StopIteration) as e:
        return {'state':'error','reason':str(e)[:150]}

def main(deadline):
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--source-archive',type=Path,required=True);p.add_argument('--output-parent',type=Path,required=True);p.add_argument('--output-name',required=True);p.add_argument('--seed-name',default='private-probe-results-v3.json');p.add_argument('--before',required=True,type=date.fromisoformat);p.add_argument('--limit',type=int,default=3);p.add_argument('--jurisdiction',action='append',required=True,help='explicit municipal jurisdiction reference; repeat up to the limit');p.add_argument('--ignore-robots',action='store_true');p.add_argument('--fetch-files',action='store_true',help='capture at most one explicit published PDF per tenant');a=p.parse_args()
    for path in (a.source_archive,a.output_parent):
        if not path.is_absolute() or path.is_symlink() or path.resolve()!=path or not path.is_dir() or path.stat().st_uid!=os.geteuid() or path.stat().st_mode&0o077:p.error('archives must be real owner-only absolute directories')
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,63}',a.output_name) or not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,63}\.json',a.seed_name) or not 1<=a.limit<=MAX_TENANTS or not 1<=len(a.jurisdiction)<=a.limit or len(set(a.jurisdiction))!=len(a.jurisdiction) or any(not re.fullmatch(r'us-co-place-[0-9]{7}',ref) for ref in a.jurisdiction) or a.before>date.today():p.error('invalid output, seed name, selection, limit, or cutoff')
    for name in ('content','observations'):
        path=a.source_archive/name
        if path.is_symlink() or not path.is_dir():p.error('invalid source archive child')
    data=json.loads(r.read_blob(a.source_archive/a.seed_name,2*1024*1024));entries=data['results']
    if not isinstance(entries,list) or len(entries)>100:p.error('invalid source seed count')
    lookup=h.receipts(a.source_archive);by_ref={}
    for item in entries:
        if not isinstance(item,dict):p.error('invalid source seed')
        ref=item.get('jurisdiction_ref')
        if item.get('state')=='html_screened':
            if not isinstance(ref,str) or not re.fullmatch(r'us-co-place-[0-9]{7}',ref):p.error('invalid municipality seed')
            by_ref.setdefault(ref,[]).append(item)
    bound=[];errors=[]
    for ref in a.jurisdiction:
        items=by_ref.get(ref,[])
        if not items:
            errors.append({'jurisdiction_ref':ref,'reason':'requested source seed unavailable'});continue
        item,omissions=bind_group(a.source_archive,ref,items,lookup);errors.extend(omissions)
        if item:bound.append(item)
    counts={}
    for item in bound:counts[item['tenant']]=counts.get(item['tenant'],0)+1
    for item in bound[:]:
        if counts[item['tenant']]>1:
            errors.append({'jurisdiction_ref':item['jurisdiction_ref'],'reason':'same tenant linked by multiple jurisdictions; attribution unresolved'});bound.remove(item)
    out=a.output_parent/a.output_name;os.mkdir(out,0o700);os.mkdir(out/'content',0o700);budget=r.DownloadBudget(32*1024*1024);results=[]
    for item in bound:
        row={k:v for k,v in item.items() if k!='capture_proofs'}
        row['source_proofs']=[]
        for proof_url,proof_sha,(_,receipt),body in item['capture_proofs']:
            r.archive(out,[(receipt,body)])
            encoded=(json.dumps(receipt,sort_keys=True,separators=(',',':'))+'\n').encode()
            row['source_proofs'].append({'url':proof_url,'sha256':proof_sha,'receipt_sha256':r.sha(encoded)})
        host,url=list_url(item['tenant'],a.before);row['list_url']=url
        try:
            try:
                rules,observations=r.get_robots(host,{host},budget);r.archive(out,observations)
                row['robots_receipt_sha256']=[r.sha((json.dumps(meta,sort_keys=True,separators=(',',':'))+'\n').encode()) for meta,_ in observations]
            except r.RobotsDenied as e:r.archive(out,e.observations);raise
            allow=r.allowed(rules,url);row['robots_allowed']=allow;row['robots_override']=not allow and a.ignore_robots
            if not allow and not a.ignore_robots:raise ValueError('robots disallows list request')
            captures=r.get(url,{host},maximum=MAX_JSON,timeout=12,redirects=0,stop_at_redirect=True,budget=budget);r.archive(out,captures);meta,body=captures[-1];row.update(status=meta['status'],sha256=meta['sha256'],receipt_sha256=r.sha((json.dumps(meta,sort_keys=True,separators=(',',':'))+'\n').encode()))
            if meta['status']!=200:raise ValueError('HTTP '+str(meta['status']))
            if 'json' not in meta.get('content_type','').lower():raise ValueError('unexpected content type')
            row['metadata']=summarize(body,a.before);row['state']='list_captured'
            if a.fetch_files:row['file_probe']=capture_file(out,item,row,rules,host,budget,a.ignore_robots)
        except (OSError,ValueError,TypeError,KeyError,RuntimeError,RecursionError) as e:row['state']='error';row['reason']=str(e)[:150]
        results.append(row)
    payload={'schema_version':'pnull-private-civicclerk-v1','notice':'Private source-linked metadata and optional PDF bytes only. Jurisdiction/body attribution, document completeness, OCR, text screening and decisions remain unverified. No pagination or public promotion was performed.','cutoff':a.before.isoformat(),'requested_jurisdictions':a.jurisdiction,'results':results,'errors':errors}
    h.atomic_output(out/'report',h.bounded_json(payload),deadline)
    print(f'Private list requests: {len(results)}; successful: {sum(x.get("state")=="list_captured" for x in results)}; source bindings withheld: {len(errors)}.')

def cli():
    if sys.platform!='linux':raise SystemExit('Linux deadline required')
    old=signal.signal(signal.SIGALRM,h.deadline_handler);deadline=time.monotonic()+90;signal.setitimer(signal.ITIMER_REAL,90)
    try:main(deadline)
    except h.DeadlineExpired:raise SystemExit('private CivicClerk discovery timed out; no completed report')
    finally:signal.setitimer(signal.ITIMER_REAL,0);signal.signal(signal.SIGALRM,old)
if __name__=='__main__':cli()
