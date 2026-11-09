# Operator guide — procurement chain

This guide covers the v0.0.3 and v0.0.4 procurement commands. The full model is in
`docs/procurement-methodology.md`; the surveyed sources are in `docs/0.0.3-source-survey.md`.

## Principles

- **Follow the money without inventing the links.** Records are connected only when the
  evidence supports the connection.
- **Not observed is not "does not exist."** Absence from a partial source is never proof of
  absence; the language is "Not observed in the checked sources."
- **Informational mirrors are not authoritative.** The City's solicitation list may be
  incomplete or outdated; BidNet and Bonfire hold the authoritative versions but are not
  automated.
- **A technology purchase is not automatically surveillance.** Ingestion does not accuse.

## Offline demonstration

```console
cargo run --locked -p pnull-cli -- demo
```

This runs entirely offline against the preserved fixtures under `fixtures/procurement/`,
verifies the fixture digests, ingests the solicitation mirror and contract-award snapshots,
records the OpenBook negative capability finding, builds the transit-fare RFI matter and the
benign control matter, generates their case files and a local unsent CORA draft, and exports
a formula-neutralized `procurement/awards.csv`. It performs zero network posts.

The v0.0.4 demonstration additionally re-ingests the second contract-award snapshot
`fixtures/procurement/contract-awards-2.html` (a labeled SYNTHETIC demonstration fixture
derived from the preserved official snapshot — it edits one amount and one vendor name and
adds notes on `Q25-130ZM`, removes `R24-T114JD`, and adds `R25-044AB`). This exercises
snapshot supersession, the record-level diff, change alerts, `RecordCorrected`/`RecordRemoved`
events, and the "what changed" section. The demo publishes the procurement pages/Atom feed,
produces one dry-run X draft for a procurement change alert, and registers the transit-fare
CORA request in `drafted` state. `network-posts.txt` stays at 0, and the output is
byte-for-byte reproducible across two clean directories (`site/`, `state/records/`,
`procurement/`).

The demo store is created under `<output>/state/pnull.db`. Commands below can be run against
a demo store, for example:

```console
cargo run --locked -p pnull-cli -- --data-dir <output>/state procurement show <matter>
```

## Commands

### Ingestion (offline default)

```console
pnull procurement ingest solicitations
pnull procurement ingest awards
pnull procurement ingest openbook
pnull procurement import <path>
```

- `ingest solicitations` and `ingest awards` parse the preserved snapshots into records with
  row-level provenance. The solicitation records carry the source's incompleteness warning.
- `ingest openbook` records the documented negative capability finding: OpenBook COS is a
  budget-level export that does not provide vendor-level expenditures. Payment evidence from
  this source is represented as unavailable; no relationship is invented.
- `import <path>` ingests an operator-supplied public record. You must provide a declared
  source URL or records-request identifier, the acquisition date, the document role, and a
  declaration of lawful possession. The file is treated as hostile and passes through the
  existing sandbox and resource limits, and requires human review before publication.

Live retrieval of any procurement surface requires an explicit live mode **and** an approved
persistent source review (see `docs/operator-guide-source-review.md`). Default demonstrations
are fully offline.

### Reviewing a matter

```console
pnull procurement show <matter>
pnull procurement gaps <matter>
pnull procurement reconcile <matter>
```

- `show` prints the matter's records, events, organizations, money, citations, coverage, and
  limitations.
- `gaps` lists unresolved evidence gaps (missing expected documents, unlocated records).
- `reconcile` lists the reconciliation-review queue for the matter. Connections that are not
  exact are never automatic; they wait for an operator decision.

To record an immutable reconciliation decision:

```console
pnull procurement reconcile <matter> --item <item-id> --decision <accepted|rejected> --operator <name> --note <note>
```

Every decision is append-only and auditable; it binds to the exact item and decision inputs.

### The linked record and official-relationship links (v0.0.4A)

```console
pnull procurement chain <matter>
pnull procurement reconcile <matter>
```

`chain` prints the ordered procurement lifecycle for a matter —

`solicitation -> amendment -> award -> contract -> expenditure`

— as a deterministic linked record. Each stage shows the observed records, and every link
carries digest-bound evidence for **both** endpoints: the event id, source id, snapshot id,
and exact SHA-256 digest of the snapshot each record was ingested from. The connection can
therefore be traced to immutable, digest-bound evidence. Missing stages render as
`Not observed`, never as proof that no record exists.

