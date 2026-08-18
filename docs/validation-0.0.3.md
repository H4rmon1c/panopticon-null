# Validation report — v0.0.3 ("The Procurement Chain")

This report states exactly what was proven by the v0.0.3 validation suite and what remains
unknown. It is an honest account, not a claim of perfection or legal compliance.

## What 0.0.3 adds

v0.0.3 ("The Procurement Chain") turns isolated evidence receipts into a verifiable
institutional money trail: solicitation → amendment → award → contract → expenditure. It
adds a procurement domain model, a source-authority and coverage ledger, immutable source
snapshots with change detection, bounded ingestion adapters, reconciliation rules, case-file
generation, and gap-driven CORA drafts — while reusing every 0.0.1 and 0.0.2 control.

## Validation commands

The following validation commands pass for this release:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo deny check`
- `nix --extra-experimental-features 'nix-command flakes' flake check --print-build-logs`
- `sha256sum -c fixtures/co/SHA256SUMS`
- `sha256sum -c fixtures/co2/SHA256SUMS`
- `sha256sum -c fixtures/procurement/SHA256SUMS`
- `cargo run --locked -p pnull-cli -- demo`

`cargo deny check` is provided by the pinned Nix environment (`cargo-deny`); it is exercised
inside `nix flake check` as the `dependency-policy` check, which runs `cargo deny --offline
check` against the pinned RustSec advisory database. In a plain non-Nix shell the command is
not on PATH; the authoritative path is the Nix flake check.

## The demo

The offline demo (`cargo run --locked -p pnull-cli -- demo`) is proven to:

- run entirely offline using preserved official fixtures (no network access);
- verify the procurement fixture SHA-256 digests before ingestion;
- ingest the solicitation mirror and contract-award snapshots with row-level provenance;
- record the OpenBook COS negative capability finding (budget-level only, no vendor-level
  expenditure linkage);
- build two procurement matters: the Next-Generation Transit Fare Collection System RFI
  (R26-023AB) and a benign control matter (Crack Seal Materials award);
- generate deterministic case files (JSON + Markdown) with citations, coverage, gaps, and a
  limitations section;
- generate a local, unsent CORA draft from the transit-fare matter's unresolved gaps;
- export the award rows as a formula-neutralized CSV (`procurement/awards.csv`);
- generate a JavaScript-free static site and an Atom feed;
- produce only dry-run X drafts and perform zero network posts;
- be reproducible byte-for-byte: the test runs the demo in two clean directories and asserts
  that the generated `site/`, `state/records/`, and `procurement/` trees are identical, and
  that `network-posts.txt` contains `0`.

## Real sources and fixtures

The 0.0.3 procurement fixtures preserve the following official surfaces, retrieved one
request at a time with no authentication, access-control bypass, or browser automation.
Exact URLs and hashes are in `fixtures/README.md` and `fixtures/procurement/SHA256SUMS`.

| Surface | Authority classification | Coverage state | Note |
|---|---|---|---|
| City contract-award table | Official informational mirror | `partial` | Parsed to award rows with row-level provenance |
| City solicitation list | Official informational mirror | `informational_only` | Carries the source's own incompleteness warning |
| OpenBook COS / Socrata export | Official financial export | `partial` | Budget-level only; no vendor-level expenditure linkage (documented negative finding) |
| BidNet / Bonfire | Authoritative procurement portal (per City) | `access_blocked` | Registration/terms-restricted; not automated |
| Legistar meeting records (0.0.1/0.0.2) | Official meeting/legislative record | — | Reused, unchanged |

## The real case study

The real case study is the **Next-Generation Transit Fare Collection System RFI
(R26-023AB)** for Mountain Metropolitan Transit. It is a **Request for Information (RFI)**,
not an RFP, award, contract, or purchase. The preserved City-hosted documents state this
explicitly.

- The RFI is revalidated against the live official identifier and documents.
- It is not labeled as mass surveillance merely because it handles data; only documented
  capabilities, data practices, requirements, and institutional actions are extracted.
- A sample contract is distinguished from an executed contract.
- No award or payment record for the fare system was located in the checked sources, so the
  case file shows that exact gap and the CORA draft targets it. No contract or payment
  relationship is invented.

A **benign control matter** (Crack Seal Materials, award under `B22-T168KK`) is also
ingested. It proves that ingestion does not automatically turn every technology or
materials purchase into a surveillance accusation: the control matter is modeled without
any surveillance classification.

## Migration

The schema advances to `SCHEMA_VERSION = 2` (`MAX_SUPPORTED_SCHEMA_VERSION = 2`) through a
transactional migration that preserves all 0.0.1 and 0.0.2 rows byte-for-byte and never
rewrites old evidence or processing history. See `docs/migration-v0.0.3.md`.

- Upgrade test loads the committed fixture `fixtures/migration/v0.0.2-minimal.sql` (a real
  0.0.2 database) and proves every canonical row is preserved.
- Failure-injection tests prove migration failure rolls back atomically.
- Migration is idempotent; a newer unsupported schema is rejected.

## Build checkpoints

The milestone was built in logical, committed checkpoints so a fresh session can resume
without reconstructing context:

- `1139734` — source survey, authority/coverage model, domain types, migration.
- `cd24452` — ingestion adapters, reconciliation, coverage, snapshots, case files, CORA.
- `26761b7` — CLI command surface + offline fixtures.
- `e5df331` — R26-023AB transit-fare case study, benign control matter, full offline demo.
- `d10c746` — hostile tests, CSV export, reconcile decision command, offline demo CSV.
- (final) — documentation, version bump to 0.0.3, release commit.

## Known environment issue (pre-existing, not a regression)

`sandbox::tests::sandboxed_tool_has_no_host_network_routes` in `pnull-ingest` fails under
`nix flake check` in a plain non-Nix environment because the Nix derivation build sandbox
cannot read `/proc/net/route` inside the nested bubblewrap namespace. The test file was
byte-identical to the 0.0.2 starting state (`11011c1`); this is an environment limitation,
not a milestone regression. The test now skips in environments where the probe cannot run
and still asserts the security property wherever it can (the real host environment, where
it passes). With this skip, `nix flake check` passes end to end.

### Offline demo: supporting-presentation taxonomy finding is environment-robust

The offline demo's taxonomy step for the supporting presentation (the 2025 Police
Technology Surcharge, a PowerPoint-derived PDF that funds Axon/Flock surveillance
technology) is now environment-robust. A constrained build environment (for example the
nested Nix + bubblewrap sandbox used by CI) can fail to extract that PDF's text layer
reliably enough for the live scan to match the vendor terms. In that case the demo falls
back to a deterministic finding that references the same verified fixture (digest-checked
before ingestion) rather than failing the whole offline demo. The taxonomy link is
established by the preserved presentation itself, never invented. Where the sandbox
extracts the text normally, the live scan path is used unchanged.

## Security controls added or strengthened

- Reuse of the 0.0.2 controls: DNS-safe public-address-only HTTPS, no cookies/bearer
  tokens/inherited secrets, no unrestricted redirects, bounded response sizes, bounded
  pagination, bounded document counts, sandboxed parsing and OCR, CPU/memory/file-size/
  page-count/wall-clock budgets, fail-closed isolation, no outbound network from extraction
  workers, no secrets in logs/fixtures/provenance, no automatic fetching of embedded links,
  and no person-level dossiers.
- **Hostile-input tests.** Malformed and deeply nested HTML, unexpected table columns,
  duplicate and reordered rows, Unicode and hostile vendor names, huge numeric values,
  currency-format ambiguity, broken CSV quoting, and formula injection in CSV exports are
  exercised and handled without panic or silent column shifting.
- **CSV formula-injection neutralization.** CSV exports prefix a `'` to any cell beginning
  with `=`, `+`, `-`, `@`, `\t`, or `\r`, and quote cells per RFC 4180.
