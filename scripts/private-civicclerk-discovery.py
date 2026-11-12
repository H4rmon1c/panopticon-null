#!/usr/bin/env python3
"""Bounded source-linked CivicClerk list and published-file reconnaissance.

Creates a fresh private archive, not a research database or a publication.
Vendor metadata is a lead and does not establish a responsible body or vote.
"""
import argparse
from datetime import date,datetime,timezone
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
MAX_TENANTS=3;MAX_LIST_PAGES=2;MAX_RESUME_ARCHIVES=16;MAX_EVENTS=100;MAX_FILES=20;MAX_JSON=2*1024*1024;MAX_PDF=8*1024*1024
PAGINATION_INDEX='/assets/index-DOHLNEJb.js'
PAGINATION_INDEX_SHA='6290028848edbc67b1cfa9f14bed77885cbddb459ef2864d853e14f7c5dd9955'
PAGINATION_LIST='/assets/EventList-YeUmi4cy.js'
PAGINATION_LIST_SHA='6e7dca12c301731a615095415d37b51f32bd76ad3004f31990f6fc8234375b1f'

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
    events=[];seen_events=set();previous=None;ties=False
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
        try:
            instant=datetime.fromisoformat(stamp.replace('Z','+00:00'))
            if instant.tzinfo is None or instant.utcoffset() is None:raise ValueError('event timestamp lacks timezone')
            instant=instant.astimezone(timezone.utc)
        except OverflowError:raise ValueError('event timestamp outside supported UTC range') from None
        if previous is not None and instant>previous:raise ValueError('event order is not descending')
        if previous is not None and instant==previous:ties=True
        previous=instant
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
        events.append({'event_id':e['id'],'event_name':name,'category':category,'date':stamp[:10],'timestamp':instant.isoformat(),'json_path':f'$.items[{n}]','published_files_state':file_state,'published_files':saved})
    if obj['hasMore'] and not events:raise ValueError('empty page claims more events')
    return {'items_count':len(items),'has_more':obj['hasMore'],'timestamp_ties_observed':ties,'events':events}

def list_url(tenant,cutoff,cursor=0):
    if not re.fullmatch(r'[a-z0-9-]{1,80}',tenant):raise ValueError('invalid tenant')
    if type(cursor) is not int or not 0<=cursor<1000000000:raise ValueError('invalid numeric cursor')
    path="/Events/GetEventsList(fromDate='',toDate='"+cutoff.isoformat()+"',categoryIds=[],skip=0,sortDescending=true,cursorId="+str(cursor)+")"
    host=tenant+'.api.civicclerk.com';return host,'https://'+host+'/v1'+quote(path,safe="/(),=:'")

def pagination_assets(out,item,budget,ignore_robots):
    """Bind the cursor implementation to the captured portal and its scripts."""
    host=urlsplit(item['portal_url']).hostname
    shell=next(proof[3] for proof in item['capture_proofs'] if proof[0]==item['portal_url'])
    parser=Assets();parser.feed(shell.decode('utf8','strict'))
    if parser.paths.count(PAGINATION_INDEX)!=1:raise ValueError('pagination entry asset not linked')
    try:rules,observations=r.get_robots(host,{host},budget);r.archive(out,observations)
    except r.RobotsDenied as e:r.archive(out,e.observations);raise
    robots_receipts=[r.sha((json.dumps(meta,sort_keys=True,separators=(',',':'))+'\n').encode()) for meta,_ in observations]
    proofs=[];index_body=None
    for path,expected in ((PAGINATION_INDEX,PAGINATION_INDEX_SHA),(PAGINATION_LIST,PAGINATION_LIST_SHA)):
        if path==PAGINATION_LIST and (index_body is None or path[1:] not in index_body.decode('utf8','strict')):raise ValueError('pagination list asset not linked')
        url='https://'+host+path;allowed=r.allowed(rules,url)
        if not allowed and not ignore_robots:raise ValueError('robots disallows pagination asset')
        captures=r.get(url,{host},maximum=MAX_JSON,timeout=12,redirects=0,stop_at_redirect=True,budget=budget);r.archive(out,captures);meta,body=captures[-1]
        if meta['status']!=200 or meta['sha256']!=expected:raise ValueError('pagination asset differs or HTTP '+str(meta['status']))
        if path==PAGINATION_INDEX:index_body=body
        proofs.append({'url':url,'sha256':meta['sha256'],'receipt_sha256':r.sha((json.dumps(meta,sort_keys=True,separators=(',',':'))+'\n').encode()),'robots_allowed':allowed,'robots_override':not allowed and ignore_robots,'robots_receipt_sha256':robots_receipts})
    return proofs

