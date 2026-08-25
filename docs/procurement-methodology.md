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

An "explicitly stated" relationship (a link declared directly by an official record) is **not
yet supported** — the capability is unimplemented and is not advertised as working. An
existing immutable relationship already supported by evidence is likewise carried through
only when that evidence resolves to exact, digest-bound snapshots.

All other relationships require human confirmation. Records are never connected
automatically solely through similar vendor names, similar titles, equal dollar amounts,
close dates, keyword overlap, or an LLM judgment.

A reconciliation-review queue holds candidate identifier matches, vendor aliases,
conflicting award amounts, conflicting dates, duplicate or revised rows, missing documents,
and records that disappear from a later snapshot. Every accepted or rejected reconciliation
decision is immutable and auditable.

The chain builder's `Review suggestions` section is **not** the reconciliation queue: it lists
in-memory candidate suggestions derived at read time and does not persist them. Only the
`reconcile` command writes durable, auditable reconciliation items.

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

## Limits

This is not comprehensive procurement coverage. An informational mirror is not an
authoritative procurement system. Absence from checked sources is not proof of absence.
Vendor appearance is not proof of procurement wrongdoing. A technology purchase is not
automatically surveillance. OpenBook may not provide vendor-level payment evidence.
Restricted records may require a lawful CORA request. Panopticon Null does not provide
legal advice or a legal-compliance guarantee.
