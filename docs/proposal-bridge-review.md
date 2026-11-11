# Checked document proposal bridge review — 2026-11-10

This follow-up fixes the path from a source-checked document proposal to the public atlas, geographic coverage report, and alerts/news exports. It does not add researched Colorado cases or establish statewide search coverage. The public snapshot still contains the existing five cases. All additional decisions and people in the bridge regression are synthetic test data, generated in a temporary directory and never written into the public dataset.

## Confirmed defects and fixes

| Finding | Preconditions and impact | Fix |
| --- | --- | --- |
| A named-vote quote could be cropped from a larger extracted line | A supplied proposal could select `Alice Smith Aye` from `Not Alice Smith Aye`, or from a line containing multiple voters. This was a potentially serious attribution error, not an unauthenticated write endpoint. No automatic proposal author currently exists. | Require the complete extracted line to match the cited quote, then verify its single exact name/position pair. Negation, trailing narrative, and multiple-voter lines cannot be cropped into a named vote. |
| Valid document votes failed the browser's API-only proof grammar | Generic Rust releases used `line:N`; the browser accepted only the existing Legistar JSON vote paths. | Add an explicit document-only proof context: same-source matter, motion, whole-line named vote, bounded line distances, decision-scoped voter, and consistent jurisdiction identity. Existing JSON vote rules remain separate. A document marker or decision-scoped voter cannot fall back to the JSON vote path. |
| Generic decisions had no coverage join | The exporter supplied `jurisdiction_ref` on a decision but omitted its `jurisdiction_ids` and `JURISDICTION` entity. The coverage API correctly refused the unmapped decision. | Export one geography identity matched to the roster and attach the decision, contract, and recorded voters to it. The source index/config mapping remains checked in Rust. The geography entity explicitly describes an index identity and makes no claim that its presence establishes technology use or a decision. |
| Multiple proposals could not share a source or jurisdiction | The bundle append function rejected every duplicate ID, including an identical preserved document cited by two decisions. | Deduplicate only identical objects. Different content under an existing ID and duplicate case releases still fail. People remain scoped to their particular decision. |
| A verified award still received a “not established” summary | A proposal with a verified authorization relationship contradicted that relationship in the generated summary. | State that award authorization is cited while keeping execution and payment unestablished. |

The independent validator review also identified contradictory document markers attached to JSON API votes and inconsistent geography references. Both are rejected with regressions. The news exporter now excludes established facts from its “Unresolved” sentence while retaining the structured states in the alerts API.

## Validation

- `cargo test --locked -p pnull-cli case_proposal::`: 23 tests passed, including whole-line vote attribution, item/motion scope, source receipts, jurisdiction mapping, failed outcomes, and award gaps.
- `node --test web/panopticon-terminal/scripts/test-proposal-bridge.mjs`: 26 tests passed. This command invokes the actual Rust proposal composer, release exporter, and bundle append code. Its synthetic extracted-text fixture creates two different decisions in one document, with four named vote rows. The JavaScript validator, coverage generator, and alerts/news generator then consume those exact Rust outputs, both separately and appended to the existing five-case snapshot.
- The generated combined result shares one source and one Moffat geography identity, preserves four decision-scoped voters, and keeps alert revisions unchanged on a repeated export. Tests reject changed people or votes, cropped/multiline quotes, page-only and out-of-range locators, unrelated sources, consent relabeling, removed or substituted motion proofs, orphan proof markers, and mismatched geography.
- The cross-language test is a required CI step after Rust and Node setup. The broader production checks are recorded separately in the production security review and CI results.

The cross-language fixture begins at checked extracted text; it does not claim to fetch or ingest a new live municipal document. Archive/raw-byte validation and sandbox extraction have separate tests. No external systems were attacked, and no live VPS was assessed in this follow-up.

## Remaining limits

The browser checks structural consistency; it does not independently download preserved source bytes or provide cryptographic assurance against an administrator replacing the entire site. The Rust producer remains responsible for raw bytes, fetch receipts, current source review, exact extraction, and intervening item/motion context.

The generic parser is deliberately narrow. It requires complete scalar lines and atomic named-vote lines, rejects ambiguous item context, and cannot turn page-only quotes or consent-block roll calls into direct-item votes. A wider parser needs its own source-specific semantics and fixtures; weakening these gates would risk false attribution.

Proposal input files are still authored explicitly. Automatic document screening, source-specific claim extraction and proposal generation, historical source traversal, and official portrait discovery/identity matching remain development work. Configuring a town or listing a geography row does not implement those stages or prove the town was searched. See [statewide search status](statewide-search-status.md) for the actual worker handoffs and missing coverage.
