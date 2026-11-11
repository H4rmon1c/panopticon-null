# Private document-first discovery

The repository has seven configured official Legistar **HTML meeting calendars** in the optional broad discovery matrix. The production environment currently selects the separate twelve-endpoint release refresh matrix, so none of these calendars is enabled by the production default. Each calendar source is a fixed `Calendar.aspx` URL in its jurisdiction config. `pnull source discover-calendar <source-id>` uses the same source-review, DNS-safe HTTPS, conditional-request, interval, and byte-limit controls as `source archive`. It retains exact HTML bytes and fetch observations in the private store.

The command extracts same-host `MeetingDetail.aspx` links and `View.ashx` agenda, agenda-packet, and minutes links. It writes a private, digest-named candidate artifact at `discovery-candidates/<source-id>/<calendar-sha256>.json`. Each candidate records the link URL, kind, anchor position in the archived HTML, and source snapshot digest. Repeated captures of identical bytes produce the same artifact. A changed page gets a new artifact; a page with no eligible links fails instead of reporting a false empty calendar. The parser does not follow links, crawl arbitrary hosts, or create public facts.

The optional calendar collector has a [separate fallback matrix](../deploy/selfhost/collection-fallbacks.tsv) for five official Legistar event APIs. It tries an API only if that jurisdiction's HTML calendar review or fetch fails, and only after verifying a current review of the API source itself. The run still exits as degraded so monitoring does not hide the failed document path. API event discovery retains bounded raw responses in the private ledger; an exact individual vote still requires its own cited roll-call record. Douglas County and Littleton do not yet have verified API fallback profiles.

```sh
pnull --config configs/states/co-commerce-city.toml \
  --data-dir /var/lib/panopticon source review verify commerce-city-calendar
pnull --config configs/states/co-commerce-city.toml \
  --data-dir /var/lib/panopticon source discover-calendar commerce-city-calendar
```

The first command will fail until a current review of that exact calendar endpoint is recorded. Do not enable unattended retrieval merely because the endpoint is public. The review is at the **source level**, not for every new record. The scheduled matrix checks the review before each fetch. The candidate artifact and the raw source archive stay outside the static web root.

## Non-Legistar town indexes