A link is created only when **both** endpoints resolve to a stored, immutable source snapshot
with a valid SHA-256 digest, and the normalized procurement identifiers match exactly.
Similar, incomplete, or ambiguous identifiers are never auto-linked; they are surfaced as
**Review suggestions** (in-memory candidates, not yet persisted reconciliation items). If
either endpoint of a candidate link lacks digest-bound evidence, the link is not created and
an explicit evidence gap is rendered instead. A newer snapshot for a source never retroactively
rebinds an existing link — each event stays bound to the exact snapshot it was ingested from.

**Official-relationship links (v0.0.4, Item 5).** Source adapters may declare reference
fields — a fixed allowlist of documented fields in which an official document may reference
another official identifier. Fields not declared are free-text and can never produce a link.

A link of kind `official_relationship` is recorded only when a declared reference field of one
preserved record contains an **exact** match of an identifier stored for another record, and
both endpoints resolve to stored snapshots with valid SHA-256 digests. The published phrasing
is: "The preserved record X (snapshot, digest) references Y in reference field Z." The case
file and the published site render a "documented relationships" section.

A near-miss (non-exact) reference becomes a **candidate** in the reconciliation-review queue —
it is never an auto-link. `pnull procurement reconcile <matter>` (documented under
"Reviewing a matter") accepts a reviewed link decision for these candidates.

> **Honest note.** In the current demo, **zero** official-relationship links are recorded: no
> preserved record carries an explicit procurement-identifier reference in a declared
> reference field. The demo proves that absence rather than fabricating a link. An operator
> should therefore not be surprised to see no `official_relationship` links in the output.

### Coverage

```console
pnull coverage show
pnull coverage diff <old-snapshot> <new-snapshot>
```

`coverage show` prints the coverage ledger. `coverage diff` produces a deterministic
record-level diff between two snapshots, which is how changed or removed official records
remain historically inspectable. Coverage defaults to `unknown`/`partial`; a source is
`complete` only with affirmative, reproducible evidence.

### Case files

```console
pnull case build <matter>
pnull cora draft <matter>
```

`case build` writes a deterministic case file (JSON + Markdown) with the chronological
timeline, organizations in their documented roles, raw and parsed money, exact citations,
source-authority labels, contradictions, missing documents, coverage, provenance, a SHA-256
manifest, and a limitations section. The case file remains a draft until it passes the human
citation-review and publication-allowlist controls.

`cora draft` generates a local, unsent Colorado Open Records Act draft from the matter's
unresolved gaps. It never sends the request, never guesses an email recipient, never claims a
legal deadline or entitlement, and states that operator/legal review is required. As of
v0.0.4, `cora draft` also registers the request in `drafted` state in the request ledger (see
"CORA request ledger" below).

## Change alerts

```console
pnull procurement alerts
```

`procurement alerts` lists the procurement change alerts for the store. Each alert carries the
old and new snapshot id with its SHA-256 digest, the row identity, the change kind, the
field-level diff, the retrieval timestamp, and the coverage state of the new snapshot. Change
kinds include `RecordCorrected`, `RecordRemoved`, and additions; a removed row is phrased as
"The row observed in snapshot N (digest …) is not present in snapshot M (digest …)."

Alerts are idempotent: the same snapshot-to-snapshot transition produces the same alert
deterministically, and re-running against an unchanged store changes nothing.

**Snapshot-row persistence (v0.0.4c).** Every snapshot now stores the exact parsed row set it
captured (`snapshot_rows` + `snapshot_row_sets`). Change detection compares the exact previous
snapshot's stored rows from the database, so alerts remain correct across process restarts and
fixture deletion, and they stay bound to the old and new snapshot ids with both digests. Alerts
created against an earlier snapshot are never rebound to a later one. Snapshots captured before
v0.0.4c have no stored rows and produce no diff until their rows are next captured.

The general `pnull alerts` command now lists **both** the v0.0.1 taxonomy alerts and the
procurement change alerts in one listing, so an operator sees every alert in a single place.

## Refresh

```console
pnull procurement refresh <source-id> [--live]
```

