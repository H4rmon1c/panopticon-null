# Panopticon Null

> **No human being is born to be indexed.**

Panopticon Null is lawful, nonviolent, evidence infrastructure for dismantling the surveillance panopticon. It makes acquisitions, promises, changes, and institutional actions visible without rebuilding person-level surveillance under a different operator.

> The machinery of mass surveillance depends on invisibility. This project records what is purchased, what is promised, what changes, and who authorized it.

Version 0.0.1 is deliberately narrow: one Colorado jurisdiction, one complete local-first pipeline, and no live posting. It monitors the official Colorado Springs City Council Legistar API and preserves official matter attachments used by the offline demonstration.

## What 0.0.1 does

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
| **Observed** | Exact text in an identified public source, with URL, SHA-256, retrieval time, extraction method, and line citation. |
| **Classified** | A deterministic state assigned because an exact cited phrase satisfies a published rule. Ambiguous or conflicting phrases resolve to `Unknown` or `Mention detected`. |
| **Compared** | A textual difference between two preserved source versions. It is not a legal conclusion. |
| **Unknown** | Legality, implementation outside the record, effectiveness, intent, completeness of the portal, and any unstated contract term. |

A keyword never proves a purchase. Approval of Ordinance 25-93 establishes approval of that ordinance; it does **not** by itself establish approval of an Axon or Flock purchase. The supporting presentation establishes that those systems appeared in the same public matter and states listed costs. The project does not infer beyond those sources.

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

The flake pins Nixpkgs, the Rust overlay, Rust 1.89.0, and the RustSec advisory database. Poppler, Tesseract, and `prlimit` come from Nix; tests never download executables.

### Without Nix

Install Rust 1.89.0, Cargo, Poppler (`pdfinfo`, `pdftotext`, `pdftoppm`), Tesseract with at least one language, and `prlimit`, then run:

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

Common commands:

```console
cargo run --locked -p pnull-cli -- source list
cargo run --locked -p pnull-cli -- --data-dir .pnull ingest --robots-reviewed
cargo run --locked -p pnull-cli -- --data-dir .pnull scan
cargo run --locked -p pnull-cli -- --data-dir .pnull diff
cargo run --locked -p pnull-cli -- --data-dir .pnull build-site --output site
cargo run --locked -p pnull-cli -- --data-dir .pnull alerts
cargo run --locked -p pnull-cli -- --data-dir .pnull verify <evidence-id>
cargo run --locked -p pnull-cli -- --data-dir .pnull x draft <alert-id>
cargo run --locked -p pnull-cli -- --data-dir .pnull x approve <alert-id>
```

Live source retrieval is refused unless the operator has reviewed current robots directives and passes `--robots-reviewed`. The configured 24-hour interval is persisted and enforced. The source uses one request at a time; it does not bypass authentication, CAPTCHAs, access controls, or restrictions.

### X safety model

Drafting is always local. `x approve` hashes and approves the exact generated post or thread. A live attempt requires all of the following:

1. A real public `canonical_base_url` in `configs/states/co.toml` (the repository default is intentionally `.invalid`).
2. An approved, unchanged draft digest.
3. `X_BEARER_TOKEN` or `PNUL_X_SECRET_FILE` pointing to a mode-`0600` token file.
4. `pnull x post <alert-id> --confirm`.

Tests use only a fake transport. An attempt is reserved before network activity, each successful thread segment is stored immediately, and uncertain partial attempts cannot be blindly retried. Version 0.0.1 provides no automatic recovery for a partial live thread; reconcile it manually before touching local state.

## Validate

```console
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
nix --extra-experimental-features 'nix-command flakes' flake check --print-build-logs
```

Fixture integrity:

```console
sha256sum -c fixtures/co/SHA256SUMS
```

## Privacy boundary

Raw evidence and SQLite state are created under a private local directory. Public output includes only institutional facts, selected citations, and provenance. Publication fails closed on recognized plate labels, personal contact fields, Social Security numbers, home-address patterns, coordinates, and movement-log fields. Detection is a backstop, not permission to publish arbitrary free text; operators must review all citations before distributing a site or approving an X draft.

No facial recognition. No person-level movement analysis. No dossiers on activists, officers, employees, or residents. No harassment, doxxing, unauthorized access, evasion, or physical interference.

## Repository map

- `pnull-core`: canonical records, IDs, SQLite, digest verification, approval/post ledgers.
- `pnull-ingest`: lawful retrieval, Legistar parsing, bounded extraction, Poppler/Tesseract orchestration.
- `pnull-detect`: YAML rules, cautious classification, exact citations, meaningful diffs.
- `pnull-publish`: privacy-gated static HTML and Atom.
- `pnull-x`: state-aware drafts, exact-draft approval, transport trait, redacted credentials.
- `pnull-cli`: commands and the offline vertical slice.

See `docs/architecture.md`, `docs/methodology.md`, and `docs/source-adapters.md` for details.

## License

GNU Affero General Public License v3.0 or later.
