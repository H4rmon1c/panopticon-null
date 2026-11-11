# Colorado public-record research pipeline

This pipeline works toward statewide discovery while keeping preliminary
references distinct from the five released, evidence-reviewed decisions. The
checked-in `web/panopticon-terminal/public/research-signals.json` is a dated
snapshot of the actual local run; its `generated_at`, `coverage`, and records
are the source for current counts. A jurisdiction in the geography directory
has not necessarily been searched, and a missing match is not a finding that
no relevant decision exists.

## What the machine does

1. Seed municipality websites from the existing Colorado Municipal League
   directory, county sites from an archived county-clerks directory, and
   school district websites from an exact CDE district-code to NCES-ID join.
   The pinned, reproducible school directory provides 177 website leads for
   178 mapped school districts; a shared-host site remains unbound. A separate
   pinned DOLA extract supplies [special-district website leads](colorado-special-districts.md)
   when the district ID, name, and a unique root host agree. Each such homepage
   must also display the district's complete name before any linked documents
   can be attributed to it. Shared-host and path-based sites need a tenant-aware
   adapter. Discovery prioritizes never-attempted sites, then rotates by oldest
   attempt, so a bounded run does not permanently starve later entries. Visit
   up to twelve prioritized, same-host pages across two navigation steps on each site, find linked official meeting portals and
   same-host agenda/minutes PDFs and Word documents, and retain the exact index page and link.
   Document links may contain encoded spaces; other encoded path sequences are
   left out of the queue to avoid ambiguous separators, traversal, and controls.
   Same-origin HTML base URLs and numeric `t` timestamp queries are retained
   when rebinding a document to its original index link. If an indexed link
   redirects to a Revize document, the collector can separately capture the
   same path without a numeric timestamp when source policy permits. The
   original index and link remain visible, and the page states that the
   captured document may be a different version.
   When a directory website has moved, discovery can follow a bounded HTTPS
   redirect to a `.gov` domain carrying the complete jurisdiction name.
   The original redirect and final page must both remain in the private archive
   before the new host can be attributed to that jurisdiction.
   For captured CivicPlus agenda indexes, the collector also reads published
   year/category selectors without executing their JavaScript and requests a
   bounded number of older official indexes. It rotates between jurisdictions,
   retains the selector and index receipts, and queues linked meeting documents.
   It pauses new historical discovery when 1,000 documents are already queued;
   one in-flight index batch may add more before the next check.
2. Prefer linked government documents. For linked Legistar installations, use
   its public API as an additional matter-title index. Query a bounded number of
   pages for configured vendor and technology terms. A title match is a lead,
   not proof of approval or deployment. The collector rotates between portals
   and searches the expanded term list in bounded batches; the scheduler runs
   two passes so each portal can advance through both. It resumes pagination
   across runs, resetting after the final page or the 100-page safety cap;
   changing the term version or page size also resets the relevant cursor.
   Captured queries from the previous term version remain verifiable. Results can
   shift while a portal is being paged, so this is not a complete census.
   For El Paso County, follow the county's own link to the AgendaSuite calendar
   and screen the public meeting pages linked from its recent calendar. That
   portal includes other public bodies, so a match is labeled as a portal
   reference until the responsible body is established. The calendar's recent
   window does not establish historical coverage.
3. Fetch discovered meeting documents within per-file and per-run byte budgets.
   The Rust worker verifies their SHA-256 digests, extracts text with the
   existing sandboxed Poppler path, and applies the repository's detection
   rules. Failures remain errors; they are never counted as no-match results.
   OCR retries render one page at a time, cover at most 20 pages per document,
   and are marked as OCR when exported. Every expected page must render and
   complete extraction; a partial scan cannot become a no-match result. Because
   OCR text is currently flattened by page, its public reference uses a generic
   title and does not attach nearby names as if they were tied to the item. DOCX files are
   parsed in a separate network-isolated sandbox with ZIP, XML, memory, CPU,
   and output limits; embedded links and files are never followed.
   A bare “Flock” mention is labeled as an unresolved identity unless the
   cited line also identifies the company or relevant technology. Obvious
   non-surveillance artwork context is withheld and counted separately.
   When a packet contains a clearly delimited earlier minutes section, a
   present official can be named as context only when that same line names
   the official; this does not establish their vote on the topic.
