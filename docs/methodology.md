# Methodology

## Observation

Panopticon Null first preserves bytes. The original SHA-256, source URL, jurisdiction, title, source type, publication date when supplied, retrieval time, MIME type, filename, extractor, status, lineage, and processing version form the evidence record. The original is never replaced by a revision.

Extraction normalizes line endings and horizontal whitespace to make deterministic line citations. PDF line numbers refer to the preserved normalized extraction, not to visual PDF line numbering. This limitation is shown on evidence pages; page-specific PDF locators are a later improvement.

## Detection

`rules/surveillance.yml` is the complete machine-readable taxonomy. Each rule has an identifier, category, explicit terms, documented false-positive phrases, and rationale. Terms include ALPR/LPR, named vendors, biometric systems, real-time crime centers, predictive policing, social-media monitoring, cell-site simulators, geofence warrants, drones, data brokers, and investigative platforms.

A term establishes only `Mention detected`. A stronger state requires a separate exact phrase in the same bounded context or a structured Legistar `Action:` field. Negated and conditional phrases do not create approval. Conflicting state phrases produce `Unknown` for human review.

The supported states are: mention detected, proposal, public hearing scheduled, vote scheduled, approved, rejected, contract executed, renewal or expansion, deployment reported, policy change, and unknown.

An optional language model may suggest a category to an operator, but this release does not call one. No language-model output can enter a finding without an exact source span and deterministic rule.

## Change detection

The comparator reports configured changes in price, duration, retention, data sharing, vendor/subcontractor, vote date, scope/quantity, amendment language, new surveillance terms, and removed privacy-relevant language. Published diffs contain only lines attached to meaningful changes, minimizing unrelated free text. A difference is never labeled illegal.

## Publication

Public pages show the institution, state, reason, rules and rule digest, exact citations, source links, local hashes, and limitations. They do not publish the full local evidence archive. The privacy gate is deliberately conservative and human review remains mandatory before distribution.

## Demo interpretation

The Colorado Springs presentation lists Axon body-worn cameras, Axon evidence/storage and AI systems, and Flock vehicle-intelligence cameras with stated annual costs. Draft and signed versions of Ordinance 25-93 show that the technology-surcharge ordinance later passed. This establishes that the systems and ordinance appeared in the same official matter and that the ordinance passed. It does not establish a new vendor award, a contract amendment, legality, deployment beyond the record, or the effect of the surcharge.
