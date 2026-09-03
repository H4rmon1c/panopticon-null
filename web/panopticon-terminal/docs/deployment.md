# PANOPTICON.FAIL same-VPS deployment

PANOPTICON.FAIL may run on the same VPS as the collector without sharing the collector's trust boundary.

## Target topology

```text
internet
   |
Caddy / nginx
   |-- static Civic Intelligence Atlas
   `-- /api/* -> pnull-api on 127.0.0.1 or a Unix socket
                         |
                  public.sqlite (read-only)
                         ^
                  atomic publication
                         ^
                  Panopticon collector
```

The public browser never connects to the collector and never queries its operational database.

## Unix users

Use separate Unix identities:

```text
pnull-collector
pnull-api
caddy
```

`pnull-collector` may read and write private state, source credentials, immutable evidence, and temporary publication output. It has no public inbound listener.

`pnull-api` may read only the current sanitized public artifact. It cannot read collector state, credentials, pending findings, rejected claims, review notes, or unpublished evidence. It listens only on loopback or a Unix socket.

`caddy` serves static files and proxies `/api/*`. It has no access to collector secrets.

## Suggested paths

```text
/var/lib/panopticon/private/
    state.sqlite
    evidence/
    credentials/

/var/lib/panopticon-public/
    public.sqlite
    manifest.json

/var/www/panopticon.fail/
    index.html
    styles.css
    app.js
    scripts/
    styles/
```

## Publication sequence

```text
collect
  -> parse
  -> reconcile
  -> review
  -> select publication-safe claims
  -> build public.sqlite.tmp
  -> validate references and review bindings
  -> fsync database and manifest
  -> atomic rename to public.sqlite
  -> notify or reopen pnull-api
```

A failed publication must leave the previous public dataset intact. Collection never implies publication.

## Civic-place data

The public artifact may include coarse public place and jurisdiction indexes needed for deliberate town, county, or ZIP searches. The frontend should not request device geolocation. Place searches should resolve only against public geographic and jurisdiction records.

People may appear only in documented public or organizational roles. Private-person surveillance records are outside the public model.

## Reverse proxy example

See [`../deploy/Caddyfile.example`](../deploy/Caddyfile.example). The reverse proxy should serve static files directly and send only `/api/*` to the loopback API.

Recommended response protections include:

- strict transport security
- content security policy
- MIME sniffing protection
- referrer policy
- clickjacking protection
- short caching for status/search/activity
- immutable caching for content-stable source and evidence routes

## Operational rule

The interface can be visually powerful. The public process remains intentionally boring:

- read-only
- sanitized
- independently restartable
- no collector credentials
- no administrative routes
- no direct operational database access
