# PANOPTICON.FAIL Civic Intelligence Atlas

> **Power leaves a record. We connect it.**

PANOPTICON.FAIL is a public intelligence commons for navigating documented institutional power. It is designed for two audiences using the same publication-safe data:

- Residents asking what is being built, funded, approved, owned, contracted, or changed in their community.
- Journalists and researchers tracing the underlying organizations, decisions, money, infrastructure, sources, and revisions.

The default surface is a place-first civic atlas. The wide-area globe is retained only as an optional reporter lens; it is not the product's identity.

## Public view

The public workspace begins with a manually entered town, county, or ZIP code and four ordinary questions:

- What changed?
- Where did money go?
- Who approved it?
- What is being built?

Records include a plain-language public brief, why the record matters, documented connections, and an explicit list of facts the current public record does not yet establish. Unknowns remain unknown instead of being filled with inference.

## Reporter view

Reporter mode adds denser source tooling without changing the underlying claims:

- Advanced query syntax
- Source-first and change-first reading modes
- Evidence-pack export
- Citation-pack copy
- Optional wide-area relationship lens
- Exact quote, authority, locator, retrieval time, review state, and content digest

## Scope and safety boundary

This system maps **institutional power through public records**. It does not provide private-person surveillance.

- No device-location access
- No private-person tracking
- People appear only in documented public or organizational roles
- No hidden collector state in the browser
- No pending, rejected, or unpublished claims in the public read model
- No relationship is presented as established without publication-safe evidence

The interface may be visually powerful. The evidence must remain literal, review-bound, and one click away.

## Data boundary

The browser attempts the read-only public API first:

```text
/api/v1/status
/api/v1/activity
/api/v1/entities
/api/v1/search
/api/v1/entities/:id
/api/v1/evidence/:id
/api/v1/sources/:id
```

The intended same-VPS architecture remains:

```text
private collector state
        |
review + publication gates
        |
sanitized atomic public read model
        |
read-only loopback API
        |
PANOPTICON.FAIL civic atlas
```

The frontend must never query Panopticon's operational database directly.

Until the public API exists, the branch falls back to `mock/public-snapshot.json`. Every bundled organization, facility, source, contract, and relationship is synthetic and marked `DEMO DATA`.

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

## Run locally

```console
cd web/panopticon-terminal
python3 -m http.server 4173
```

Open `http://127.0.0.1:4173`.

Opening `index.html` directly is not supported because browsers restrict module and JSON loading from `file://` URLs. For deterministic CI rendering, use `http://127.0.0.1:4173/?ci=1`.

## Design rule

> Nothing here asks for your trust. Every claim has a source.
