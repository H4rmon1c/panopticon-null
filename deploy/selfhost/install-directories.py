#!/usr/bin/env python3
"""Provision directories by FD; a planted path symlink must never change its target."""
import os
from pathlib import Path
import pwd


def directory(path, uid, gid, mode):
    path=Path(path)
    if not path.is_absolute() or '..' in path.parts:
        raise ValueError('absolute non-traversing directory required')
    descriptor=os.open('/',os.O_RDONLY|os.O_DIRECTORY|os.O_CLOEXEC)
    try:
        for component in path.parts[1:]:
            try: os.mkdir(component,0o755,dir_fd=descriptor)
            except FileExistsError: pass
            child=os.open(component,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW|os.O_CLOEXEC,dir_fd=descriptor)
            os.close(descriptor)
            descriptor=child
        os.fchown(descriptor,uid,gid)
        os.fchmod(descriptor,mode)
    finally:
        os.close(descriptor)


def main():
    if os.geteuid()!=0: raise PermissionError('root required')
    account=pwd.getpwnam('panopticon')
    for path,uid,gid,mode in [
        ('/srv/panopticon',0,0,0o755),
        ('/var/lib/panopticon',account.pw_uid,account.pw_gid,0o700),
        ('/srv/panopticon/site',account.pw_uid,account.pw_gid,0o755),
        ('/srv/panopticon/site/releases',account.pw_uid,account.pw_gid,0o755),
        ('/srv/panopticon/status',account.pw_uid,account.pw_gid,0o755),
        ('/etc/panopticon',0,account.pw_gid,0o750),
        ('/etc/caddy',0,0,0o755),
    ]:
        directory(path,uid,gid,mode)


if __name__=='__main__': main()
