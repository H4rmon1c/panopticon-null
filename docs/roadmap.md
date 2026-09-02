# Roadmap

## 0.0.1

One Colorado Springs vertical slice: official Legistar ingestion, preserved evidence, deterministic taxonomy, cautious classifications, meaningful version changes, SQLite, static publication, Atom, and citation-bound dry-run X drafts.

## 0.0.2 (shipped: "The Verifiable Receipt")

Stay in Colorado. The 0.0.2 scope proposed in the 0.0.1 roadmap was delivered:

1. Page/section coordinates and quoted bounding boxes for PDFs (page-accurate citations via `pnull-geometry`).
2. An operating-system sandbox (bubblewrap) and aggregate extraction/OCR job budgets.
3. DNS resolution and validation, persisted redirect/header/ETag provenance, and conditional requests (DNS-safe HTTP).
4. Persistent robots/terms review snapshots and an explicit review command workflow.
5. Bounded Legistar pagination and per-matter attachment discovery without automating restricted procurement portals.
6. Explicit subjects and actions so ordinance, policy, solicitation, award, and vendor states cannot be conflated.
7. Structured publication allowlists and a human citation-review queue.
8. Reconciliation commands for uncertain partial X threads.
9. Immutable processing-run records with native extractor versions, source revision, budgets, and actual consumption.
10. A second Colorado Springs matter (Ordinance No. 15-84, matter 15-00663) with genuine legislative change, validated against the preserved official source.

## Honest limitations at 0.0.2

- **Not comprehensive procurement coverage.** The reviewed Legistar source is a meeting system, not a complete procurement ledger. No separate vendor contract or award for Axon or Flock was located in the reviewed source, so none is asserted.
- **D10 is limited to two matters.** The system demonstrates two preserved Colorado Springs matters; it does not cover the full corpus of city business.
- **No legal compliance guarantee.** The project makes no legal conclusions and offers no guarantee that any republication or data-handling practice is lawful in every jurisdiction.
- **No perfect privacy detection.** Pattern checks cannot reliably catch every sensitive value; human review remains a required boundary.
- **No proof beyond the preserved records.** The project asserts only what the preserved public record proves.
- **No automated restricted portals.** BidNet and other registration/auth-required portals are not automated.
- **No live X transport in tests or demos.** The demo produces only dry-run drafts and zero network posts.

## 0.0.3 (shipped: "The Procurement Chain")

Turn isolated evidence receipts into a verifiable institutional money trail:
solicitation → amendment → award → contract → expenditure. Stay in Colorado. The 0.0.3
scope was delivered:

1. **Source-authority model and coverage ledger.** Every procurement source carries an
   authority classification (authoritative, informational mirror, financial export,
   meeting/legislative, operator-supplied, unreviewed, restricted/inaccessible). Every
   acquisition writes a persistent coverage-ledger entry. Coverage defaults to
   `unknown`/`partial`; absence is phrased "Not observed in the checked sources," never
   "No contract exists."
2. **Immutable source snapshots + change detection.** Fetched pages/exports/documents are
   immutable; changed official bytes produce a second snapshot linked by
   revision/supersession with a deterministic record-level diff; `304` records provenance
   without duplication; embedded links are never auto-followed.
3. **Procurement domain model.** Matters, events, identifiers (no cross-format merging
   without deterministic rules + tests), money (never floating point; raw strings kept
   distinct), and organizations (source spelling preserved; non-exact matches to human
   review; no auto-merging of subsidiaries/joint ventures/similarly named firms).
4. **Bounded ingestion adapters.** Contract-award table with row-level provenance,
   solicitation mirror carrying its incompleteness warning, documented negative capability
   finding for OpenBook COS (no vendor-level expenditure linkage), and a safe
   operator-supplied public-record import path treating supplied files as hostile.
5. **Reconciliation.** Automatic connections only via exact normalized identifiers,
   explicit official relationships, or existing evidence-backed relationships; a
   reconciliation-review queue with immutable, auditable decisions.
