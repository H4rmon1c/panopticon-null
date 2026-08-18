# Panopticon Null

> **No human being is born to be indexed.**

Panopticon Null is lawful, nonviolent, evidence infrastructure for dismantling the surveillance panopticon. It makes acquisitions, promises, changes, and institutional actions visible without rebuilding person-level surveillance under a different operator.

> The machinery of mass surveillance depends on invisibility. This project records what is purchased, what is promised, what changes, and who authorized it.

Version 0.0.3 ("The Procurement Chain") stays deliberately narrow: one Colorado jurisdiction, one complete local-first pipeline, and no live posting. It builds a verifiable institutional money trail — solicitation → amendment → award → contract → expenditure — connecting official records only when the evidence supports the connection. Missing records, inaccessible portals, ambiguous vendor names, contradictory amounts, and incomplete coverage remain visible as explicit evidence gaps.

## What 0.0.3 does

0.0.3 turns isolated evidence receipts into a verifiable procurement chain under the rule **"follow the money without inventing the links."**

- **Source-authority model.** Every procurement source carries an explicit authority classification (authoritative procurement record, official informational mirror, official financial export, official meeting or legislative record, operator-supplied public record, unreviewed, or restricted/inaccessible). The City's solicitation page states its listings may be incomplete or outdated and that BidNet and Bonfire hold the authoritative versions; that distinction is preserved.
- **Persistent coverage ledger.** Every acquisition records source identity, retrieval timestamp, persisted-byte SHA-256, HTTP provenance metadata, parser/schema version, date range, record count, pagination/completion state, authority, failures, completeness, and human review state. Coverage states include `complete`, `partial`, `informational_only`, `access_blocked`, `terms_unreviewed`, `schema_changed`, and `unknown` (default `unknown`/`partial`). Absence from a partial source is never proof of absence; the phrasing is "Not observed in the checked sources."
- **Immutable source snapshots + change detection.** Every fetched page, export, and document becomes an immutable snapshot. If an official URL later serves different bytes, both snapshots are preserved and linked through a revision/supersession relationship with a deterministic record-level diff; old artifacts and derived observations are never rewritten. A `304 Not Modified` records provenance without duplicating the artifact. Embedded links are never auto-followed.
- **Procurement domain model.** Matters, events (solicitation published, amendment, award announced, contract executed, expenditure reported, record corrected/removed, and so on), identifiers (never merged across differing formats without a deterministic rule + tests), money (never floating point; raw string preserved; exact/zero/N/A/various/IDIQ/unknown/unparseable kept distinct), and organizations (source spelling preserved; non-exact matches enter human review; no automatic merging of subsidiaries, parents, joint ventures, or similarly named firms).
- **Bounded ingestion adapters.** The Colorado Springs contract-award table (with row-level provenance), the City solicitation mirror (carrying the source's own incompleteness warning), a documented negative capability finding for OpenBook COS (budget-level only, no vendor-level expenditure linkage), and a safe operator-supplied public-record import path that treats supplied files as hostile.
- **Reconciliation.** Connections are created automatically only through exact normalized identifiers, explicit official relationships, or existing evidence-backed relationships. Similar names/titles/amounts/dates/keywords/LLM judgment never connect records. A reconciliation-review queue holds candidate matches, vendor aliases, conflicting amounts/dates, duplicate/revised rows, missing documents, and vanished records; every decision is immutable and auditable.
- **Case files.** A procurement matter produces deterministic JSON + Markdown case files with a chronological timeline, organizations in documented roles, raw and parsed money, exact citations, source-authority labels, contradictions, missing documents, coverage, provenance, a SHA-256 manifest, and a limitations section. Files stay drafts until they pass the human citation-review and publication-allowlist controls.
- **Gap-driven CORA drafts.** A command generates a local, unsent Colorado Open Records Act draft from unresolved gaps (institution, identifiers, missing record types, narrow date range, vendor/project name, sources already checked). It never sends, never guesses an email recipient, and states that operator/legal review is required.
- **A real case study + a benign control.** The Next-Generation Transit Fare Collection System RFI (R26-023AB) is ingested as an RFI (not an award or contract), with no mass-surveillance labeling and an explicit gap where no executed contract or payment was located. A benign control matter (Crack Seal Materials award) proves ingestion does not automatically turn every purchase into a surveillance accusation.
- **Hostile-input tests + CSV safety.** Malformed/deeply nested HTML, unexpected columns, duplicate/reordered rows, Unicode and hostile vendor names, huge numbers, currency ambiguity, broken CSV quoting, and source schema drift are exercised. CSV exports neutralize spreadsheet-formula injection.

## What 0.0.2 does

0.0.2 adds a page-accurate, cryptographically verifiable receipt for every public claim. Each observation is bound to the exact bytes, page, and quoted region of a preserved official document, and each step of the pipeline records immutable provenance.

- **Page-accurate PDF citations.** Immutable text maps record, per page, the evidence ID, page number, page width/height, rotation, the coordinate system (`pdf_user_space_points_bottom_left_y_up`), every extracted word and its bounding box, the extractor and version, a text-map digest, and its relation to the source digest. Bounding rectangles are validated (negative coordinates, inverted rectangles, out-of-bounds regions, missing pages, and quote mismatches are rejected). `pnull citation show` and `pnull citation render` display and highlight the exact quoted region. OCR uses deterministic Tesseract TSV with a pixel-to-page transform; OCR confidence is metadata, never proof.
- **Immutable processing-run provenance.** Every ingestion and reprocessing job records schema version, Panopticon Null version, source revision, rules digest, state-config digest, input evidence IDs, native tool names and versions (Poppler, Tesseract, bubblewrap, prlimit), sandbox backend and version, configured budgets, actual aggregate resource consumption, start/end timestamps, outcome, structured errors, and output artifact IDs and digests. These are supplemental records linked to existing evidence IDs; v0.0.1 records are never rewritten.
- **A real extraction sandbox with aggregate budgets.** PDF and OCR tools run inside a Linux bubblewrap sandbox with no network namespace, no inherited secrets, and no writable access outside a dedicated temporary output directory. Inputs are staged read-only; CPU, memory, file size, output size, and wall-time limits are applied via `prlimit`; and process trees are contained in an isolated PID namespace torn down on success, failure, timeout, or interrupt. Live PDF/OCR ingestion fails closed when the sandbox cannot be established. Aggregate job budgets cap total downloaded bytes, attachments, PDF pages, OCR pages, extracted bytes, child processes, CPU allowance, and wall-clock allowance.
- **DNS-safe HTTP provenance and conditional retrieval.** For every request and redirect, the system persists the requested URL, resolved public IPs, retrieval timestamp, method, status code, redirect target, final URL, allowlisted headers, content type, content length, ETag, Last-Modified, and response-body digest. Cookies, authorization headers, and bearer tokens are never persisted. Loopback, private, link-local, multicast, unspecified, documentation, and other non-public addresses are rejected, and mixed public/prohibited DNS answers fail closed. HTTPS is mandatory and certificate validation is never disabled. Conditional requests use `If-None-Match`/`If-Modified-Since`; a `304` creates a fetch observation referencing previous preserved evidence rather than a new blob.
- **Persistent robots/terms review.** Live retrieval now requires a persistent, expiring human source review rather than the ephemeral `--robots-reviewed` flag (which is deprecated). `pnull source review capture/record/show/verify` persists immutable review artifacts, and live retrieval refuses when there is no review, the review has expired, the source configuration changed, allowed hosts changed, the endpoint is outside the reviewed scope, or a prior restriction requires renewed review.
- **Bounded Legistar pagination and attachment discovery.** One request at a time, configurable page size, hard maximums on pages, events, matters, and attachments per matter, aggregate byte and time budgets, deduplication by official identifiers, repeated-page/non-progressing detection, deterministic ordering, conditional requests with cached observations, and fail-closed handling of malformed identifiers or unknown hosts. Discovery uses only documented official fields and reviewed hosts.
- **Explicit subjects, actions, and document roles.** Versioned domain types model the subject of an action (Ordinance, Policy, Solicitation, Contract, Amendment, Vendor, SurveillanceTechnology, Program, BudgetItem, Other, Unknown), the action (Mentioned, Proposed, HearingScheduled, VoteScheduled, Approved, Rejected, Awarded, Executed, Amended, Renewed, Expanded, DeploymentReported, PolicyChanged, Unknown), and the document role (Agenda, Minutes, Ordinance, Policy, Solicitation, Award, Contract, Amendment, StaffReport, Presentation, Other). Every action identifies its exact subject and citations. A regression test proves an Ordinance 25-93 approval can never become an Axon/Flock purchase assertion.
- **A human citation-review queue.** Append-only review states (Pending, Approved, Rejected, NeedsContext, Superseded). Each decision binds to the exact digest of the evidence ID, source digest, locator/geometry, quote, quote digest, rule digest, processing artifact digest, and proposed public fields. Changing any bound value invalidates the approval. Structured publication allowlists name which categories may appear publicly; an allowlist is not auto-approval. The static site, Atom feed, and X drafting fail closed on pending, rejected, stale, or mismatched decisions. The demo uses clearly labeled deterministic demonstration reviews.
- **Safe reconciliation of uncertain X threads.** Dry-run is the default. `pnull x attempts`, `pnull x status`, and `pnull x reconcile` record append-only operator decisions (confirm a segment exists plus remote ID/URL, confirm none posted, mark partially posted, abandon, or authorize a new attempt only after the previous one is resolved). There is no blind retry of uncertain attempts and no audit-history deletion.
- **A second genuine Colorado Springs matter.** Ordinance No. 15-84 (2015), matter 15-00663, established the municipal court Information Technology Surcharge that Ordinance 25-93 (the v0.0.1 matter) amends. The surveillance-technology link (Axon body cameras/evidence systems/AI transcription, Flock vehicle-intelligence cameras) is documented in the preserved 2025 presentation as supporting evidence, not asserted by the 2015 action itself.

### The 0.0.1 foundation

0.0.1 established the local-first vertical slice that 0.0.2 builds on:

- Preserves original public bytes by SHA-256 in a local content-addressed evidence directory.
- Extracts static HTML, UTF-8 text, text PDFs, and optional OCR PDFs under size, page, process, and time limits.
- Parses official Legistar event JSON, including expanded agenda items.
- Applies a published YAML surveillance taxonomy and stores exact normalized-text line citations.
- Detects prices, durations, retention terms, data-sharing terms, vendors, dates, scope, and relevant removals.
- Stores durable state in SQLite and prevents duplicate evidence, findings, alerts, and X attempts.
- Builds a stark, accessible, JavaScript-free static site and Atom feed.
- Produces citation-constrained Colorado X drafts. Approval is bound to the exact draft digest; posting additionally requires explicit confirmation, runtime credentials, and a real canonical URL.
- Runs a complete offline demonstration against preserved official fixtures. No X transport is constructed by the demo or tests.

## Epistemic boundaries

Every result separates four things:

| Layer | Meaning |
|---|---|
| **Observed** | Exact text in an identified public source, with URL, SHA-256, retrieval time, extraction method, and line citation. In 0.0.2 this extends to page-accurate citations with validated geometry. |
| **Classified** | A deterministic state assigned because an exact cited phrase satisfies a published rule. Ambiguous or conflicting phrases resolve to `Unknown` or `Mention detected`. |
| **Compared** | A textual difference between two preserved source versions. It is not a legal conclusion. |
| **Unknown** | Legality, implementation outside the record, effectiveness, intent, completeness of the portal, and any unstated contract term. |

A keyword never proves a purchase. Approval of Ordinance 25-93 establishes approval of that ordinance; it does **not** by itself establish approval of an Axon or Flock purchase. The supporting presentation establishes that those systems appeared in the same public matter and states listed costs. The project does not infer beyond those sources.

0.0.2 sharpens this discipline with explicit subject/action modeling: an action is bound to exactly one subject and its citations, so a vendor mention in a supporting document can never be promoted to a procurement assertion. Human citation review is mandatory before any public citation, image excerpt, or X draft, and free-text reviewer notes are never published automatically.

## Compile

### Reproducible Nix environment

```console
nix --extra-experimental-features 'nix-command flakes' develop
cargo build --workspace --all-features --locked
```

Or build directly:

```console
nix --extra-experimental-features 'nix-command flakes' build
./result/bin/pnull --help
```

The flake pins Nixpkgs, the Rust overlay, Rust 1.89.0, and the RustSec advisory database. Poppler, Tesseract, `bubblewrap`, and `prlimit` come from Nix; tests never download executables.

### Without Nix

Install Rust 1.89.0, Cargo, Poppler (`pdfinfo`, `pdftotext`, `pdftoppm`), Tesseract with at least one language, bubblewrap (`bwrap`), and `prlimit`, then run:

```console
cargo build --workspace --all-features --locked
```

Nix is the supported reproducible path.

## Run

Run the complete offline vertical slice:

```console
cargo run --locked -p pnull-cli -- demo
# Open demo-output/site/index.html directly in a browser.
```

The demo runs offline against preserved official fixtures, exercises the v0.0.1→v0.0.2 migration, generates page-accurate citations, models explicit subjects and actions for two genuine matters, requires deterministic demonstration review decisions, generates a JavaScript-free static site and Atom feed, and produces only dry-run X drafts. It performs zero network posts and constructs no live X transport.

Common commands:

```console
cargo run --locked -p pnull-cli -- source list
cargo run --locked -p pnull-cli -- source review capture colorado-springs-legistar-events
cargo run --locked -p pnull-cli -- source review record colorado-springs-legistar-events --reviewer NAME --note NOTE --expires 2027-08-16
cargo run --locked -p pnull-cli -- source review show colorado-springs-legistar-events
cargo run --locked -p pnull-cli -- source review verify colorado-springs-legistar-events
cargo run --locked -p pnull-cli -- ingest --source colorado-springs-legistar-events
cargo run --locked -p pnull-cli -- matter list
cargo run --locked -p pnull-cli -- matter show <matter-id>
cargo run --locked -p pnull-cli -- matter attachments <matter-id>
cargo run --locked -p pnull-cli -- scan
cargo run --locked -p pnull-cli -- diff
cargo run --locked -p pnull-cli -- citation show <citation-id>
cargo run --locked -p pnull-cli -- citation render <citation-id> --output review.png
cargo run --locked -p pnull-cli -- review list
cargo run --locked -p pnull-cli -- review show <citation-id>
cargo run --locked -p pnull-cli -- review approve <citation-id> --reviewer NAME --note NOTE
cargo run --locked -p pnull-cli -- review reject <citation-id> --reviewer NAME --reason REASON
cargo run --locked -p pnull-cli -- review supersede <decision-id> --reviewer NAME --reason REASON
cargo run --locked -p pnull-cli -- build-site --output site
cargo run --locked -p pnull-cli -- alerts
cargo run --locked -p pnull-cli -- verify <evidence-id>
cargo run --locked -p pnull-cli -- x attempts
cargo run --locked -p pnull-cli -- x status <alert-id>
cargo run --locked -p pnull-cli -- x reconcile <attempt-id> --decision confirm_posted --operator NAME --note NOTE --remote_id ID
cargo run --locked -p pnull-cli -- x draft <alert-id>
cargo run --locked -p pnull-cli -- x approve <alert-id>
```

Procurement chain commands (all offline by default; live retrieval requires an explicit
live mode and an approved persistent source review):

```console
cargo run --locked -p pnull-cli -- procurement ingest solicitations
cargo run --locked -p pnull-cli -- procurement ingest awards
cargo run --locked -p pnull-cli -- procurement ingest openbook
cargo run --locked -p pnull-cli -- procurement import <path>
cargo run --locked -p pnull-cli -- procurement reconcile <matter>
cargo run --locked -p pnull-cli -- procurement show <matter>
cargo run --locked -p pnull-cli -- procurement gaps <matter>
cargo run --locked -p pnull-cli -- procurement export-awards --output awards.csv
cargo run --locked -p pnull-cli -- coverage show
cargo run --locked -p pnull-cli -- coverage diff <old-snapshot> <new-snapshot>
cargo run --locked -p pnull-cli -- case build <matter>
cargo run --locked -p pnull-cli -- cora draft <matter>
```

Live source retrieval is refused unless the operator has recorded a persistent, current, in-scope source review (see `docs/operator-guide-source-review.md`). The ephemeral `--robots-reviewed` flag is deprecated and is no longer the primary authorization. The configured 24-hour interval is persisted and enforced. The source uses one request at a time; it does not bypass authentication, CAPTCHAs, access controls, or restrictions.

### X safety model

Drafting is always local and dry-run by default. `x approve` hashes and approves the exact generated post or thread. A live attempt requires all of the following:

1. A real public `canonical_base_url` in `configs/states/co.toml` (the repository default is intentionally `.invalid`).
2. An approved, unchanged draft digest.
3. `X_BEARER_TOKEN` or `PNUL_X_SECRET_FILE` pointing to a mode-`0600` token file.
4. `pnull x post <alert-id> --confirm`.

Tests use only a fake transport. An attempt is reserved before network activity, each successful thread segment is stored immediately, and uncertain partial attempts cannot be blindly retried. If a live attempt becomes uncertain, inspect it with `pnull x status` and resolve it with `pnull x reconcile` before any new attempt is allowed. No audit history is ever deleted.

## Validate

```console
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
nix --extra-experimental-features 'nix-command flakes' flake check --print-build-logs
```

Fixture integrity (v0.0.1, v0.0.2, and v0.0.3 procurement fixtures):

```console
sha256sum -c fixtures/co/SHA256SUMS          # run from the repository root
(cd fixtures/co2 && sha256sum -c SHA256SUMS)        # co2 uses bare filenames
(cd fixtures/procurement && sha256sum -c SHA256SUMS)  # procurement uses bare filenames
```

The two fixture SUMS files verify that every preserved official byte under `fixtures/co/`
and `fixtures/co2/` is intact and unmodified, and `fixtures/procurement/SHA256SUMS` verifies
the v0.0.3 procurement fixtures.

`cargo deny check` is provided by the pinned Nix environment; it runs inside
`nix flake check` as the `dependency-policy` check.

The offline demo is reproduced byte-for-byte across two clean output directories (site,
`state/records`, and the procurement chain output), and the proof file
`network-posts.txt` is `0`.

## Privacy boundary

Raw evidence and SQLite state are created under a private local directory. Public output includes only institutional facts, selected citations, and provenance. Publication fails closed on recognized plate labels, personal contact fields, Social Security numbers, home-address patterns, coordinates, and movement-log fields. Detection is a backstop, not permission to publish arbitrary free text; operators must review all citations before distributing a site, publishing an image excerpt, or approving an X draft.

In 0.0.2, every public citation and image excerpt passes a human citation-review gate, image excerpts require a separate review decision, and free-text reviewer notes are never published automatically. HTTP provenance never leaks cookies, authorization headers, or bearer tokens.

No facial recognition. No person-level movement analysis. No dossiers on activists, officers, employees, or residents. No harassment, doxxing, unauthorized access, evasion, or physical interference.

## Repository map

- `pnull-core`: canonical records, IDs, SQLite schema + migration, digest verification, subjects/actions/document roles, review decisions, processing-run provenance, source reviews, fetch observations, X attempts/reconciliations, publication allowlists.
- `pnull-ingest`: lawful retrieval, Legistar parsing, bounded pagination, the bubblewrap sandbox, aggregate job budgets, Poppler/Tesseract orchestration, page-accurate citation construction.
- `pnull-detect`: YAML rules, cautious classification, exact citations, meaningful diffs, explicit subject/action extraction.
- `pnull-publish`: privacy-gated static HTML and Atom, citation-review gates, publication allowlists.
- `pnull-x`: state-aware drafts, exact-draft approval, attempts/status/reconcile, transport trait, redacted credentials.
- `pnull-geometry`: page-accurate PDF citation geometry, text maps, OCR pixel-to-page transforms, reviewer-image rendering.
- `pnull-http`: DNS-safe HTTP provenance, conditional retrieval, credential redaction, resolver/transport abstractions.
- `pnull-procurement`: the procurement chain — source-authority/coverage ledger, immutable snapshots + change detection, awards/solicitation/openbook/import adapters, reconciliation, case-file generation, gap-driven CORA drafts, and the offline procurement demo.
- `pnull-cli`: commands and the offline vertical slice.

See `docs/architecture.md`, `docs/methodology.md`, `docs/procurement-methodology.md`,
`docs/source-adapters.md`, and `docs/0.0.3-source-survey.md` for details.

## License

GNU Affero General Public License v3.0 or later.
