# Contributing

Contributions must strengthen both the mission and the evidence standard. This is not a neutral dashboard, and rhetoric is never a substitute for proof.

## Before submitting

1. Keep scope narrow and local-first.
2. Do not add telemetry, tracking, advertising, credential examples, or hosted-service requirements.
3. Do not add person-level surveillance, face matching, movement analysis, plate publication, equipment interference, access-control bypasses, or private-source scraping.
4. Research source terms and robots directives. Preserve exact lawful fixture bytes, URL, retrieval date, and SHA-256.
5. Add deterministic tests that do not require internet access.
6. Treat every external byte and free-text field as hostile and potentially sensitive.
7. Explain what a source establishes and what it cannot establish.
8. Never construct a live network transport in a test, fixture, or demo. The demo produces only dry-run X drafts and zero network posts.

## The eight-crate layout

The workspace is eight crates; keep responsibilities where they live:

- `pnull-core` — evidence, finding, alert, matter, subject, action, citation, review, processing-run, source-review, fetch-observation, X-attempt, and allowlist schemas; deterministic IDs; SQLite and migration.
- `pnull-ingest` — metadata validation, live retrieval policy, the bubblewrap sandbox, job budgets, and hostile-content extraction.
- `pnull-geometry` — page-accurate PDF citations: text maps, bounding boxes, coordinate transforms, geometry validation, OCR confidence.
- `pnull-http` — DNS-safe HTTP: public-address validation, allowlisted headers, conditional retrieval, fetch observations.
- `pnull-detect` — the reviewed YAML taxonomy, classification, negation handling, ambiguity fallback.
- `pnull-publish` — publication gates, review-queue enforcement, static site + Atom generation.
- `pnull-x` — draft/approve/posting with the transport behind a trait; attempts and reconciliation.
- `pnull-cli` — composition of the above without duplicating state-specific logic.

## Development

```console
nix --extra-experimental-features 'nix-command flakes' develop
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
nix --extra-experimental-features 'nix-command flakes' flake check
sha256sum -c fixtures/co/SHA256SUMS
sha256sum -c fixtures/co2/SHA256SUMS
cargo run --locked -p pnull-cli -- demo
```

Use focused commits. Avoid unnecessary dependencies and clever abstractions. Comments should explain only non-obvious constraints. Never commit secrets, raw sensitive logs, generated local databases, or unreviewed personal records.

## Adding fixtures

- Preserve exact lawful official bytes under `fixtures/<jurisdiction>/` or a new matter directory, together with the source URL, retrieval date, and SHA-256 in the SUMS file and `fixtures/README.md`.
- Add a migration fixture under `fixtures/migration/` when schema behavior changes (for example, `v0.0.1-minimal.sql`), and a migration test that proves old records upgrade without reinterpretation.
- Never mutate an existing fixture digest; content-addressed blobs must stay stable.

## Review gates

- A new rule requires a rationale, false-positive fixtures, classification tests, and exact-citation tests.
- A new source requires official discovery documentation, access-policy notes, rate limits, fixtures, provenance hashes, and honest completeness limitations.
- New citation geometry or review logic requires tests proving that bound values invalidate approvals and that the site/Atom/X fail closed on non-approved decisions.
- Sandbox and HTTP changes require tests that stay offline and prove the boundary (no network reach, no unrelated-file reads, no out-of-dir writes, fail-closed DNS).