4. For matched Legistar matters, capture the matter history, corresponding
   meeting item, recorded votes, and any separate consent-agenda item. The
   exporter reopens every raw capture and verifies client, matter, meeting,
   item, vote IDs, and source locators before showing a vote. Direct item
   votes, consent-block votes, consent-designated items with uncertain vote
   scope, and missing votes stay distinct. A failed motion remains failed.
5. Look for identity-labeled portraits in government directories, homepages,
   and up to four linked same-host council or commissioner pages, and retain
   the fetched image bytes. A same-name official source or explicitly sourced
   nickname is required; a missing portrait stays missing. The interface can
   also reuse an existing sourced portrait when the portal, jurisdiction,
   person ID, and name all match. Portrait dates are not inferred.
   For Douglas County, the collector also checks the official current-and-former
   commissioners gallery. It accepts an image only when the exact official name
   and image occur in the same unique gallery card, and revalidates that card,
   the original image bytes, and both fetch receipts before export.
   Ten historical Denver voters can also use original, individually labeled
   JPEGs from the [2019 city budget](https://www.denvergov.org/content/dam/denvergov/Portals/344/documents/Budget/2019/2019_Budget%20Book_V1-OnlineVersion-compressed.pdf).
   The pinned proof binds each Denver portal person ID and name to the budget's
   label, page position, original embedded image index, and digest. A sandboxed
   test rechecks both the page fixture and the complete archived source.
   Five Arapahoe County research voters also reuse the original JPEG streams
   from the labeled 2022 board page of the county's recommended budget. The
   archived PDF digest, page, image indexes, aliases, and extracted-page fixture
   are pinned in `fixtures/atlas/portrait-sources/arapahoe-2022-budget-proof.json`.
   Two additional Arapahoe voters have original JPEG streams from a labeled
   2024 county budget page. Jessica Campbell's different source caption is
   supported by a separately preserved self-published name-identity source.
   Seven Littleton voters have portraits from a labeled 2023 city newsletter.
   Its embedded JPEG 2000 streams are retained in the source PDF; lossless PNG
   display copies and their original-stream digests are independently verified
   by `scripts/test-littleton-portraits.py`.
   The remaining two Littleton voters use original JPEG streams from the
   labeled 2024 newsletter page, verified by `scripts/test-littleton-2024-portraits.py`.
   Five Commerce City voters use original JPEG streams from the labeled 2022
   city financial report. Their row and column positions and embedded streams
   are independently verified by `scripts/test-commerce-portraits.py`.
6. Export a strict, sanitized static JSON snapshot. Every entry is labeled
   `unconfirmed_source_reference`; source links, digests, timestamps, exact
   locators, and any uncertain vote scope remain visible. The website and
   `/api/v1/research.json` use this snapshot. Raw documents, raw responses,
   network receipts, extracted text, and the SQLite queue remain private. For
   each jurisdiction with a recorded website attempt, the snapshot reports
   partial screening, failure, and queue counts without asserting a complete
   date window.
   A text-layer document may show up to five additional, separately located
   formal passages, so an earlier discussion does not hide a later resolution.
   These excerpts are still preliminary mentions and do not create vote claims.
   Source links whose verified redirect has a queued or screened alternate are
   counted as redirects rather than failed document extractions.
   School-district research references use generic descriptions and omit
   personal names, raw passages, and vote lists until a separate source can
   establish an appropriate public official identity. School-site links to
   third-party legislative portals are not automatically attributed to the
   district until the responsible agency is established.

The research collector records its own machine observations; it does not
fabricate human review decisions or change the existing verified-release
gates. Source policy is checked for each captured site. The self-hosted
research schedule enables the project owner's requested exception for public
TownCloud agenda links explicitly present on a municipality website, including
the same-tenant PDF redirect. Set `PNUL_ALLOW_TOWNCLOUD_PUBLIC=0` to honor that
service's robots exclusion. Other explicit robots disallows, content access
denials, CAPTCHAs, and login requirements remain blocked; the five previously
excluded Granicus attachment URLs remain excluded.
The collector understands robots wildcard and longest-match rules using the
pinned, vendored Protego parser. A municipality may redirect its robots file
to a narrowly scoped Revize host. A vendor document is only followed when an
archived government link and vendor redirect bind it to the same document or
TownCloud tenant;
the vendor does not become a general trusted source for that jurisdiction.
Linked TownCloud calendars can include other public bodies, so a meeting's
responsible body remains unestablished until separately sourced.
The collector uses TLS verification, public-IP-only DNS checks with connection
pinning, bounded reads, per-host pacing, and restricted redirects. Archive
bytes are content-addressed. The document queue is resumable and retains
prior matches if a refresh fails; changed captures retain private history and
are labeled historical where appropriate. A failed website rediscovery also
preserves earlier portal proofs and reports the latest attempt as unavailable;
an earlier portal proof establishes a past link, not that the link is current.
Bounded document retries, action checks, and portrait lookups prioritize
unchecked and oldest work so a growing queue does not continually revisit
the same jurisdictions first.
These safeguards are not a claim
that every public source is accessible or every automated match is correct.
New collection stops if the private volume has under 2 GiB or 4,096 free
inodes; publication can still export a previous snapshot. Monitor archive
growth and maintain off-server backups rather than deleting source evidence.

## Running locally

Use an absolute private directory outside the website and a compiled `pnull`
with Poppler and bubblewrap available on its runtime path. For example:

```console
python3 scripts/colorado-research.py --data-dir /private/panopticon-research discover --limit 320 --workers 8
python3 scripts/colorado-research.py --data-dir /private/panopticon-research civic-history --limit 100 --per-place 4 --workers 8
python3 scripts/colorado-research.py --data-dir /private/panopticon-research scan --max-clients 100 --pages 10
python3 scripts/colorado-research.py --data-dir /private/panopticon-research actions --limit 100
python3 scripts/colorado-research.py --data-dir /private/panopticon-research agendasuite --limit 60 --pages 3
python3 scripts/colorado-research.py --data-dir /private/panopticon-research screen-documents --pnull-bin /absolute/path/to/pnull --limit 300 --per-place 30 --workers 4
python3 scripts/colorado-research.py --data-dir /private/panopticon-research docx-saved --limit 50 --workers 2
python3 scripts/colorado-research.py --data-dir /private/panopticon-research ocr-saved --pnull-bin /absolute/path/to/pnull --limit 30 --workers 2
python3 scripts/colorado-research.py --data-dir /private/panopticon-research portraits --limit 32
python3 scripts/colorado-research.py --data-dir /private/panopticon-research export --output /absolute/staging/public/research-signals.json --assets-dir /absolute/staging/research-portraits
```

Use `screen-documents --refresh-existing` to recheck older successful captures
and `--retry-errors --ocr` to retry eligible failures. Each command enforces
its own limits. `ocr-saved` works on preserved PDFs without another HTTP fetch;
after repairing unavailable OCR data, `--retry-unavailable` retries those
specific failures immediately. Install English Tesseract data at the system
default or set `PNUL_TESSDATA_DIR` to a directory under `/usr` or `/nix/store`.
Only that validated read-only directory is passed into the sandbox.
Use `--retry-previous-limit` to reconsider documents rejected by the former
five-page OCR limit. The scheduled research job uses this option; documents
over the current 20-page cap remain errors, not no-match conclusions.
The `--revalidate-ocr` mode rechecks earlier OCR results that lack the current
per-page text-mapping version. Earlier workers could render every page but
discard words after the first page, so even their old page-set flag is not
sufficient. Until rechecked, these results count as errors rather than completed
screens and cannot create a public match. `docx-saved --revalidate-docx` similarly rechecks older Word
extractions that did not establish that deleted text was excluded. The earlier
result remains in private history.
Do not run collectors concurrently against the same hosts:
the self-hosted wrapper holds the shared collection lock for the complete job.
Use the deployment publisher for an atomic staged release rather than writing
directly into a live web root. The six-hour timer is provided but does not run
until installed and enabled on the future VPS. Migrate and back up the private
research store separately; the Git repository contains the sanitized snapshot,
not the full archive.

## Remaining coverage and engineering gaps

The current crawler does not enumerate every meeting, every historical date,
school district, special district, agency, or unsupported portal. The captured
county and municipality websites are leads, not proof that all records on
those sites have been examined. Some document extraction attempts fail, some
sources restrict automated access, and many queued documents remain to be
processed. The title screen is keyword-based and can miss relevant matters or
produce unrelated matches. The public snapshot reports no complete lookback
period. Older binary `.doc` files are not yet supported.

The next major expansion is additional document/portal adapters and audited
date-window coverage, followed by stronger identity/term resolution, more
portraits, and source-bound contract and signature extraction. Preliminary
actions and votes do not become the five-case verified release or its alerts
automatically. Corrections to those verified cases continue through their
existing release and withdrawal mechanism; preliminary data is not an
automatic X posting queue.