6. **Case files.** Deterministic JSON + Markdown case files with timeline, roles, raw/parsed
   money, citations, authority labels, contradictions, missing documents, coverage,
   provenance, SHA-256 manifest, and limitations; drafts until citation review.
7. **Gap-driven CORA drafts.** Local, unsent Colorado Open Records Act drafts built from
   unresolved gaps; no sending, no email guessing, operator/legal review required.
8. **Real case study + benign control.** The Next-Generation Transit Fare Collection System
   RFI (R26-023AB) is ingested as an RFI (not an award/contract) with no mass-surveillance
   labeling and an explicit gap where no executed contract/payment was located; a benign
   control matter (Crack Seal Materials) proves ingestion does not automatically accuse.
9. **Hostile-input tests + CSV safety.** Malformed/deeply nested HTML, column shifts,
   duplicates/reordering, Unicode/hostile names, huge numbers, currency ambiguity, broken
   CSV quoting, source schema drift, and CSV formula-injection neutralization.

## Honest limitations at 0.0.3

- **Not comprehensive procurement coverage.** Only the checked surfaces were reviewed;
  other contracts and amendments may exist in sources not reviewed.
- **BidNet / Bonfire not automated.** They are the City's authoritative portals but are
  registration/terms-restricted; records there were not scraped.
- **No vendor-level payment linkage from OpenBook.** OpenBook may not provide vendor-level
  payment evidence; the negative finding is documented and visible.
- **No executed fare-system contract or payment asserted.** None was located in the
  checked sources; the gap is shown and targeted by a CORA draft.
- **Not surveillance-by-default.** A technology purchase is not automatically surveillance;
  the RFI and control matter are modeled without automated accusation.
- **No legal advice.** Panopticon Null provides no legal advice or legal-compliance
  guarantee.

## 0.0.4 (shipped: "The Public Ledger")

Closes the four gaps left when 0.0.3's procurement chain ended in local state: the chain
becomes public, changes to official records become alerts, and the records-request loop
closes — all behind the existing human review and privacy gates. The 0.0.4 scope was
delivered:

