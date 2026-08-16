# Validation report — v0.0.2 ("The Verifiable Receipt")

This report states exactly what was proven by the v0.0.2 validation suite and what remains unknown. It is an honest account, not a claim of perfection or legal compliance.

## Validation commands

The following validation commands pass for this release:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo deny check`
- `nix flake check` (with flakes enabled)
- `sha256sum -c fixtures/co/SHA256SUMS`
- `sha256sum -c fixtures/co2/SHA256SUMS`
- `cargo run --locked -p pnull-cli -- demo`

The two fixture SUMS files verify that every preserved official byte under `fixtures/co/` and `fixtures/co2/` is intact and unmodified.

## The demo

The offline demo (`cargo run --locked -p pnull-cli -- demo`) is proven to:

- run entirely offline using preserved official fixtures (no network access);
- exercise the v0.0.1 → v0.0.2 schema migration path and the SQLite store;
- generate page-accurate citations from the official PDF using the real Poppler-in-bubblewrap extraction path;
- produce explicit subjects and actions for both matters, keeping the ordinance approval separate from any vendor mention;
- require deterministic, clearly labeled demonstration review decisions before publication (the seeded reviews are labeled as demonstrations, not real operator approvals);
- generate a JavaScript-free static site and an Atom feed;
- produce only dry-run X drafts;
- construct no live X transport;
- perform zero network posts;
- be reproducible byte-for-byte: the test runs the demo in two clean directories and asserts that the generated `site/` and `state/records/` trees are identical, and that `network-posts.txt` contains `0`.

## D10 status: the second matter

The second preserved matter is **Ordinance No. 15-84 (2015)**, matter **15-00663**, preserved at `fixtures/co2/matter-15-00663-ordinance-15-84.json` and `fixtures/co2/event-1109-2015-11-24.json`. It established the municipal court Information Technology Surcharge that Ordinance 25-93 (the v0.0.1 matter) later amended.

The demo models this matter with the subject (Ordinance 15-84) and the action (finally passed), distinguishing action/object/technology, supporting-versus-dispositive evidence, and known-versus-unknown.

**Honest limitation.** The surveillance-technology link (Axon body cameras/evidence systems/AI transcription, Flock vehicle-intelligence cameras) is documented via the preserved 2025 presentation as supporting evidence. It is not asserted by the 2015 action itself. **No separate vendor contract or award for Axon or Flock was located in the reviewed Legistar source, so no such procurement is asserted.** This is a documented limitation of the reviewed source, not a fabricated relationship.

## Required tests

The validation suite includes twenty tests, each proving a specific property. All pass.

1. `offline_demo_is_reproducible_and_never_posts` — the demo runs twice in clean directories and produces byte-identical `site/` and `state/records/` trees, zero network posts, and no `<script>` in public output.
2. `real_pdf_fixture_is_extracted_by_poppler_in_sandbox` — the official PDF is extracted by Poppler running inside the real bubblewrap sandbox.
3. `parses_ocr_tsv_deterministically` — deterministic Tesseract TSV parsing with the pixel-to-page transform.
4. `rejects_negative_coordinates` — a bounding rectangle with negative coordinates is rejected.
5. `rejects_inverted_rectangles` — an inverted rectangle (x_max < x_min or y_max < y_min) is rejected.
6. `rejects_out_of_bounds` — geometry outside the page bounds is rejected.
7. `quote_mismatch_fails_closed` — a quote that does not match the map's words fails closed.
8. `v01_database_upgrades_without_reinterpreting_records` — a v0.0.1 database upgrades transactionally and preserves every canonical record byte-for-byte, with no reinterpretation.
9. `sandboxed_tool_cannot_read_an_unrelated_host_file` — a sandboxed tool cannot read a file unrelated to its inputs.
10. `sandboxed_tool_cannot_write_outside_its_output_directory` — a sandboxed tool cannot write outside its dedicated output directory.
11. `sandboxed_tool_has_no_host_network_routes` — a sandboxed tool has no network access.
12. `sandboxed_tool_that_never_terminates_is_killed_on_timeout` — a non-terminating tool is killed on timeout.
13. `aggregate_budget_blocks_many_attachments` — aggregate job budgets block an excess of attachments.
14. `mixed_public_and_private_answers_fail_closed` — mixed public + private DNS answers fail closed.
15. `credentials_are_never_persisted` — HTTP provenance never persists credentials.
16. `etag_200_then_304_does_not_create_evidence` — a 304 after a 200 references prior evidence and never creates a new blob.
17. `discover_matters_uses_only_official_fields_and_https_hosts` — matter discovery uses only documented official fields and HTTPS hosts.
18. `pagination_stops_at_configured_max_pages` — pagination stops at the configured maximum page count.
19. `repeated_page_cannot_create_an_infinite_loop` — repeated-page/non-progressing detection prevents an infinite loop.
20. `ordinance_approval_never_becomes_a_vendor_purchase` — the approval of an ordinance cannot be transformed into an Axon/Flock vendor purchase assertion.

Additional suite tests cover reconciliation (append-only, no history deletion, no blind retry), duplicate/idempotent ingestion, HTML script exclusion, structured extraction failures, malformed metadata rejection, budget limits for PDF pages and downloaded bytes, and many others. All pass.

## What was proven

- Preserved official bytes are unmodified (both fixture SUMS files verify).
- The full validation command set passes.
- The demo is offline, reproducible, exercises migration and page-accurate citations, models explicit subjects/actions, requires demonstration review decisions, generates a JS-free site + Atom feed, produces only dry-run X drafts, constructs no live X transport, and performs zero network posts.
- The sandbox, DNS-safe HTTP, geometry validation, review binding, and subject/action boundaries behave as specified, as proven by the twenty required tests above.

## What remains unknown

- **Procurement coverage.** The reviewed Legistar source is a meeting system, not a complete procurement ledger. No Axon or Flock vendor contract/award was located, so none is asserted. Other contracts and amendments may exist only in sources this project has not reviewed.
- **Completeness.** D10 is limited to two preserved matters; it is not comprehensive coverage of city business.
- **Legal compliance.** No legal conclusions are made, and this report offers no guarantee that any republication or data-handling practice is lawful in every jurisdiction.
- **Privacy detection.** Pattern checks cannot reliably detect every sensitive value; human review remains a required boundary and no perfect privacy detection is claimed.
- **Boundary robustness.** Sandbox and HTTP tests prove the specified behaviors under the tested conditions but do not prove absence of latent weaknesses. A kernel-level sandbox would be a stronger boundary than bubblewrap.
- **Live behavior.** Validation is offline by design; live retrieval under reviewed terms is an operator responsibility and is not exercised by this report.
