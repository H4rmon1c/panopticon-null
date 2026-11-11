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
   directory and county sites from an archived county-clerks directory. Visit
   bounded pages on each site, find linked official meeting portals and
   same-host agenda/minutes PDFs and Word documents, and retain the exact index page and link.
2. Prefer linked government documents. For linked Legistar installations, use
   its public API as an additional matter-title index. Query a bounded number of
   pages for configured vendor and technology terms. A title match is a lead,
   not proof of approval or deployment. The collector rotates between portals
   and resumes pagination across runs, resetting after the final page or the
   100-page safety cap; changing page size also resets the cursor. Results can
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
   complete extraction; a partial scan cannot become a no-match result. DOCX files are
   parsed in a separate network-isolated sandbox with ZIP, XML, memory, CPU,
   and output limits; embedded links and files are never followed.
4. For matched Legistar matters, capture the matter history, corresponding
   meeting item, recorded votes, and any separate consent-agenda item. The
   exporter reopens every raw capture and verifies client, matter, meeting,
   item, vote IDs, and source locators before showing a vote. Direct item
   votes, consent-block votes, consent-designated items with uncertain vote
   scope, and missing votes stay distinct. A failed motion remains failed.
5. Look for identity-labeled portraits on government directories and retain
   the original image bytes. A same-name official source or explicitly sourced
   nickname is required; a missing portrait stays missing. The interface can
   also reuse an existing sourced portrait when the portal, jurisdiction,
   person ID, and name all match. Portrait dates are not inferred.
   Five Arapahoe County research voters also reuse the original JPEG streams
   from the labeled 2022 board page of the county's recommended budget. The
   archived PDF digest, page, image indexes, aliases, and extracted-page fixture
   are pinned in `fixtures/atlas/portrait-sources/arapahoe-2022-budget-proof.json`.
6. Export a strict, sanitized static JSON snapshot. Every entry is labeled
   `unconfirmed_source_reference`; source links, digests, timestamps, exact
   locators, and any uncertain vote scope remain visible. The website and
   `/api/v1/research.json` use this snapshot. Raw documents, raw responses,
   network receipts, extracted text, and the SQLite queue remain private. For
   each jurisdiction with a recorded website attempt, the snapshot reports
   partial screening, failure, and queue counts without asserting a complete
   date window.

The research collector records its own machine observations; it does not
fabricate human review decisions or change the existing verified-release
gates. Source policy is checked for each captured site. Explicit robots
disallows, access denials, CAPTCHAs, and login requirements are not bypassed.
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
The `--revalidate-ocr` mode rechecks earlier OCR results that lack a validated
complete page set; until then, they count as errors rather than completed
screens. `docx-saved --revalidate-docx` similarly rechecks older Word
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
