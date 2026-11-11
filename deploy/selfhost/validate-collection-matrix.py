#!/usr/bin/env python3
"""Offline validation of every scheduled source before any collection CLI call."""
import argparse
import ipaddress
from pathlib import Path
import re
import tomllib
from urllib.parse import urlsplit

ADAPTERS={'ingest','procurement','archive','discover-calendar','discover-index'}


def safe_file(root, relative):
    path=Path(relative)
    if path.is_absolute() or '..' in path.parts:
        raise ValueError('schedule/config path must stay in repository')
    full=root/path
    if any((root/Path(*path.parts[:i])).is_symlink() for i in range(1,len(path.parts)+1)):
        raise ValueError(f'symlinked schedule/config: {relative}')
    if not full.is_file() or full.stat().st_size>1048576:
        raise ValueError(f'missing or oversized schedule/config: {relative}')
    return full


def rows(root,path):
    result=[]
    for line in safe_file(root,path).read_text().splitlines():
        if not line or line.startswith('#'): continue
        fields=line.split('\t')
        if len(fields)!=3 or any(not field for field in fields): raise ValueError('schedule requires three TSV fields')
        result.append(tuple(fields))
    if len(result)>512: raise ValueError('schedule exceeds 512 rows')
    return result


def public_url(raw):
    url=urlsplit(raw)
    if url.scheme!='https' or not url.hostname or url.port not in (None,443) or url.username or url.password or url.fragment:
        raise ValueError('source requires an exact public HTTPS URL')
    if '.' not in url.hostname or url.hostname.endswith(('.localhost','.local','.internal','.invalid')):
        raise ValueError('source hostname is not public')
    try: ipaddress.ip_address(url.hostname)
    except ValueError: pass
    else: raise ValueError('literal source IP not allowed')
    return url


def fixed_path(path):
    return path.startswith('/') and path!='/' and len(path)<=160 and not any(x in path for x in ('%','\\','//')) and all(x not in ('.','..') for x in path.split('/'))


def validate(root, matrix, fallbacks):
    root=Path(root).resolve()
    schedule=rows(root,matrix); fallback_rows=rows(root,fallbacks)
    if not schedule: raise ValueError('empty collection schedule')
    configs={}
    def source(config_path,identity):
        if not re.fullmatch(r'configs/states/co[-a-z0-9]*\.toml',config_path) or not re.fullmatch(r'[a-z0-9-]+',identity):
            raise ValueError('invalid configuration path/source identity')
        if config_path not in configs:
            config=tomllib.loads(safe_file(root,config_path).read_text())
            if config.get('state_code')!='CO': raise ValueError('non-Colorado source configuration')
            items=config.get('sources',[])
            if not isinstance(items,list) or len(items)>512: raise ValueError('invalid source list')
            ids=[item.get('id') for item in items]
            if len(set(ids))!=len(ids): raise ValueError('duplicate source ID in configuration')
            configs[config_path]={item['id']:item for item in items}
        item=configs[config_path].get(identity)
        if item is None: raise ValueError(f'unknown configured source: {identity}')
        public_url(item['url'])
        if type(item.get('minimum_interval_seconds')) is not int or not 0<item['minimum_interval_seconds']<=31536000:
            raise ValueError('invalid source request interval')
        if type(item.get('maximum_bytes')) is not int or not 0<item['maximum_bytes']<=20971520:
            raise ValueError('invalid source byte limit')
        return item
    seen={}
    for config,identity,adapter in schedule:
        if identity in seen or adapter not in ADAPTERS: raise ValueError('duplicate source or unsupported adapter')
        item=source(config,identity);url=public_url(item['url']);kind=item['source_type']
        if adapter=='archive' and kind not in ('html_page','pdf','official_api'): raise ValueError('archive adapter source type mismatch')
        if adapter=='ingest' and (kind!='official_api' or url.hostname!='webapi.legistar.com' or not re.fullmatch(r'/v1/[a-z0-9-]+/events',url.path)):
            raise ValueError('ingest requires an official Legistar event source')
        if adapter=='procurement' and (kind!='html_page' or identity not in ('colorado-springs-contract-awards','colorado-springs-solicitation-mirror')):
            raise ValueError('unsupported procurement parser/source')
        if adapter=='discover-calendar' and (kind!='html_page' or url.path.lower()!='/calendar.aspx' or url.query):
            raise ValueError('calendar adapter requires an exact HTML calendar URL')
        if adapter=='discover-index':
            if kind!='html_page' or not fixed_path(url.path) or url.path.lower()=='/calendar.aspx' or url.query or item['maximum_bytes']>2097152:
                raise ValueError('document-index adapter requires bounded exact HTML index')
            policies=[p for p in configs[config].values() if p.get('source_type')=='candidate_index_pdf' and p.get('official_discovery_url')==item['url'] and p.get('documented_at')==item['url']]
            if len(policies)!=1: raise ValueError('document index requires one matching PDF path policy')
            policy=source(config,policies[0]['id']);pdf=public_url(policy['url'])
            if pdf.netloc!=url.netloc or not fixed_path(pdf.path) or not pdf.path.endswith('/') or pdf.query or policy['minimum_interval_seconds']<120:
                raise ValueError('document index PDF policy must be bounded on the same host')
        seen[identity]=(config,adapter)
    primary_ids=set();fallback_ids=set()
    for config,primary,fallback in fallback_rows:
        if seen.get(primary)!=(config,'discover-calendar') or primary in primary_ids or fallback in fallback_ids or fallback in seen:
            raise ValueError('invalid or duplicate fallback association')
        item=source(config,fallback);url=public_url(item['url'])
        if item['source_type']!='official_api' or url.hostname!='webapi.legistar.com' or not re.fullmatch(r'/v1/[a-z0-9-]+/events',url.path):
            raise ValueError('fallback must be configured official Legistar events')
        primary_ids.add(primary);fallback_ids.add(fallback)
    return schedule


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('repository');parser.add_argument('matrix');parser.add_argument('fallbacks')
    args=parser.parse_args()
    validate(args.repository,args.matrix,args.fallbacks)
