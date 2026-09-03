# Threat model

## Protected interests

- Integrity and provenance of original public evidence.
- Accuracy and restraint of published claims.
- Privacy of people incidentally present in public records.
- X credentials and operator approval intent.
- Availability of the local workstation and bounded use of CPU, memory, process, and disk resources.
- Reproducibility of rules, builds, and output.
- The human review boundary: publication decisions, source reviews, and reconciliation must remain explicit operator actions.

## Adversaries and failures

Sources may serve malformed HTML, JSON, PDFs, decompression bombs, huge pages, misleading metadata, scripts, macros, attachments, redirects, or later-edited documents. Vendors or authorities may remove language, change files in place, or dispute characterization. An operator may accidentally publish sensitive free text or approve one draft and later generate another. A partial network failure may make the state of an X thread uncertain. A hostile or compromised source may attempt server-side attacks (SSRF, DNS rebinding), malicious extraction inputs, or attempts to defeat the review and reconciliation gates.

## Controls added or strengthened in v0.0.2

- **DNS-safe HTTP.** Only public addresses are accepted; loopback, private, link-local, multicast, unspecified, documentation, and non-public addresses are rejected. Mixed public + prohibited DNS answers fail closed. HTTPS is required and certificate validation cannot be disabled. Redirects are same-host and provenance is persisted for every request and redirect.
- **Sandbox escape boundary.** Live PDF/OCR ingestion runs in a Linux bubblewrap sandbox with no network namespace, no inherited secrets, no writable access outside a dedicated temporary output directory, read-only exact inputs, new process/session boundaries, and prlimit CPU/memory/file-size/output-size/wall-time limits. Live ingestion fails closed when the sandbox cannot be established.
- **Aggregate job budgets.** Total downloaded bytes, attachments, PDF pages, OCR pages, extracted bytes, child processes, CPU allowance, and wall-clock allowance bound each job, so a single matter cannot exhaust workstation resources.
- **Secret leakage.** Processing-run environment capture uses an allowlist, not an environment dump; subprocess logs contain no secrets; HTTP provenance never persists cookies, authorization headers, or bearer tokens.
- **Content execution.** Source-document content is never executed — no macros, embedded files, JavaScript, or document actions. Poppler/Tesseract are allowlisted subprocesses with the limits above.
- **Review subversion.** Publication decisions are bound to exact content digests (evidence, source, locator/geometry, quote, quote digest, rule digest, processing artifact, proposed public fields). Changing any bound value invalidates approval. The site, Atom, and X fail closed on pending/rejected/stale/mismatched decisions.
- **Procurement conflation.** Explicit subjects/actions mean an action is never transferred to another subject merely because both appear in the same matter; a regression test proves an ordinance approval cannot become a vendor purchase assertion.
- **Robots/terms review.** Persistent, expiring source reviews (capture/record/show/verify) gate live retrieval; retrieval refuses on no review, expiration, config change, host change, out-of-scope endpoint, or a prior restriction.
- **X reconciliation.** Uncertain attempts are never blindly retried; append-only operator decisions gate new attempts; no audit history is deleted.

## Controls added or strengthened in v0.0.3

- **Source authority and coverage.** Every procurement source carries an authority classification and every acquisition writes a coverage-ledger entry. Coverage defaults to `unknown`/`partial`; a source is `complete` only with affirmative reproducible evidence. This prevents a partial source from being treated as proof of absence. User-facing language says "Not observed in the checked sources," never "No contract exists."
- **Immutable snapshots + revision/supersession.** Fetched pages/exports/documents are immutable. If an official URL later serves different bytes, both snapshots are preserved and linked by a revision/supersession relationship with a deterministic record-level diff; old artifacts and derived observations are never rewritten. A `304` records provenance without duplicating the artifact. Embedded links are never auto-followed.
- **Money and identifier discipline.** Money is never floating point and raw strings are preserved; `N/A`, `various`, omitted, and `$0.00` are kept distinct. Differently formatted identifiers are never merged without a deterministic rule and tests. This blocks silent coercion of ambiguous values into a false link.
- **Entity review boundary.** Normalization may produce candidate aliases but never auto-merges subsidiaries, parents, joint ventures, or similarly named firms; non-exact matches require an immutable human decision. This blocks fuzzy automatic entity merging.
- **Reconciliation controls.** Automatic connections require exact normalized identifiers, explicit official relationships, or existing evidence-backed relationships. Similar names/titles/amounts/dates/keywords/LLM judgment never connect records. The reconciliation-review queue and immutable decisions make every accept/reject auditable.
- **Operator-supplied records treated as hostile.** The import path requires a declared source URL/records-request identifier, acquisition date, document role, a declaration of lawful possession, an exact file digest, processing provenance, the existing sandbox and resource limits, and human review before publication. Supplied files are never trusted.
- **CSV formula-injection neutralization.** CSV exports prefix a `'` to cells beginning with `=`, `+`, `-`, `@`, `\t`, or `\r`, and quote cells per RFC 4180, so exported spreadsheets cannot execute formulas.
- **Hostile procurement-input tests.** Malformed/deeply nested HTML, unexpected table columns, duplicate/reordered rows, Unicode and hostile vendor names, huge numeric values, currency-format ambiguity, broken CSV quoting, and source schema drift are exercised without panic or silent column shifting.
- **CORA drafts are local and unsent.** Gap-driven CORA drafts never send, never guess an email recipient, and never claim a legal deadline or entitlement; they require operator/legal review and avoid person-level data unless directly necessary and lawfully justified.

