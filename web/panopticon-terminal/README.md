# PANOPTICON.FAIL public intelligence terminal

A dependency-free frontend skeleton for Panopticon Null's public, read-only intelligence interface.

The terminal is intentionally separate from the collector and its operational database. It first attempts to read `/api/v1`; when that API is absent, it falls back to the clearly labeled synthetic snapshot in `mock/public-snapshot.json`.

## What is included

- Search-first terminal home screen
- Entity workspaces
- Source-backed entity facts
- Interactive depth-one relationship graph
- Public timeline and source views
- Evidence/source drawer
- Keyboard navigation and command palette
- Standard, dense, and terminal density modes
- Responsive layout
- No build step and no runtime dependencies

## Run locally

From this directory:

```console
python3 -m http.server 4173
```

Then open `http://127.0.0.1:4173`.

Opening `index.html` directly is not supported because browsers restrict module and JSON loading from `file://` URLs.

## Public API boundary

The client expects these read-only endpoints:

```text
GET /api/v1/status
GET /api/v1/activity?limit=12
GET /api/v1/entities?limit=6&sort=updated
GET /api/v1/search?q=<query>&limit=12
GET /api/v1/entities/<id>
GET /api/v1/evidence/<id>
GET /api/v1/sources/<id>
```

Responses may be direct arrays or `{ "items": [...] }` envelopes where a collection is expected.

The frontend must never receive collector credentials, source-management controls, pending findings, rejected claims, operator notes, or access to Panopticon's private working database. A future `pnull-public` publisher should atomically produce a sanitized public read model; a future `pnull-api` process should expose only that read model.

See [`docs/api-contract.md`](docs/api-contract.md) for the draft response shapes and publication rules.

## Same-VPS deployment

A suitable topology is:

```text
internet
   |
Caddy / nginx
   |-- static files: /var/www/panopticon.fail
   `-- /api/* -> pnull-api on 127.0.0.1 or a Unix socket
                         |
                  public.sqlite (read-only)
                         ^
                    atomic publish
                         ^
                  Panopticon collector
```

Run the collector, API, and reverse proxy under separate Unix users. The public API user should have read-only access to the sanitized public database and no access to collector secrets or private state.

## Mock-data rule

Every item in the bundled snapshot is synthetic and visually labeled `DEMO DATA`. The mock entities demonstrate the intended datacenter/hyperscale expansion without claiming real-world facts.

## Design rule

> Nothing here asks for your trust. Every claim has a source.