1. **Procurement change alerts.** Re-ingesting a reviewed surface that differs from the
   latest snapshot produces deterministic, idempotent alerts `record_added`/`record_modified`/
   `record_removed`. Award-row `record_modified` carries a field-level diff (field name, old
   raw value, new raw value). Row identity is a stable key (official identifier where present;
   otherwise a digest over the row's normalized field values). Removals are phrased as
   comparisons, never legal conclusions. Alerts flow into the existing Alert store; `pnull
   alerts` lists both kinds; X drafts reuse the existing pipeline verbatim. No surveillance
   labeling: taxonomy matches appear only as optional metadata.
2. **Publish the procurement chain.** `pnull build-site` publishes from the SAME
   deterministic case-file JSON as `case build` (one source of truth): a matter list at
   `/co/procurement/index.html` plus a per-matter page with timeline, roles, raw/parsed money,
   citations, contradictions, coverage gaps, a "what changed" section from supersessions,
   provenance, a SHA-256 manifest, and a limitations block. Gates fail closed (citation-review
   bound to exact digests, a `procurement_casefile` publication-allowlist category, privacy
   backstop over all rendered text incl. vendor names and raw money); pending/rejected/stale/
   mismatched pages are withheld with a "publication withheld pending review" note. The Atom
   feed runs under identical gates. `pnull procurement publish-ready <matter-id>` reports gate
   state without publishing.
3. **CORA request ledger.** Append-only, fully local, never sends. States
   `drafted`/`submitted`/`response_received`/`gap_resolved`/`still_unresolved`; no transition
   is reversed or edited; corrections are new events. The tool never guesses a recipient and
   never claims a legal deadline.
4. **Second snapshot in-place-edit demonstration.** `fixtures/procurement/contract-awards-2.html`
   is a labeled SYNTHETIC demonstration fixture derived from the preserved official snapshot
   (not an official record), editing one amount + one vendor name + adding notes on
   `Q25-130ZM` (record_modified), removing `R24-T114JD` (record_removed), and adding
   `R25-044AB` (record_added). The demo re-ingests it and shows supersession + diff + alerts +
   RecordCorrected/RecordRemoved events + "what changed".
5. **Explicit official-relationship links ("who authorized it").** Source adapters may declare
   reference fields; a link (kind `official_relationship`) is recorded only when a declared
   reference field of one preserved record contains an exact match of an identifier stored for
   another record AND both endpoints resolve to stored snapshots with valid SHA-256 digests.
   Near-miss identifiers become candidates in the reconciliation review queue, never auto-links.
   The demo records ZERO such links (absence proven, not fabricated).
6. **`pnull procurement refresh`.** `pnull procurement refresh <source-id> [--live]` with
   `--dry-run` as the default (prints the planned fetch with zero network). `--live` requires
   the persistent source-review gate, one request at a time, DNS-safe HTTPS, conditional
   request where an ETag exists, and aggregate budgets. Live path: fetch → new snapshot (or 304
   provenance) → change detection → alert count + matter ids → coverage-ledger entry. Fails
   closed on refusal/failure. The demo never invokes this command.

Schema v3 (four new tables: `procurement_alerts`, `cora_requests`, `official_relationships`,
`supplied_records`) migrates transactionally from v2, preserving every 0.0.1/0.0.2/0.0.3 row
byte-for-byte, with the real 0.0.3 upgrade fixture and atomic-rollback failure-injection tests.

## Honest limitations at 0.0.4

- **Not comprehensive procurement coverage.** Only the checked surfaces were reviewed; other
  contracts and amendments may exist in sources not reviewed.
- **BidNet / Bonfire not automated.** They are the City's authoritative portals but are
  registration/terms-restricted; records there were not scraped.
- **No vendor-level payment linkage from OpenBook.** OpenBook may not provide vendor-level
  payment evidence; the negative finding is documented and visible.
- **No executed fare-system contract asserted.** None was located in the checked sources; the
  gap is shown and targeted by a CORA draft.
- **The second snapshot is synthetic.** It is a labeled demonstration fixture derived from the
  preserved official snapshot, not a live re-fetch.
- **Zero official-relationship links demonstrated.** The demo records no such links; that
  absence is proven, not fabricated.
- **Refresh change detection reads prior rows from the preserved fixture on disk.** Snapshots
  store metadata, not raw bytes, so removed/modified rows are compared against the preserved
  fixture rather than a byte-level snapshot.
- **No legal advice.** Panopticon Null provides no legal advice or legal-compliance guarantee.
- **Privacy pattern checks are a backstop, not a guarantee.** Detection cannot reliably catch
  every sensitive value; human review remains a required boundary.

## Future directions

Only after the 0.0.2, 0.0.3, and 0.0.4 controls are proven should the project consider a
second Colorado jurisdiction or state-specific account. Likely next steps:

- Expand the number of preserved matters and validated live-source runs under reviewed terms.
- Consider a kernel-level sandbox (VM/microVM) as a stronger boundary than bubblewrap.
- Consider a formal DNS rebinding policy (for example, resolving and validating before and after connect).
- Continue to grow the reviewed taxonomy and rules as new matters are preserved.
- Under an approved persistent source review, exercise a lawful live re-fetch of the
  contract-award surface and record its coverage ledger entries — a concrete next step now
  that `pnull procurement refresh --live` exists.
- Preserve a record that contains an explicit procurement-identifier reference field, so that
  an `official_relationship` link can be demonstrated (the current demo records zero such
  links, proving absence rather than fabrication).

Any expansion must keep the same boundaries: no legal-intent inference, no private-person dossiers, no target ranking, and no automated access to restricted portals.
