# Statewide search status — November 10, 2026

**Historical audit:** this page records the November 10 baseline. The November 11
implementation and current output are described in [Colorado research](colorado-research.md).
Statements below about the current state and missing features refer to that earlier audit.

The current website is a five-decision release, **not a statewide search engine**.
The 4,111 geography entries are a directory. They do not mean 4,111 governments
have been investigated. The owner’s target remains every Colorado public body
that makes a relevant surveillance-technology decision, regardless of size.

## What has actually been done

| Measure | Current evidence |
| --- | --- |
| Published cases | Five decisions in four municipalities |
| Named positions | 47 vote rows, representing 38 scoped recorded voters |
| Preserved official portraits | 25; Denver’s 13 recorded voters have no bound portrait |
| Geography entries without a published case | 4,107; this is not a search-result count |
| Entries without a connected source/case mapping | 4,099: state 1, counties 62, municipalities 263, schools 178, provisional districts 3,595 |
| Other repository mappings | Four published municipalities, one county with preview cases, two calendar-only profiles, five small-town document profiles |
| Website leads | 241 municipal directory links; not verified record portals or completed searches |
| Systematic bounded jurisdiction searches | None recorded in the inspected release/capture stores |
| Default scheduled work | Recheck 12 exact URLs for existing cases; no statewide discovery |
| Automatic portrait collection | Not implemented; current registry entries were individually sourced and identity-bound |

The inspected stores have no discovery queue items, discovery artifacts, or
coverage-ledger entries. Their 12 source captures concern already identified
cases in Commerce City, Castle Rock, and Denver. The additional 17 HTTP
observations are robots-policy probes, not searches of government decisions.
These local observations are a dated audit, not values the public API should
hardcode as future search history.

## Current automation boundary

The release updater preserves newly fetched bytes and rebuilds the existing
manifests. A changed factual digest blocks publication until its proof binding
is resolved. It does not infer new decisions from arbitrary documents.

Seven calendar profiles and five small-town document-index profiles exist.
They need current source-level acquisition policies and an explicitly enabled
schedule before collection. The private document queue is empty. A captured
document is not automatically extracted, screened, converted into a proposal,
and released. The proposal builder takes exact claim inputs supplied separately;
its narrow verifiers do not cover arbitrary minutes formats. Source review is
per source/path, not a requirement to approve every new record by hand.

## Next development milestone

Complete **one unattended new-case cycle**, then expand the same mechanism by
portal type and jurisdiction. A cycle must:

1. Enumerate the official meeting/document archive for a declared date window,
   recording pagination, source identity, missing periods, and fetch receipts.
2. Archive new and changed documents through the existing bounded queue; extract
   them in the sandbox and screen their text with a versioned relevance rule set.
   A keyword match creates a private candidate, not an allegation.
3. Turn supported record layouts into source-bound decision and vote claims.
   Keep failed motions and consent blocks distinct; retain missing contract,
   signature, role, and term links as unknown.
4. Discover official role/profile pages for each named voter, resolve identity
   within the public body and term, and preserve the exact official portrait
   bytes with their source locator. A name-only match or missing public image
   must not create a portrait binding.
5. Publish verified facts automatically, retain corrections and withdrawal
   history, and emit an alert revision only when public content changes.
6. Publish a search audit: last attempt, successful date window, documents
   examined, parser/rule version, inaccessible sources, and unresolved candidates.

Use the existing 64 counties and 273 municipal entries as the first measurable
coverage tranche, including the five small-town profiles. Expand state agencies,
school districts, and verified special districts in parallel as their official
portals are identified. The 3,595 DOLA rows are provisional discovery entries;
they require legal-body and identity checks. The current roster also omits some
authorities, universities, tribal governments, and intergovernmental bodies.

Success means a previously unseen official decision reaches a published page
with preserved proofs and any available identity-verified portraits, and a
repeat run neither duplicates it nor invents a vote. A jurisdiction can say
“no matches in the checked sources for these dates” only after complete
enumeration and successful screening of that declared source/window. A
published case alone never establishes exhaustive search coverage.

## First implementation slice

Start with archived discovery candidates, not another directory expansion.
Extract the existing `check_candidate_archive` verification in
`crates/pnull-cli/src/case_proposal.rs` into one shared verified-candidate loader.
It already binds the original index anchor, raw bytes, source policy, and fetch
receipts. Enqueue durable screening work where `archive.rs` persists a candidate
archive, and add a bounded `source screen-queue` worker after candidate capture.
Reuse `pnull_ingest::ingest_bytes`, `RealSandbox`, one aggregate resource budget,
and `pnull_detect::scan` rather than introducing another extraction path.

Checkpoint by document digest, extraction version, and rules digest. Existing
evidence IDs cannot alone prove successful extraction: failed extractions are
persisted and current evidence insertion is first-write-only. Define a versioned
retry path and test restart, changed bytes/rules, sandbox failure, source-policy
expiry, and no-match versus extraction failure. The worker produces private
cited screening results; supported record parsers then create the explicit
proposal claims. Fixed archive endpoints lack the discovery anchor manifest and
need their own explicit adapter. None of this worker is implemented by the
schedule and public-coverage fixes in this follow-up.
