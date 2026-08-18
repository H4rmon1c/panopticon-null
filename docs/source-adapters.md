# Source adapters

## Selected jurisdiction

Panopticon Null monitors **Colorado Springs, Colorado**. The City directs users seeking recent agendas and minutes to its Legistar portal:
- City discovery: <https://coloradosprings.gov/city-council-meetings>
- City document guidance: <https://coloradosprings.gov/citydocs>
- Public calendar: <https://coloradosprings.legistar.com/Calendar.aspx>
- Documented API: <https://webapi.legistar.com/Help/Api/GET-v1-Client-Events>
- Configured collection: the URL in `configs/states/co.toml`, using documented OData filtering, ordering, a bounded top count, and expanded event items.

Colorado Springs was selected because the City officially links a stable, ID-addressable, structured public meeting system and because official matters 25-581 (v0.0.1) and 15-00663 (v0.0.2) include concrete surveillance-related references. The official City solicitation index is informational and points to BidNet as authoritative; BidNet may require registration and has restrictive terms, so Panopticon Null does not automate it.

## DNS-safe HTTP layer

`pnull-http` governs all live retrieval. Every request and redirect persists provenance: requested URL, resolved public IPs, retrieval timestamp, method, status code, redirect target, final URL, allowlisted headers, content type, content length, ETag, Last-Modified, response-body digest, and structured errors. Cookies, authorization headers, and bearer tokens are never persisted.

- Rejects loopback, private, link-local, multicast, unspecified, documentation, and non-public addresses; fails closed on mixed public + prohibited DNS answers.
- Requires HTTPS; certificate validation cannot be disabled.
- Conditional requests use `If-None-Match` / `If-Modified-Since`; a 304 creates a fetch observation referencing previous preserved evidence, never a new blob.
- The resolver and transport are abstractions so CI tests stay offline.

## Persistent robots/terms review

Live retrieval requires a persistent, expiring source review rather than an ephemeral flag. The review workflow:

- `pnull source review capture <id>` — snapshot current robots/terms state.
- `pnull source review record <id> --reviewer <name> --note <text> --expires <date>` — record an immutable review artifact (source ID, source-config digest, reviewed hosts, robots URL + snapshot digest + provenance, terms URLs, reviewer, note, review/expiration timestamps, minimum request interval, restrictions, superseded review).
- `pnull source review show <id>` — display the current review.
- `pnull source review verify <id>` — verify the review is current and applicable.

Live retrieval refuses when: there is no review, the review is expired, the source configuration changed, allowed hosts changed, the endpoint is outside the reviewed scope, or a prior restriction requires renewed review.

The ephemeral `--robots-reviewed` flag is deprecated and is no longer the primary authorization.

## Bounded Legistar pagination and attachment discovery

The Legistar adapter pages through the documented API one request at a time with:

- a configurable page size and a hard maximum number of pages;
- maximum total events, maximum matters, and maximum attachments per matter;
- aggregate byte and time budgets;
- deduplication by official identifiers;
- repeated-page / non-progressing detection;
- deterministic ordering;
- conditional requests and cached observations;
- fail-closed behavior on malformed identifiers or unknown hosts.

Attachment discovery proceeds only through documented official fields and reviewed hosts. Commands:

- `pnull ingest --source colorado-springs-legistar-events`
- `pnull matter list`
- `pnull matter show <id>`
- `pnull matter attachments <id>`

## Preserved demonstration matters

**Matter 25-581 / Matter API ID 12913** (v0.0.1) concerns a Police Department Technology Surcharge, later enacted as Ordinance 25-93. Fixtures preserve:

- draft ordinance attachment `14876734`;
- supporting presentation `14876735`;
- signed ordinance attachment `14995655`;
- work-session event `2654`;
- final-vote event `2660`.

**Matter 15-00663** (v0.0.2) is Ordinance No. 15-84 (2015), which established the municipal court Information Technology Surcharge that Ordinance 25-93 amends. Fixtures preserve:

- `fixtures/co2/matter-15-00663-ordinance-15-84.json`;
- `fixtures/co2/event-1109-2015-11-24.json`.

The surveillance-technology link (Axon body cameras/evidence systems/AI transcription, Flock vehicle-intelligence cameras) is documented in the preserved 2025 presentation as supporting evidence, not asserted by the 2015 action itself. No separate vendor contract or award for Axon or Flock was located in the reviewed Legistar source, so no such procurement is asserted.

Exact URLs and hashes are in `fixtures/README.md`, `fixtures/co/SHA256SUMS`, and `fixtures/co2/SHA256SUMS`.

## Procurement adapters (v0.0.3)

The v0.0.3 procurement surfaces are documented in `docs/0.0.3-source-survey.md`. In summary,
each adapter records a coverage-ledger entry and preserves every fetched artifact as an
immutable snapshot with row-level provenance.

- **Colorado Springs contract-award table** (official informational mirror) — parses the
  City-hosted contract-award table into rows with the solicitation identifier, project
  name, awarded contractor, raw awarded amount, parsed amount state, contract start date,
  notes, source snapshot, and row-level provenance. The parser tolerates historical
  formatting irregularities without silently shifting columns. Money states keep `N/A`,
  `various`, `$0.00 IDIQ`, and an omitted amount distinct.
- **Colorado Springs solicitation mirror** (official informational mirror) — parses the
  City-hosted informational solicitation list and linked City-hosted documents. Every
  record carries the source's own warning that the list may be incomplete or outdated; the
  connector never claims it represents every solicitation.
- **OpenBook COS** (official financial export) — investigates the official downloadable or
  directly connectable financial export. The documented finding is that OpenBook COS is a
  budget-level export that does not provide vendor-level expenditures, so no vendor-level
  payment relationship is invented. This negative capability finding is preserved and
  visible. Payment evidence from this source is represented as unavailable.
- **Operator-supplied public records** — a safe import path for public records obtained
  manually or through CORA. Requires a declared source URL or records-request identifier,
  acquisition date, document role, an operator declaration of lawful possession, an exact
  file digest, processing provenance, the existing sandbox and resource limits, and human
  review before publication. Supplied files are treated as hostile.

Live retrieval of any procurement surface still requires an explicit live mode and an
approved persistent source review; default demonstrations are fully offline. BidNet and
Bonfire (the City's authoritative portals) are not automated because they are
registration/terms-restricted.

## Limitations

The API is a meeting source, not a complete procurement ledger. Some contracts and amendments may appear only in attachments, the City document system, BidNet, or a Colorado Open Records Act response. Absence from this feed proves nothing. The reviewed source covers two matters; it is not comprehensive procurement coverage. Panopticon Null does not schedule itself, monitor BidNet, or claim to have located every vendor contract or award.