`refresh` is the exposure heartbeat for a procurement surface. `--dry-run` is the **default**:
it prints exactly what would be fetched and compared, with **zero** network activity.

`--live` requires the source to pass the persistent source-review gate. The gate refuses on:
no review, an expired review, a configuration change, a host change, or an out-of-scope
endpoint. The live path fetches the surface with one request at a time, DNS-safe HTTPS, a
conditional request where an ETag exists, and aggregate budgets. It then records a new snapshot
(or a 304 provenance when nothing changed), runs change detection, prints the alert count and
the affected matter ids, and writes a coverage-ledger entry.

On any refusal or failure the live path **fails closed**: it states the reason and changes
nothing. The two reviewed procurement surfaces (the contract-award table and the solicitation
mirror) are already configured; the offline demo never invokes `refresh`.

Change detection in refresh loads the exact previous snapshot's rows from the database (see
"Change alerts" above) and never re-reads the source fixture from disk, so a refresh stays
correct even if the fixture is deleted or the process restarts between snapshots.

## CORA request ledger

```console
pnull cora list [--matter <matter-id>]
pnull cora show <request-id>
pnull cora submit <request-id> --operator NAME --date YYYY-MM-DD --tracking REF [--recipient-note TEXT]
pnull cora response <request-id> --evidence-id EID [--note TEXT]
```

The request ledger is **append-only and fully local**. It connects `procurement gaps` → CORA
draft → human submission → response import → case-file gap update. `list` shows requests
(optionally filtered by matter), and `show` prints one request's full history.

Each request passes through the states `drafted`, `submitted`, `response_received`,
`gap_resolved`, and `still_unresolved`. Every transition is an immutable event with an
operator, a timestamp, and a note. No transition may be reversed or edited; corrections are
recorded as new events.

The tool **never sends anything**, never guesses a recipient, and never claims a legal
deadline or entitlement. `submit` records operator-supplied facts about an action the human
performed (recipient, date, tracking reference): the tool stores them, it does not perform
them. `response` links imported evidence (via the existing hostile-file import path) to the
request; `--evidence-id` must reference evidence already present.

When a response covers a gap, the matter's case-file gap section updates to show the citation
and the request that closed it (`gap_resolved`). When the response does not cover the gap, the
gap remains visible with the response's digest noted (`still_unresolved`).

## Publishing the procurement chain

```console
pnull build-site
pnull procurement publish-ready <matter-id>
```

`pnull build-site` publishes the procurement chain from the **same** deterministic case-file
JSON that `case build` produces. The site layout is `/co/procurement/index.html` (a matter
list with coverage states and "Not observed in the checked sources" phrasing) and
`/co/procurement/<matter-slug>/index.html` per matter. Each matter page renders the timeline,
roles, raw and parsed money, citations (with source-authority labels), contradictions, missing
documents/coverage gaps, a "what changed" section from snapshot supersessions, provenance, the
SHA-256 manifest, and a limitations block.

The publish gates **fail closed**. Every citation needs an Approved citation-review bound to
exact digests; a `procurement_casefile` publication-allowlist category is required (an
allowlist entry is not auto-approval); and the privacy backstop runs over all rendered text,
including vendor names and raw money strings. Pending, rejected, stale, or mismatched
citations withhold the page with a visible "publication withheld pending review" note — never
a partial page. Published matters and change alerts appear in the Atom feed under identical
gates.

The demo publishes the transit-fare page
(`/co/procurement/proc_matter_co_r26-023ab/`), the benign control matter
(`/co/procurement/proc_matter_co_crack-seal-2023/`, a restraint demonstration with no
surveillance labeling), and a derived "what changed" matter
(`/co/procurement/proc_matter_co_q25130zm/`) carrying the synthetic label.

`pnull procurement publish-ready <matter-id>` is the operator's **pre-publish checklist**. It
reports gate state for a matter **without publishing anything**: pending citations, the
publication-allowlist status (whether the `procurement_casefile` category is present), and the
privacy-backstop results.

## Verification

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
(cd fixtures/procurement && sha256sum -c SHA256SUMS)
nix --extra-experimental-features 'nix-command flakes' flake check --print-build-logs
```

`cargo deny check` is provided by the pinned Nix environment and runs inside `nix flake
check` as the `dependency-policy` check.