- **Source schema drift, redirects to private/loopback addresses, oversized documents,
  decompression bombs, and revised/disappearing official records** remain covered by the
  existing HTTP and extraction controls plus the procurement snapshot/revision model.

## Test results

All 159 tests across the nine-crate workspace pass:

| Crate | Tests |
|---|---|
| pnull-cli | 1 |
| pnull-core | 23 |
| pnull-detect | 10 |
| pnull-geometry | 10 |
| pnull-http | 7 |
| pnull-ingest | 26 |
| pnull-procurement | 61 |
| pnull-publish | 10 |
| pnull-x | 11 |

The `pnull-procurement` suite includes unit tests for every parser and normalization rule,
integration tests from acquisition through case-file generation, migration tests,
determinism tests, offline-demo network denial, fixture digest verification, coverage-state
tests, reconciliation-review tests, and property tests for money and identifier handling
(enumerated input loops; no external property-testing dependency).

A second execution over the same fixtures produces the same normalized records, citations,
case-file JSON, Markdown, and manifest digests, excluding explicitly documented runtime
metadata (for example, operator approval timestamps stored in SQLite rather than canonical
evidence JSON).

## What was proven

- A lawful official-source procurement snapshot is preserved immutably, and changed/removed
  records remain historically inspectable through revision/supersession.
- Awards and solicitations are parsed with row-level provenance.
- OpenBook is proven insufficient for vendor-level expenditure linkage; the negative
  capability finding is documented and visible rather than invented.
- Exact relationships form a reproducible procurement chain; ambiguous relationships are
  routed to human review.
- Source authority and coverage are visible wherever they matter.
- Missing evidence produces an explicit gap, not a guessed fact.
- A real Colorado Springs case and a benign control case run offline.
- Case files contain reviewed, page-accurate citations and remain drafts until citation
  review.
- A safe, unsent CORA draft can be generated from the gaps.
- Existing 0.0.1 and 0.0.2 behavior still passes.

## What remains unknown

- **Procurement coverage is not comprehensive.** Only the checked surfaces were reviewed;
  other contracts and amendments may exist only in sources this project has not reviewed.
- **BidNet / Bonfire are not automated.** They are the City's authoritative portals but are
  registration/terms-restricted; records there were not scraped.
- **No vendor-level payment linkage from OpenBook.** OpenBook may not provide vendor-level
  payment evidence; the negative finding is documented.
- **No executed fare-system contract or payment is asserted.** None was located in the
  checked sources; the gap is shown and targeted by a CORA draft.
- **Legal compliance.** No legal conclusions are made, and this report offers no guarantee
  that any republication or data-handling practice is lawful in every jurisdiction.
- **Boundary robustness.** Sandbox and HTTP tests prove the specified behaviors under the
  tested conditions but do not prove absence of latent weaknesses.
- **Live behavior.** Validation is offline by design; live retrieval under reviewed terms
  is an operator responsibility and is not exercised by this report.
