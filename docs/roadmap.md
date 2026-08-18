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

## Future directions

Only after the 0.0.2 and 0.0.3 controls are proven should the project consider a second Colorado jurisdiction or state-specific account. Likely next steps:

- Expand the number of preserved matters and validated live-source runs under reviewed terms.
- Consider a kernel-level sandbox (VM/microVM) as a stronger boundary than bubblewrap.
- Consider a formal DNS rebinding policy (for example, resolving and validating before and after connect).
- Continue to grow the reviewed taxonomy and rules as new matters are preserved.
- Under an approved persistent source review, exercise a lawful live retrieval path for a procurement surface (for example, the contract-award table) and record its coverage ledger entries.

Any expansion must keep the same boundaries: no legal-intent inference, no private-person dossiers, no target ranking, and no automated access to restricted portals.
