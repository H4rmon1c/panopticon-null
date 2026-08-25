# Operator guide — procurement chain

This guide covers the v0.0.3 procurement commands. The full model is in
`docs/procurement-methodology.md`; the surveyed sources are in `docs/0.0.3-source-survey.md`.

## Principles

- **Follow the money without inventing the links.** Records are connected only when the
  evidence supports the connection.
- **Not observed is not "does not exist."** Absence from a partial source is never proof of
  absence; the language is "Not observed in the checked sources."
- **Informational mirrors are not authoritative.** The City's solicitation list may be
  incomplete or outdated; BidNet and Bonfire hold the authoritative versions but are not
  automated.
- **A technology purchase is not automatically surveillance.** Ingestion does not accuse.

## Offline demonstration

```console
cargo run --locked -p pnull-cli -- demo
```

This runs entirely offline against the preserved fixtures under `fixtures/procurement/`,
verifies the fixture digests, ingests the solicitation mirror and contract-award snapshots,
records the OpenBook negative capability finding, builds the transit-fare RFI matter and the
benign control matter, generates their case files and a local unsent CORA draft, and exports
a formula-neutralized `procurement/awards.csv`. It performs zero network posts.

The demo store is created under `<output>/state/pnull.db`. Commands below can be run against
a demo store, for example:

```console
cargo run --locked -p pnull-cli -- --data-dir <output>/state procurement show <matter>
```

## Commands

### Ingestion (offline default)

```console
pnull procurement ingest solicitations
pnull procurement ingest awards
pnull procurement ingest openbook
pnull procurement import <path>
```

- `ingest solicitations` and `ingest awards` parse the preserved snapshots into records with
  row-level provenance. The solicitation records carry the source's incompleteness warning.
- `ingest openbook` records the documented negative capability finding: OpenBook COS is a
  budget-level export that does not provide vendor-level expenditures. Payment evidence from
  this source is represented as unavailable; no relationship is invented.
- `import <path>` ingests an operator-supplied public record. You must provide a declared
  source URL or records-request identifier, the acquisition date, the document role, and a
  declaration of lawful possession. The file is treated as hostile and passes through the
  existing sandbox and resource limits, and requires human review before publication.

Live retrieval of any procurement surface requires an explicit live mode **and** an approved
persistent source review (see `docs/operator-guide-source-review.md`). Default demonstrations
are fully offline.

### Reviewing a matter

```console
pnull procurement show <matter>
pnull procurement gaps <matter>
pnull procurement reconcile <matter>
```

- `show` prints the matter's records, events, organizations, money, citations, coverage, and
  limitations.
- `gaps` lists unresolved evidence gaps (missing expected documents, unlocated records).
- `reconcile` lists the reconciliation-review queue for the matter. Connections that are not
  exact are never automatic; they wait for an operator decision.

To record an immutable reconciliation decision:

```console
pnull procurement reconcile <matter> --item <item-id> --decision <accepted|rejected> --operator <name> --note <note>
```

Every decision is append-only and auditable; it binds to the exact item and decision inputs.

### The linked record (v0.0.4)

```console
pnull procurement chain <matter>
```

`chain` prints the ordered procurement lifecycle for a matter —

`solicitation -> amendment -> award -> contract -> expenditure`

— as a deterministic linked record. Each stage shows the observed records, and every link
retains its supporting record, citation, and snapshot digest, so the connection can be traced
to digest-bound evidence. Missing stages render as `Not observed`, never as proof that no
record exists.

Links are created only when normalized procurement identifiers match exactly, or when an
official record explicitly identifies the relationship. Similar, incomplete, or ambiguous
identifiers are never auto-linked; they are queued for human reconciliation instead. If a
fixture contains no genuine cross-stage match, the command reports the exact missing document
rather than manufacturing a connection.

### Coverage

```console
pnull coverage show
pnull coverage diff <old-snapshot> <new-snapshot>
```

`coverage show` prints the coverage ledger. `coverage diff` produces a deterministic
record-level diff between two snapshots, which is how changed or removed official records
remain historically inspectable. Coverage defaults to `unknown`/`partial`; a source is
`complete` only with affirmative, reproducible evidence.

### Case files

```console
pnull case build <matter>
pnull cora draft <matter>
```

`case build` writes a deterministic case file (JSON + Markdown) with the chronological
timeline, organizations in their documented roles, raw and parsed money, exact citations,
source-authority labels, contradictions, missing documents, coverage, provenance, a SHA-256
manifest, and a limitations section. The case file remains a draft until it passes the human
citation-review and publication-allowlist controls.

`cora draft` generates a local, unsent Colorado Open Records Act draft from the matter's
unresolved gaps. It never sends the request, never guesses an email recipient, never claims a
legal deadline or entitlement, and states that operator/legal review is required.

## Verification

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
(cd fixtures/procurement && sha256sum -c SHA256SUMS)
nix --extra-experimental-features 'nix-command flakes' flake check --print-build-logs
```

`cargo deny check` is provided by the pinned Nix environment and runs inside `nix flake
check` as the `dependency-policy` check.