[Silver Plume's official agenda and minutes index](silver-plume-discovery.md) is a separate small-town HTML/PDF source. `source discover-index` creates private same-host PDF candidates under an exact upload prefix. Its index and PDF path policies remain unscheduled until reviewed. An official API may supplement a missing named roll call if one is found and reviewed; the town profile does not assume one exists.

The same reviewed exact-index and same-host PDF-prefix shape now supports
[Moffat and Simla](moffat-simla-discovery.md), [Del Norte](del-norte-discovery.md), and [Saguache](saguache-discovery.md).
The index path and PDF prefix are configured per town, rather than chosen from
two hardcoded index paths. A current source review is still required for each
exact index and PDF path policy before private capture.

## Schedule the five configured town indexes

The opt-in [`collection-small-town-sources.tsv`](../deploy/selfhost/collection-small-town-sources.tsv) selects Silver Plume, Moffat, Simla, Del Norte, and Saguache through `discover-index`. The collector now dispatches that adapter directly. Before its first CLI operation, it validates the complete selected matrix, every referenced TOML source, parser compatibility, bounded URL/size/interval settings, and the index's same-host PDF policy. A malformed later row fails the run before earlier sources can be fetched. A missing or expired source review still skips that source and marks the run degraded while other reviewed sources continue.

This optional matrix records no source reviews, enables no timer, and performs no recursive crawl. After current reviews of the exact index and PDF policies exist, an explicit town discovery run can use:

```sh
PNUL_COLLECT_MATRIX_PATH=deploy/selfhost/collection-small-town-sources.tsv \
PNUL_COLLECT_FALLBACKS_PATH=deploy/selfhost/collection-release-fallbacks.tsv \
bash deploy/selfhost/collect.sh
```

The fallback file is empty because the town indexes have no established API fallback. Keep the production release-refresh schedule when adding discovery: either run this profile separately or combine its rows with the exact release rows in a separately selected matrix. Replacing the release matrix with only town indexes would stop refreshing the existing release sources. Candidate PDF fetching still requires corresponding rows in `candidate-queues.tsv`; the checked-in queue is empty. A parsed index only produces private document links.

## Archive one discovered official document

Each of the seven calendar configs now has two **path policies**: one for its exact same-host `/MeetingDetail.aspx` HTML path and one for `/View.ashx` agenda, packet, and minutes PDFs. A current source review of that path policy covers new query-addressed documents on the same path; it does not require a separate review of every meeting. The operator must still review that path's robots and terms before recording the policy. Path-policy IDs are `<jurisdiction>-meeting-documents` and `<jurisdiction>-pdf-documents`.

After reviewing and capturing the appropriate path policy, select one `anchor_index` from the private calendar artifact:

```sh
pnull --config configs/states/co-commerce-city.toml \
  --data-dir /var/lib/panopticon source archive-candidate \
  commerce-city-calendar <calendar-snapshot-sha256> <anchor-index> \
  --source-id commerce-city-pdf-documents
```

The command re-parses the preserved calendar HTML and checks the exact candidate artifact and its original HTTP receipt before fetching. It rejects a changed artifact, wrong host/path/kind, redirect, wrong content type, missing or expired path review, and a source-config change. The shared fetcher enforces DNS-safe HTTPS and size limits. One invocation fetches **one** document; the path-wide minimum interval is 60 seconds for meeting HTML and 120 seconds for PDFs, including failed fetches. It retains the raw bytes, fetch receipt, and a private record binding the calendar snapshot and anchor to the document digest and source review. A changed response at the **same URL** is reported with old and new SHA-256 digests; both versions remain private and no public claim is changed. These policy sources are not in the automatic collection matrix.

This remains a discovery queue, not complete coverage. The default calendar view may omit older meetings or documents. A candidate agenda, minute, contract, or vote must be parsed with exact locators and checked against corroborating records before a case can be published. Where a published HTML/PDF record does not expose an individual roll call, a bounded official API response can still supply that narrow fact. The public site never calls that API.

## Bounded private queue

After the origin index and its candidate path policies have current source-level reviews, `source archive-queue` can follow their preserved link candidates without selecting every anchor by hand:

```sh
pnull --config configs/states/co-commerce-city.toml \
  --data-dir /var/lib/panopticon source archive-queue commerce-city-calendar \
  --max-fetches 2 --refresh-after-seconds 604800 --dry-run
```

Remove `--dry-run` to fetch. Each newly preserved index also updates a private SQLite scheduling index. The command examines at most 256 due candidates per invocation, re-verifies each selected index's raw bytes, SHA-256, accepted HTML type, public-network receipt, and immutable candidate artifact **before any document request**, and does not rescan every historical artifact. It checks the current origin and path-policy reviews, maps each candidate to the matching same-host meeting HTML or PDF policy, and rejects a changed config or unsafe path. It makes at most eight attempts per invocation and at most one per reviewed path policy, with the policy's atomic fetch cooldown enforced again at request time. `--snapshot <sha256>` selects one preserved index directly for an explicit historical run. Raw documents, receipts, and prior versions remain private.

Indexed snapshots contribute never-fetched historical candidates; duplicate URLs are collapsed. Accepted prior receipts act as durable checkpoints. Already archived URLs are rechecked after `--refresh-after-seconds` only when they still appear on the newest index, or when an operator selects an older snapshot explicitly. When both historical candidates and current refreshes are due, the private scheduler alternates their priority between runs, so neither class can occupy every bounded selection window. A failed request remains eligible after its cooldown; a failed batch reports nonzero without undoing successful private captures. A link that falls outside the current reviewed path or exact robots rules is marked blocked in the private scheduler and reported while other eligible links continue. These blocked links are reconsidered when a new index snapshot or policy review changes their context. A missing or expired document-path review blocks only that path's candidates; other reviewed paths can continue. The origin review remains a prerequisite for the whole run. The queue does not parse a vote or publish a case.

Legacy candidate artifacts created before the scheduling index was added do not enter the automatic backlog until the official index is rediscovered under a current review. An exact old digest can still be processed with `--snapshot`; historical backfill of every pre-index artifact is a separate migration task. The current candidate matrix is empty, so no scheduled backlog is affected.
The 256-row window alternates pending and current-refresh work. Within each
turn, it interleaves meeting HTML and document PDF candidates, so a large
meeting backlog does not hide the PDF policy from the bounded window. Each
selected candidate still passes the same raw-byte, receipt, review, and path
checks before a request. An exact `--snapshot` remains available for targeted
historical work.

[`candidate-queues.tsv`](../deploy/selfhost/candidate-queues.tsv) is intentionally empty until source reviews are complete. Add one exact config/origin row after review, then run [`collect-candidates.sh`](../deploy/selfhost/collect-candidates.sh) periodically. It validates its entire bounded matrix before calling the CLI and shares the collector lock while reading candidate artifacts. The production systemd chain invokes candidate processing after collection; an optional [`panopticon-candidates.timer`](../deploy/selfhost/panopticon-candidates.timer) can also run every 15 minutes, but is disabled by default. No VPS timer has been activated yet. `bash deploy/selfhost/test-candidate-queue.sh` tests the schedule without network access. The regular calendar collector still needs historical calendar paging; a default calendar view can omit older meetings before they ever become candidates.

The [private case proposal builder](private-case-proposals.md) accepts a
preserved candidate archive and exact cited spans, including non-Legistar town
PDFs. It distinguishes direct votes from consent blocks, failed motions from
approvals, and a contract mention from an established award. Proposals retain
explicit gaps and correction history. The verified release bridge reopens the
candidate and checks its current source reviews and exact proof links before
publication. Historical calendar paging and broader official source coverage
remain future work.

## Remaining automatic-search gap

Fixed `source archive` and candidate-queue commands preserve raw bytes and receipts. They do not automatically produce extracted `EvidenceRecord` entries, keyword findings, or case proposal inputs. The collector's final `scan` command scans previously ingested evidence; it does not convert every newly archived document into searchable text. `case propose` can extract an explicitly supplied candidate while checking exact claims, but still needs an authored proposal input. A durable archive-to-extraction-and-screening worker, historical calendar paging, official source onboarding beyond the twelve configured discovery jurisdictions, and automated portrait acquisition/identity checks remain needed for statewide automatic research. Current scheduled refresh must not be described as searching all 4,111 roster entries.
