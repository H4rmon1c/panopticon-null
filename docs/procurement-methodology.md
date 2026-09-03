# Procurement methodology

Panopticon Null v0.0.3 ("The Procurement Chain") turns isolated evidence receipts into a
verifiable institutional money trail:

```text
solicitation → amendment → award → contract → expenditure
```

The governing rule is: **follow the money without inventing the links.** Records are
connected only when the evidence supports the connection. Missing records, inaccessible
portals, ambiguous vendor names, contradictory amounts, and incomplete coverage remain
visible as explicit evidence gaps.

## Evidence discipline

The 0.0.1 and 0.0.2 disciplines carry forward unchanged:

- **Observed** — exact text in an identified public source, with URL, SHA-256, retrieval
  time, and citation.
- **Classified** — a deterministic state assigned because an exact cited phrase satisfies a
  published rule.
- **Compared** — a textual difference between two preserved source versions, never a legal
  conclusion.
- **Unknown** — anything outside the preserved record.

Procurement records add their own discipline: a source is only connected to another source
when the connection is exact and evidence-backed. Vendor appearance in a document is not
proof of procurement wrongdoing, and a technology purchase is not automatically
surveillance.

## Source authority

Every procurement source carries an explicit authority classification:

- **Authoritative procurement record** — the portal or ledger the City designates as the
  valid version for procurement purposes (for example, the Rocky Mountain E-Purchasing
  System / BidNet and Bonfire references named on the City's solicitation page).
- **Official informational mirror** — a City-hosted mirror that states it may be
  incomplete or outdated (for example, the City's solicitation list).
- **Official financial export** — an official budget or expenditure export (for example,
  OpenBook COS).
- **Official meeting or legislative record** — a City Council agenda, minutes, or matter.
- **Operator-supplied public record** — a public record obtained manually or through a
  Colorado Open Records Act (CORA) request and imported with a declared origin.
- **Unreviewed source** — a source that has not passed the persistent source-review gate.
- **Restricted or inaccessible source** — a source that cannot be lawfully automated (for
  example, a registration- or authentication-gated portal) or that is otherwise
  inaccessible.

An informational mirror is not an authoritative procurement system. A distinction is
preserved in the model: the City's solicitation page states its listings may be incomplete
or outdated, and that BidNet and Bonfire contain the authoritative versions. Panopticon
Null does not merge these.

## Coverage ledger

Every acquisition attempt records a persistent coverage ledger entry:

- source identity;
- retrieval timestamp;
- exact persisted-byte SHA-256 digest;
- HTTP metadata relevant to provenance (content encoding, ETag, Last-Modified, final
  public URL, redirect history);
- parser and schema version;
- claimed or observed date range;
- record count;
- pagination or export completion state;
- authority classification;
- access or parsing failures;
- completeness status;
- human review state.

Coverage states include `complete`, `partial`, `informational_only`, `access_blocked`,
`terms_unreviewed`, `schema_changed`, and `unknown`. The default is `unknown` or `partial`.
A source may be marked `complete` only when there is affirmative, reproducible evidence
that the checked snapshot enumerates the defined population.

Absence from a partial source is never proof of absence. User-facing language says:

> Not observed in the checked sources.

It never silently transforms that into "No contract exists."

## Immutable snapshots and change detection

Every fetched page, export, and document becomes an immutable source snapshot. If an
official URL later serves different bytes:

- the old snapshot is preserved;
- the new snapshot is preserved;
- the two are linked through a revision or supersession relationship;
- a deterministic record-level diff is produced;
- the old artifact and its derived observations are never rewritten.

A `304 Not Modified` response creates an acquisition/provenance event without duplicating
the artifact. The exact bytes that are persisted are hashed; content encoding, ETag,
Last-Modified, final public URL, and redirect history are recorded. Embedded links found
inside an ingested document are never automatically followed.

## Money

Money is never stored as a floating-point value. The raw amount string is preserved and a
parsed state distinguishes:

- exact stated amount;
- explicit zero;
- not applicable (`N/A`);
- various;
- IDIQ or ceiling amount;
- unknown;
- unparseable.

`N/A`, `various`, an omitted amount, and `$0.00` are never normalized into the same value.
Ambiguous currency formats are preserved and flagged, never silently coerced.

## Organizations

The source spelling of organization and vendor names is preserved. Normalization may
produce candidate aliases, but it must not automatically merge subsidiaries, parent
companies, similarly named firms, joint ventures, or individuals and organizations. A
non-exact match enters human review. Confirmed aliases carry provenance and review
history.

## Identifiers

All raw identifiers and their source are preserved: solicitation numbers, RFP/RFQ/IFB or
quote numbers, contract numbers, purchase-order numbers, invoice identifiers, and
legislative matter identifiers. Differently formatted identifiers are not assumed to be
identical without an explicit deterministic rule and tests.

## Reconciliation

Connections may be created automatically only through:

- exact normalized identifiers (where **both** endpoints resolve to a stored snapshot with a
  valid SHA-256 digest);

An "explicitly stated" relationship (a link declared directly by an official record) is
supported only when the link is evidence-backed: a declared reference field of one
preserved record contains an EXACT match of an identifier stored for another record, and
**both** endpoints resolve to stored snapshots with valid SHA-256 digests. The reference
field must be one the source adapter declares for that purpose (see "Official-relationship
links"); fields not declared are free-text and can never produce a link.

All other relationships require human confirmation. Records are never connected
automatically solely through similar vendor names, similar titles, equal dollar amounts,
close dates, keyword overlap, or an LLM judgment. A near-miss (non-exact) reference never
becomes a link automatically.

A reconciliation-review queue holds candidate identifier matches, vendor aliases,
conflicting award amounts, conflicting dates, duplicate or revised rows, missing documents,
and records that disappear from a later snapshot. Every accepted or rejected reconciliation
decision is immutable and auditable.

The chain builder's `Review suggestions` section is **not** the reconciliation queue: it lists
in-memory candidate suggestions derived at read time and does not persist them. Only the
`reconcile` command writes durable, auditable reconciliation items.

## Official-relationship links

A link between two preserved records is recorded only when an official record itself carries
the reference. Each source adapter may DECLARE reference fields — a fixed allowlist of
documented fields in which an official document may reference another official identifier
(for example, a council matter's referenced-matter field, an ordinance's numbered citation
of a solicitation number, an award row's notes field citing an ordinance number). Fields
not declared are free-text and can never produce a link.

A link is recorded only when a declared reference field of one preserved record contains an
EXACT match of an identifier stored for another record AND both endpoints resolve to stored
snapshots with valid SHA-256 digests. The stored link records: kind `official_relationship`,
both endpoint identifiers, the source snapshot id + digest of the record whose field carries
the reference, the exact quote and locator, and a citations pair (one per endpoint).

Published phrasing is a comparison, not a conclusion: "The preserved record X (snapshot,
digest) references Y in reference field Z." Never "X authorized Y" unless the preserved text
itself says so in a cited field — in that case quote the official text and label it as the
source's own statement. A near-miss (non-exact) reference becomes a CANDIDATE in the
reconciliation-review queue, never an automatic link. The case file and site render a
"documented relationships" section.

## Change alerts

Re-ingesting a reviewed surface that differs from the latest snapshot produces
deterministic, idempotent change alerts: `record_added`, `record_modified`,
`record_removed`. An award-row `record_modified` carries a field-level diff (field name,
old raw value, new raw value).

**Row identity rule.** A stable key identifies each row: the official identifier where
present; otherwise a SHA-256 digest over the row's normalized field values. The digest rule
is stable across row reorders because field order and values are normalized
deterministically before hashing.

Each alert records: source id, surface, old snapshot id + SHA-256, new snapshot id +
SHA-256, row identity, change kind, field-level diff (raw strings), retrieval timestamp,
coverage state, and affected procurement matter/identifiers when resolvable by the
exact-identifier rule (never by similarity).

**Phrasing discipline for removals.** A removal is a comparison, not a legal conclusion:
"The row observed in snapshot N (digest …) is not present in snapshot M (digest …)."

**Idempotent alert ids.** Alert ids are stable over source id + row identity + change kind +
old/new snapshot ids. Re-ingesting the same snapshot pair never creates a second alert; a
byte-identical re-ingest (304 path) creates no alerts.

**No accusation.** A change alert reports a change in the public record. If a row title or
vendor name matches the published surveillance taxonomy, it may appear only as optional
metadata "surveillance-related terminology observed, rule `<rule-id>`" — never "surveillance
purchase" or "surveillance award".

Alerts flow into the existing Alert store; `pnull alerts` lists both kinds; X drafts reuse
the existing pipeline verbatim.

## Publication

`pnull build-site` publishes the procurement chain from the same deterministic case-file
JSON as `case build`. Every citation on a procurement page requires an Approved
citation-review decision bound to the exact digests (the same mechanism as document pages).
A structured publication-allowlist category `procurement_casefile` is required for
procurement case-file content; an allowlist entry is not auto-approval. The privacy
backstop (plate labels, personal contact fields, SSNs, home-address patterns, coordinates,
movement logs) runs over ALL rendered procurement text, including vendor names and raw money
strings. Pending, rejected, stale, or mismatched review state, or a missing allowlist
category, removes the page/entry from the build with a visible "publication withheld pending
review" note, not a partial page. Published matters and procurement change alerts appear in
the Atom feed under the identical gates. `pnull procurement publish-ready <matter-id>`
reports gate state (pending citations, allowlist status, privacy-backstop results) without
publishing anything.

## Refresh

`pnull procurement refresh <source-id> [--live]` re-fetches a source. The dry-run default
makes zero network calls; live refresh requires the persistent source-review gate (refusing
on no review, expired review, config change, host change, or out-of-scope endpoint). It makes
one request at a time, uses DNS-safe HTTPS, sends a conditional request where an ETag exists,
and applies aggregate budgets. On refusal or failure it fails closed, states the reason, and
changes nothing.

Change detection in refresh compares the exact previous snapshot's **stored rows** loaded
from the database (see "Snapshot-row persistence"), not rows reconstructed from the source
fixture on disk. A refresh therefore stays correct across process restarts and fixture
deletion.

## Snapshot-row persistence

Every immutable procurement snapshot persists the exact parsed row set it captured, bound to
the snapshot id:

- `snapshot_rows` — one row per captured record with a stable `row_key`, normalized
  `canonical` fields, a deterministic per-row `row_digest` over those normalized fields, and
  the original `raw_json` value.
- `snapshot_row_sets` — completion metadata per snapshot (`expected_count`,
  `row_set_digest`, `parser_version`, `schema_version`) that distinguishes a valid zero-row
  capture from a legacy snapshot whose rows were never preserved.

Loading rows verifies the completion metadata, the per-row digests, the expected count, and
the row-set digest, and fails closed on any mismatch (corruption is never silently ignored).
This makes change detection compare immutable data stored for the exact previous snapshot,
never a reconstruction from fixtures, current source files, or a mutable cache.

**Coverage limitation.** Snapshots captured before v0.0.4c have no preserved rows; loading
them is reported as a documented evidence limitation and change detection against such a
legacy snapshot degrades to *no diff reported* rather than reconstructing history. Row
persistence is a commit-time integrity contract: re-persisting a snapshot id with different
rows fails closed instead of overwriting historical rows.

## Case file

A procurement matter produces a deterministic case file in machine-readable JSON and
human-readable Markdown containing:

- matter title and identifiers;
- current review and publication state;
- chronological event timeline;
- organizations in their documented roles;
- raw and parsed money values;
- exact evidence citations;
- source authority labels;
- contradictions;
- missing expected documents;
- coverage summary;
- retrieval and processing provenance;
- a SHA-256 manifest;
- a clear limitations section.

Every public factual statement must resolve to an existing reviewed citation. The case file
remains a draft until it passes the existing human citation-review and publication-allowlist
controls. Panopticon Null does not create or publish an X thread during this milestone.

## Gap-driven CORA

A command creates a local draft Colorado Open Records Act request from unresolved evidence
gaps. The draft identifies the institution or department, known solicitation or contract
identifiers, specific missing record types, a narrow date range, known vendor or project
name, and existing public sources already checked. It produces Markdown or plain text only,
never sends the request, never guesses an email recipient, and never claims a legal
deadline or entitlement unless supported by reviewed project documentation. It states that
operator/legal review is required and avoids requesting person-level data unless directly
necessary and lawfully justified. This turns missing evidence into a precise next
investigative action without pretending the evidence already exists.

## CORA request ledger

A fully local, append-only request ledger connects gaps → draft → submission → response →
gap update. It never sends a request, never guesses a recipient, and never claims a legal
deadline or entitlement. States are `drafted`, `submitted`, `response_received`,
`gap_resolved`, and `still_unresolved`. Transitions are immutable events carrying operator,
timestamp, and note; corrections are new events, and duplicate transitions and unknown
evidence ids are refused.

## Limits

This is not comprehensive procurement coverage. An informational mirror is not an
authoritative procurement system. Absence from checked sources is not proof of absence.
Vendor appearance is not proof of procurement wrongdoing. A technology purchase is not
automatically surveillance. OpenBook may not provide vendor-level payment evidence.
Restricted records may require a lawful CORA request. Panopticon Null does not provide
legal advice or a legal-compliance guarantee. The second-snapshot demonstration
(`fixtures/procurement/contract-awards-2.html`) is a labeled SYNTHETIC fixture derived from
the preserved official snapshot, not a live re-fetch; it is not an official record. Zero
official-relationship links are demonstrated in that demo — the absence is proven, not
asserted. Change detection compares exact stored rows from the database; the only remaining
coverage limitation is that legacy snapshots captured before v0.0.4c have no stored rows and
produce no diff until their rows are next captured.
