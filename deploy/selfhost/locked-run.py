#!/usr/bin/env python3
"""Acquire nontruncating, no-follow regular lock files, then exec with held FDs."""
import fcntl,json,os,stat,sys


def check(fd, path):
    opened=os.fstat(fd)
    named=os.stat(path,follow_symlinks=False)
    if not stat.S_ISREG(opened.st_mode) or not stat.S_ISREG(named.st_mode) or (opened.st_dev,opened.st_ino)!=(named.st_dev,named.st_ino):
        raise ValueError('lock must be the named regular file')
    fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)


def main():
    args=sys.argv[1:]
    if args[:1]==['--verify']:
        paths=args[1:]
        held=json.loads(os.environ['PNUL_HELD_LOCKS'])
        if not isinstance(held,list) or len(held)!=len(paths): raise ValueError('lock set mismatch')
        for item,path in zip(held,paths):
            if item['path']!=path or not isinstance(item['fd'],int) or item['fd']<3: raise ValueError('lock descriptor mismatch')
            check(item['fd'],path)
        return
    separator=args.index('--')
    paths,command=args[:separator],args[separator+1:]
    if not paths or not command or len(paths)>4: raise ValueError('expected lock paths and command')
    held=[]
    for path in paths:
        if not os.path.isabs(path): raise ValueError('absolute lock path required')
        fd=os.open(path,os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW|os.O_CLOEXEC,0o600)
        check(fd,path)
        os.set_inheritable(fd,True)
        held.append({'path':path,'fd':fd})
    os.environ['PNUL_HELD_LOCKS']=json.dumps(held)
    os.execvpe(command[0],command,os.environ)


if __name__=='__main__':
    try: main()
    except (OSError,ValueError,KeyError,TypeError) as error:
        print(f'lock unavailable or unsafe: {error}',file=sys.stderr)
        sys.exit(1)