def checked_prior_page(root,page,tenant,cutoff):
    cursor=page.get('input_cursor',0);host,url=list_url(tenant,cutoff,cursor)
    if page.get('list_url')!=url:raise ValueError('prior list URL or cursor differs')
    sha=page.get('sha256');receipt_sha=page.get('receipt_sha256')
    if not isinstance(receipt_sha,str) or not re.fullmatch(r'[0-9a-f]{64}',receipt_sha):raise ValueError('invalid prior receipt digest')
    raw=r.read_blob(root/'observations'/(receipt_sha+'.json'),65536)
    if r.sha(raw)!=receipt_sha:raise ValueError('prior receipt digest differs')
    receipt=json.loads(raw)
    if not r.receipt_valid(receipt,url,sha) or not isinstance(receipt.get('content_type'),str) or 'json' not in receipt['content_type'].lower():raise ValueError('prior list receipt differs')
    body=r.source_blob(root,sha,MAX_JSON);metadata=summarize(body,cutoff)
    if not metadata['has_more'] or not metadata['events']:raise ValueError('prior list has no verified continuation')
    return metadata,receipt,body

def resume_cursor(root,parent,item,cutoff):
    """Recheck bounded ancestor chain from cursor zero, preserving omissions."""
    chain=[];seen_paths=set();current=root
    for _ in range(MAX_RESUME_ARCHIVES):
        if (not current.is_absolute() or current.parent!=parent or current.is_symlink() or current.resolve()!=current
            or not current.is_dir() or current.stat().st_uid!=os.geteuid() or current.stat().st_mode&0o077):raise ValueError('resume archive must be an owner-only direct child of output parent')
        if current in seen_paths:raise ValueError('resume archive cycle')
        seen_paths.add(current)
        for name in ('content','observations','report'):
            path=current/name
            if path.is_symlink() or not path.is_dir():raise ValueError('invalid resume archive child')
        prior=json.loads(r.read_blob(current/'report/results.json',8*1024*1024))
        if not isinstance(prior,dict):raise ValueError('invalid prior report object')
        prior_rows=prior.get('results')
        if not isinstance(prior_rows,list) or len(prior_rows)>MAX_TENANTS or any(not isinstance(v,dict) for v in prior_rows):raise ValueError('invalid prior result rows')
        if prior.get('schema_version') not in ('pnull-private-civicclerk-v1','pnull-private-civicclerk-v2') or prior.get('cutoff')!=cutoff.isoformat():raise ValueError('prior schema or cutoff differs')
        rows=[v for v in prior_rows if v.get('jurisdiction_ref')==item['jurisdiction_ref']]
        if len(rows)!=1 or rows[0].get('tenant')!=item['tenant'] or rows[0].get('portal_url')!=item['portal_url'] or rows[0].get('source_anchor')!=item['source_anchor']:raise ValueError('prior tenant or source binding differs')
        row=rows[0];pages=row.get('pages') if 'pages' in row else [row]
        if not isinstance(pages,list) or not 1<=len(pages)<=MAX_LIST_PAGES or any(not isinstance(p,dict) or p.get('state')!='list_captured' for p in pages):raise ValueError('prior pages contain a gap or error')
        chain.append((current,pages))
        previous=prior.get('resume_parent')
        if previous is None:break
        if not isinstance(previous,str) or not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,63}',previous):raise ValueError('invalid prior archive name')
        current=parent/previous
    else:raise ValueError('resume ancestry limit exceeded')
    expected_cursor=0;seen=set();previous_time=None;ties=False;last=None
    for archive,pages in reversed(chain):
        for page in pages:
            if page.get('input_cursor',0)!=expected_cursor:raise ValueError('prior cursor chain differs')
            metadata,receipt,body=checked_prior_page(archive,page,item['tenant'],cutoff)
            ids={e['event_id'] for e in metadata['events']}
            if seen&ids:raise ValueError('prior pages repeat event IDs')
            first=datetime.fromisoformat(metadata['events'][0]['timestamp'])
            if previous_time is not None and first>previous_time:raise ValueError('prior page ordering differs')
            if previous_time is not None and first==previous_time:ties=True
            ties=ties or metadata['timestamp_ties_observed'];previous_time=datetime.fromisoformat(metadata['events'][-1]['timestamp'])
            seen.update(ids);expected_cursor=metadata['events'][-1]['event_id']
            if expected_cursor==page.get('input_cursor',0):raise ValueError('repeated prior cursor')
            last=(archive,page,metadata,receipt,body)
    return expected_cursor,seen,previous_time,ties,last


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
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--source-archive',type=Path,required=True);p.add_argument('--output-parent',type=Path,required=True);p.add_argument('--output-name',required=True);p.add_argument('--seed-name',default='private-probe-results-v3.json');p.add_argument('--before',required=True,type=date.fromisoformat);p.add_argument('--limit',type=int,default=3);p.add_argument('--pages',type=int,default=1);p.add_argument('--resume-archive',type=Path);p.add_argument('--jurisdiction',action='append',required=True,help='explicit municipal jurisdiction reference; repeat up to the limit');p.add_argument('--ignore-robots',action='store_true');p.add_argument('--fetch-files',action='store_true',help='capture at most one explicit published PDF per tenant');a=p.parse_args()
    for path in (a.source_archive,a.output_parent):
        if not path.is_absolute() or path.is_symlink() or path.resolve()!=path or not path.is_dir() or path.stat().st_uid!=os.geteuid() or path.stat().st_mode&0o077:p.error('archives must be real owner-only absolute directories')
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,63}',a.output_name) or not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,63}\.json',a.seed_name) or not 1<=a.limit<=MAX_TENANTS or not 1<=a.pages<=MAX_LIST_PAGES or (a.resume_archive is not None and len(a.jurisdiction)!=1) or not 1<=len(a.jurisdiction)<=a.limit or len(set(a.jurisdiction))!=len(a.jurisdiction) or any(not re.fullmatch(r'us-co-place-[0-9]{7}',ref) for ref in a.jurisdiction) or a.before>date.today():p.error('invalid output, seed name, selection, limit, or cutoff')
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
        row['pages']=[];row['pagination_state']='incomplete_error';row['pagination_notice']='Pagination remains incomplete; ties, concurrent changes and source omissions may hide records.'
        cursor=0;seen=set();previous_time=None;ties=False
        try:
            if a.resume_archive is not None:
                cursor,seen,previous_time,ties,last=resume_cursor(a.resume_archive,a.output_parent,item,a.before)
                prior_archive,prior_page,prior_metadata,prior_receipt,prior_body=last
                r.archive(out,[(prior_receipt,prior_body)])
                row['resume_anchor']={'list_url':prior_page['list_url'],'sha256':prior_page['sha256'],'receipt_sha256':prior_page['receipt_sha256'],'next_cursor':cursor}
            if a.pages>1 or a.resume_archive is not None:row['pagination_asset_proofs']=pagination_assets(out,item,budget,a.ignore_robots)
            host,_=list_url(item['tenant'],a.before)
            try:
                rules,observations=r.get_robots(host,{host},budget);r.archive(out,observations)
                row['robots_receipt_sha256']=[r.sha((json.dumps(meta,sort_keys=True,separators=(',',':'))+'\n').encode()) for meta,_ in observations]
            except r.RobotsDenied as e:r.archive(out,e.observations);raise
            visited_cursors=set()
            for _ in range(a.pages):
                if cursor in visited_cursors:raise ValueError('repeated pagination cursor')
                visited_cursors.add(cursor);_,url=list_url(item['tenant'],a.before,cursor);page={'input_cursor':cursor,'list_url':url,'state':'error'}
                try:
                    allow=r.allowed(rules,url);page['robots_allowed']=allow;page['robots_override']=not allow and a.ignore_robots;row['robots_allowed']=allow;row['robots_override']=page['robots_override']
                    if not allow and not a.ignore_robots:raise ValueError('robots disallows list request')
                    captures=r.get(url,{host},maximum=MAX_JSON,timeout=12,redirects=0,stop_at_redirect=True,budget=budget);r.archive(out,captures);meta,body=captures[-1]
                    page.update(status=meta['status'],sha256=meta['sha256'],receipt_sha256=r.sha((json.dumps(meta,sort_keys=True,separators=(',',':'))+'\n').encode()));row['status']=meta['status']
                    if meta['status']!=200:raise ValueError('HTTP '+str(meta['status']))
                    if 'json' not in meta.get('content_type','').lower():raise ValueError('unexpected content type')
                    metadata=summarize(body,a.before);ids={e['event_id'] for e in metadata['events']}
                    if seen&ids:raise ValueError('pagination repeats event IDs')
                    if metadata['events']:
                        first=datetime.fromisoformat(metadata['events'][0]['timestamp'])
                        if previous_time is not None and first>previous_time:raise ValueError('pagination ordering changed')
                        if previous_time is not None and first==previous_time:ties=True
                        previous_time=datetime.fromisoformat(metadata['events'][-1]['timestamp'])
                    ties=ties or metadata['timestamp_ties_observed'];seen.update(ids)
                    next_cursor=metadata['events'][-1]['event_id'] if metadata['has_more'] else None
                    if next_cursor is not None and (next_cursor==cursor or next_cursor in visited_cursors):raise ValueError('repeated pagination cursor')
                    page.update(state='list_captured',metadata=metadata)
                    if next_cursor is not None:page['next_cursor']=next_cursor
                    row['pages'].append(page)
                    # Compatibility fields refer to the last validated list page.
                    row.update(list_url=url,status=meta['status'],sha256=meta['sha256'],receipt_sha256=page['receipt_sha256'],metadata=metadata,state='list_captured')
                    if next_cursor is None:
                        row['pagination_state']='source_end_unverified';break
                    cursor=next_cursor;row['pagination_state']='page_limit_reached'
                except (OSError,ValueError,TypeError,KeyError,RuntimeError,RecursionError) as e:
                    page.update(state='error',reason=str(e)[:150]);row['pages'].append(page);raise
            row['timestamp_ties_observed']=ties
            if a.fetch_files and row.get('state')=='list_captured':
                row['file_probe']=capture_file(out,item,row,rules,host,budget,a.ignore_robots)
                row['file_probe']['source_list_url']=row['list_url'];row['file_probe']['source_list_sha256']=row['sha256'];row['file_probe']['source_list_receipt_sha256']=row['receipt_sha256']
        except (OSError,ValueError,TypeError,KeyError,RuntimeError,RecursionError) as e:
            if row.get('state')!='list_captured':row['state']='error'
            row['pagination_state']='incomplete_error';row['reason']=str(e)[:150]
        results.append(row)
    payload={'schema_version':'pnull-private-civicclerk-v2','notice':'Private source-linked metadata and optional PDF bytes only. Jurisdiction/body attribution, document completeness, OCR, text screening and decisions remain unverified. Pagination is bounded; completeness and public promotion are not asserted.','cutoff':a.before.isoformat(),'resume_parent':a.resume_archive.name if a.resume_archive is not None else None,'requested_jurisdictions':a.jurisdiction,'results':results,'errors':errors}
    h.atomic_output(out/'report',h.bounded_json(payload),deadline)
    print(f'Private list requests: {len(results)}; successful: {sum(x.get("state")=="list_captured" for x in results)}; source bindings withheld: {len(errors)}.')

def cli():
    if sys.platform!='linux':raise SystemExit('Linux deadline required')
    old=signal.signal(signal.SIGALRM,h.deadline_handler);deadline=time.monotonic()+90;signal.setitimer(signal.ITIMER_REAL,90)
    try:main(deadline)
    except h.DeadlineExpired:raise SystemExit('private CivicClerk discovery timed out; no completed report')
    finally:signal.setitimer(signal.ITIMER_REAL,0);signal.signal(signal.SIGALRM,old)
if __name__=='__main__':cli()