## Controls added or strengthened in v0.0.4

- **Change alerts are comparisons, not conclusions.** Re-ingesting a reviewed surface that differs from the latest snapshot produces deterministic, idempotent alerts (`record_added`, `record_modified`, `record_removed`). Removals are phrased as a comparison — "The row observed in snapshot N (digest …) is not present in snapshot M (digest …)" — never as a legal conclusion, and no alert labels a row as a "surveillance purchase" or "surveillance award"; taxonomy-matching terminology may appear only as optional "surveillance-related terminology observed, rule `<rule-id>`" metadata.
- **Idempotent alert ids.** Alert ids are stable over source id + row identity + change kind + old/new snapshot ids, so re-ingesting the same snapshot pair never creates a second alert and a byte-identical re-ingest (304 path) creates none. This prevents duplicate or spurious change notifications.
- **Publication gates fail closed on the procurement chain.** Every citation on a procurement page requires an Approved citation-review decision bound to the exact digests; a `procurement_casefile` publication-allowlist category is required and is not auto-approval; the privacy backstop runs over all rendered procurement text, including vendor names and raw money strings; pending/rejected/stale/mismatched review or a missing allowlist category removes the page/entry with a visible "publication withheld pending review" note rather than emitting a partial page. The site and Atom render from the same deterministic case-file JSON under the identical gates.
- **CORA ledger is append-only and never sends.** The local request ledger never sends a request, never guesses a recipient, and never claims a legal deadline or entitlement. Transitions are immutable events with operator/timestamp/note; corrections are new events; duplicate transitions and unknown evidence ids are refused. No audit history is deleted.
- **Official-relationship links are never invented.** Links are recorded only through declared reference fields, an exact match of a stored identifier, and both endpoints bound to stored snapshots with valid SHA-256 digests; near-miss references become CANDIDATES in the review queue, never automatic links. This blocks fabricating an authorization relationship from proximity or similarity.
- **Refresh fails closed on the persistent source-review gate.** `pnull procurement refresh` defaults to a dry run making zero transport calls; live refresh refuses on no review, expired review, config change, host change, or out-of-scope endpoint, makes one request at a time with DNS-safe HTTPS, sends a conditional request where an ETag exists, applies aggregate budgets, and on refusal or failure states the reason and changes nothing.

## Controls retained from v0.0.1

- HTTPS only, same-host HTTPS redirects only, literal private/loopback destinations rejected, bounded redirects and response bytes.
- Original-byte hashing, collision-safe identifiers, existing-blob verification, private local directory permissions, and duplicate prevention.
- No browser engine, script execution, shell interpolation, macros, embedded attachments, or source-driven commands.
- Exact citations, rule digest, negation handling, ambiguity fallback, and rhetoric separated from factual claims.
- Minimal meaningful diff lines rather than raw free-text republication.
- Shared privacy checks for static and X text; explicit local approval remains required.
- X credentials only from environment or a mode-`0600` file; generic errors never include request/response bodies or tokens.
- Approval binds to the exact post/thread digest. Per-segment remote IDs are persisted.

## Residual risk

DNS names are resolved at request time; the public-address check is a defense against SSRF but not a complete defense against a DNS rebinding within a single request lifetime. The bubblewrap sandbox plus prlimit limits materially raise the cost of an exploit but are not a formal kernel security boundary; a kernel-level sandbox such as a dedicated VM or microVM would be stronger. External PDF parsers remain complex native code. Privacy pattern checks cannot reliably distinguish every person's name or every plate format, on procurement pages as much as on document pages — they remain a backstop, not a guarantee. Human publication review is therefore a security boundary, not a suggestion.

What remains unknown: whether a vendor contract or award for Axon or Flock exists in sources Panopticon Null has not reviewed; whether an executed contract or payment exists for the transit-fare system in sources not reviewed; whether BidNet/Bonfire (the City's authoritative portals, not automated) hold records Panopticon Null has not seen; whether OpenBook could ever support vendor-level payment evidence beyond its current budget-level export; whether a future review will surface a new sensitive value that the pattern checks miss; and whether any sandbox or HTTP boundary has a latent weakness that tests have not exercised. The second-snapshot demonstration uses a synthetic fixture derived from the preserved official snapshot, not a live re-fetch; zero official-relationship links are demonstrated in that demo (the absence is proven, not asserted). Since v0.0.4c, snapshots persist their exact rows and change detection compares stored rows from the database, so it no longer depends on the prior fixture on disk; the remaining limitation is that legacy snapshots captured before v0.0.4c have no stored rows and produce no diff until their rows are next captured. These are documented limitations, not claims of perfection.
