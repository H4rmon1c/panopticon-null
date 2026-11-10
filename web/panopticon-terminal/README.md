# PANOPTICON.FAIL Civic Intelligence Atlas

> **Power leaves a record. We connect it.**

PANOPTICON.FAIL is a public intelligence commons for navigating documented institutional power. It is designed for two audiences using the same publication-safe data:

- Residents asking what is being built, funded, approved, owned, contracted, or changed in their community.
- Journalists and researchers tracing the underlying organizations, decisions, money, infrastructure, sources, and revisions.

The default surface is a place-first civic atlas. The relationship canvas is schematic; any geographic point is only a coarse jurisdictional anchor. The wide-area globe is retained only as an optional reporter lens.

## Public view

The public workspace begins with a manually entered town, county, or ZIP code and four ordinary questions:

- What changed?
- Where did money go?
- Who voted?
- What is being built?

Records include a plain-language public brief, why the record matters, documented connections, and an explicit list of facts the current public record does not yet establish. Unknowns remain unknown instead of being filled with inference.

## Reporter view

Reporter mode adds denser source tooling without changing the underlying claims:

- Advanced query syntax
- Source-first and change-first reading modes
- Evidence-pack export
- Citation-pack copy
- Optional wide-area relationship lens
- Exact quote, authority, locator, retrieval time, source-verification state, and content digest

## Scope and safety boundary

This system maps **institutional power through public records**. It does not provide private-person surveillance.

- No device-location access
- No private-person tracking
- People appear only in documented public or organizational roles
- No hidden collector state in the browser
- No pending, rejected, or unpublished claims in the public read model
- No relationship is presented as established without publication-safe evidence

The interface may be visually powerful. The evidence must remain literal, source-bound, and one click away.

## Data boundary

The browser loads the released static snapshot at `public/public-snapshot.json` by default. The checked-in file contains the source-verified Colorado Springs Ordinance 25-93 case, approved at the case level by h4rmon1c. A missing or invalid public snapshot produces an unavailable state; it never substitutes synthetic data.

`?demo=1` loads only `mock/public-snapshot.json`. All bundled demo entities and sources are synthetic. For owner-only editorial QA, `?preview=1` loads `preview-snapshot.json` and displays a prominent pending-case-release label. The preview file is gitignored and must be supplied only to the private build.

Each released decision has a shareable `decisions/<stable-id>.html` page with its own title, description, canonical URL, and X/Open Graph card. A decision dossier in the atlas links to that page. The generic `decision.html?decision=<stable-id>` route remains available for old links and for explicit preview/demo modes, but its static crawler metadata is neutral and `noindex`. The decision page shows each sourced relationship, recorded vote, exact evidence locator and digest, correction, and specific link still not established. It does not turn a funding ordinance into an inferred vendor contract.

After source verification and one case-level release approval, the Rust CLI writes the release artifact with `cargo run -p pnull-cli -- --site-url "$PNUL_SITE_URL" atlas export --output /absolute/path/to/web/panopticon-terminal/public/public-snapshot.json`. Draft editorial QA uses `atlas export --preview --output /absolute/path/to/web/panopticon-terminal/preview-snapshot.json` and only a private site build. Run the publisher's requirements and allow-release gate before the public export. Individual evidence records carry verification digests; the snapshot carries the case release decision. See the Rust repository's `docs/atlas-export.md` for the gate sequence.

The intended same-VPS architecture remains:

```text
private collector state
        |
source verification + case release gate
        |
sanitized atomic public snapshot
        |
PANOPTICON.FAIL civic atlas
```

The frontend must never query Panopticon's operational database directly.

An optional read-only API can replace the snapshot transport after the published dataset grows. It must retain the same publication boundary.

See [`docs/api-contract.md`](docs/api-contract.md) for the draft response shapes and publication rules.

## Same-VPS deployment

A suitable topology is:

```text
internet
   |
Caddy / nginx
   `-- static files: /var/www/panopticon.fail
             |
       public/public-snapshot.json
             ^
       gated atlas export
             ^
       Panopticon collector
```

Run the collector and reverse proxy under separate Unix users. The reverse proxy should read only the static site and sanitized public snapshot, with no access to collector secrets or private state. A read-only API can follow as the released dataset grows.

## Run locally

```console
cd web/panopticon-terminal
python3 -m http.server 4173
```

Open `http://127.0.0.1:4173` to browse the released Colorado Springs case. To inspect the synthetic demonstration, open `http://127.0.0.1:4173/?demo=1`. A private editorial build can supply `preview-snapshot.json` and use `?preview=1`.

Static social crawlers do not run the decision-page JavaScript. The first released decision page has Colorado Springs-specific title, description, and Open Graph/X metadata. Generate distinct HTML metadata per case before expanding coverage.

Opening `index.html` directly is not supported because browsers restrict module and JSON loading from `file://` URLs. For deterministic CI rendering, use `http://127.0.0.1:4173/?ci=1`.

## Design rule

> Nothing here asks for your trust. Every claim has a source.

## Colorado geography

The separate [Colorado jurisdiction roster](geography/README.md) lists official places and provisional district discovery entries without treating them as published cases. Its source archives are kept outside the web release allowlist.
