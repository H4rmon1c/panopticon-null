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

## Development

```console
nix --extra-experimental-features 'nix-command flakes' develop
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
nix --extra-experimental-features 'nix-command flakes' flake check
```

Use focused commits. Avoid unnecessary dependencies and clever abstractions. Comments should explain only non-obvious constraints. Never commit secrets, raw sensitive logs, generated local databases, or unreviewed personal records.

A new rule requires a rationale, false-positive fixtures, classification tests, and exact-citation tests. A new source requires official discovery documentation, access-policy notes, rate limits, fixtures, provenance hashes, and honest completeness limitations.
