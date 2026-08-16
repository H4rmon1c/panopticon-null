# Methodology

## Observation

Panopticon Null first preserves bytes. The original SHA-256, source URL, jurisdiction, title, source type, publication date when supplied, retrieval time, MIME type, filename, extractor, status, lineage, and processing version form the evidence record. The original is never replaced by a revision.

Extraction normalizes line endings and horizontal whitespace to make deterministic line citations. Every ingestion and reprocessing job also records an immutable processing-run provenance record: schema version, Panopticon Null version, source revision, rules digest, state-config digest, input evidence IDs, native tool names and versions, sandbox backend and version, configured budgets, actual resource consumption, timestamps, outcome, structured errors, and output artifact IDs and digests.

The core principle is: **if the public record does not prove it, Panopticon Null does not say it.** A statement is only asserted when an exact, preservable source span supports it.

## Page-accurate citations

For PDFs, `pnull-geometry` builds an immutable text map per page using Poppler's `pdftotext -bbox-layout`, in the coordinate system `pdf_user_space_points_bottom_left_y_up`. Each map records page dimensions, rotation, extracted words with bounding boxes, the extractor and its version, a text-map digest, and its relation to the source digest.

Bounding rectangles are validated: negative coordinates, inverted rectangles, out-of-bounds geometry, missing pages, and quote mismatches are rejected. A page citation therefore binds a quote to exact page geometry rather than to a fuzzy line number.

OCR uses deterministic Tesseract TSV with a pixel-to-page transform. OCR confidence is metadata; it is never treated as proof of content. When a cited region was read by OCR, that is recorded and disclosed.

The render command produces a highlighted image of the quoted region for human inspection before any review decision.

## Detection

`rules/surveillance.yml` is the complete machine-readable taxonomy. Each rule has an identifier, category, explicit terms, documented false-positive phrases, and rationale. Terms include ALPR/LPR, named vendors, biometric systems, real-time crime centers, predictive policing, social-media monitoring, cell-site simulators, geofence warrants, drones, data brokers, and investigative platforms.

A term establishes only `Mention detected`. A stronger state requires a separate exact phrase in the same bounded context or a structured Legistar `Action:` field. Negated and conditional phrases do not create approval. Conflicting state phrases produce `Unknown` for human review.

The supported states are: mention detected, proposal, public hearing scheduled, vote scheduled, approved, rejected, contract executed, renewal or expansion, deployment reported, policy change, and unknown.

An optional language model may suggest a category to an operator, but this release does not call one. No language-model output can enter a finding without an exact source span and deterministic rule.

## Subject/action discipline

Actions and subjects are explicit, versioned domain types. Every action identifies exactly one subject and the citations that support it. An action is never transferred to another subject merely because both appear in the same matter. For example, the approval of an ordinance is an action on the ordinance, not on the vendors named in a related presentation; only a separate, provable action on a vendor becomes a vendor assertion. This prevents the conflation of a legislative vote with a procurement award.

## Change detection

The comparator reports configured changes in price, duration, retention, data sharing, vendor/subcontractor, vote date, scope/quantity, amendment language, new surveillance terms, and removed privacy-relevant language. Published diffs contain only lines attached to meaningful changes, minimizing unrelated free text. A difference is never labeled illegal.

## Human review before publication

No citation is published without a human review decision bound to exact content digests: evidence ID, source digest, locator/geometry, quote, quote digest, rule digest, processing artifact digest, and proposed public fields. Changing any bound value invalidates approval. Publication allowlists state which field categories may appear publicly but are not auto-approval. The site, Atom feed, and X pipeline fail closed on pending, rejected, stale, or mismatched decisions.

## Publication

Public pages show the institution, state, reason, rules and rule digest, exact citations, source links, local hashes, and limitations. They do not publish the full local evidence archive. The privacy gate is deliberately conservative and human review remains mandatory before distribution.

## Demo interpretation

Two preserved Colorado Springs matters model the system.

The v0.0.1 matter is Ordinance 25-93. The preserved Colorado Springs presentation lists Axon body-worn cameras, Axon evidence/storage and AI systems, and Flock vehicle-intelligence cameras with stated annual costs. Draft and signed versions of Ordinance 25-93 show that the technology-surcharge ordinance later passed.

The v0.0.2 matter is Ordinance No. 15-84 (2015), matter 15-00663, preserved at `fixtures/co2/`. It established the municipal court Information Technology Surcharge that Ordinance 25-93 amends. The demo models this matter with the subject (Ordinance 15-84) and the action (finally passed), distinguishing action/object/technology and supporting-versus-dispositive and known-versus-unknown.

For the 2015 matter, the surveillance-technology link (Axon body cameras/evidence systems/AI transcription, Flock vehicle-intelligence cameras) is documented in the preserved 2025 presentation as supporting evidence; it is not asserted by the 2015 action itself. No separate vendor contract or award for Axon or Flock was located in the reviewed Legistar source, so no such procurement is asserted. This is a documented limitation, not a fabricated relationship.

These examples establish that systems and ordinances appeared in the same official matters and that the ordinances passed. They do not establish a new vendor award, a contract amendment, legality, deployment beyond the record, or the effect of the surcharge.
