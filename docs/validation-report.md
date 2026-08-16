# v0.0.1 validation report

Date: 2026-08-16  
Platform: Linux x86_64  
Rust: 1.89.0, pinned by `rust-toolchain.toml` and Nix  
Release: 0.0.1

## Commands and results

| Command | Result |
|---|---|
| `cargo fmt --all --check` | Passed. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed with zero diagnostics. |
| `cargo test --workspace --all-features` | Passed: 31 tests, 0 failed, 0 ignored; all doc tests passed. Tests were offline. |
| `nix --extra-experimental-features 'nix-command flakes' develop --command cargo deny check` | Passed: advisories, bans, licenses, and sources all `ok`. Expected duplicate-version warnings remain informational under policy. |
| `nix --extra-experimental-features 'nix-command flakes' flake check --print-build-logs` | Passed all five checks: build-and-test, formatting, Clippy, dependency policy, and offline demo. The build used vendored Cargo sources and pinned Nix inputs. |
| `sha256sum -c fixtures/co/SHA256SUMS` | Passed for five exact official Colorado Springs fixtures. |
| `cargo run --locked -q -p pnull-cli -- demo --output /tmp/pnull-release-demo` | Passed entirely offline; generated one alert, 14 static files, and a three-post dry-run thread. |
| `xmllint --noout /tmp/pnull-release-demo/site/atom.xml` | Passed; Atom output is well-formed XML. |
| `! rg -q '<script' /tmp/pnull-release-demo/site` | Passed; no script element occurs in generated site output. |
| `test "$(cat /tmp/pnull-release-demo/network-posts.txt)" = 0` | Passed; no network post occurred and no X transport was constructed. |

## Demonstrated evidence

- Draft evidence ID: `evidence:0136f043bcf653166033290ffa1522d406360e7b6345b4852af92e1739c584c3`.
- Draft source SHA-256: `badda12921d29bf2fc2d86b274efc9544fa339db82de830ba460eaa9c6bbd2e4`.
- Signed evidence ID: `evidence:4284ce9d4e09fac2dca5da1532305d48a055fd3de45be39e72aaeaac47575d26`.
- Signed source SHA-256: `f364d09dbbb29a0b8d89c53002eb4bb757ddf0b942f245f4d70738543b4dc1fb`.
- Supporting presentation rules: `vendor.axon` and `vendor.flock-safety`, each with exact normalized-text lines and source provenance.
- Meaningful change: the draft's blank final-passage field became `Finally passed: November 25, 2025` in the signed source.
- Final public-record state: `Approved`, explicitly constrained to the ordinance and not represented as proof of a vendor purchase.
- Alert ID: `alert:94c9526803b6e122e8b4f31a05f21fe84535b948812d253d2817bc9f6a71b2c6`.

## Reproducibility

The CLI test runs the demo in two clean directories and compares every generated site file and every canonical evidence JSON byte-for-byte. Fixture retrieval timestamps are fixed. Runtime approval/post timestamps and SQLite physical bytes are intentionally outside canonical evidence comparison.

## Known limitations confirmed for release

- The Colorado Springs Legistar feed is not a complete procurement ledger. Absence from it establishes nothing.
- Live robots directives were not independently retrievable through the research browser on 2026-08-16. Live ingestion therefore requires an operator's current review flag and enforces a 24-hour interval.
- PDF locators identify lines in deterministic normalized extraction, not visual page coordinates.
- Poppler/Tesseract are process-limited but not enclosed in a kernel sandbox.
- Privacy pattern checks cannot identify every possible personal name or regional plate format; human citation review remains mandatory.
- The repository default canonical URL ends in `.invalid`. Dry-run drafts work, but live posting is refused until an operator configures a reviewed public URL.
- No X account was created, no credentials were configured, and no post was submitted.
