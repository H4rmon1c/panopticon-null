# State bot architecture

`StateConfig` separates state code/name, jurisdiction, canonical site URL, source list, feed label, rate policy, and fixture metadata from application logic. Colorado now has separate configurations for Colorado Springs, Denver, Commerce City, Castle Rock, Arapahoe County, Douglas County, and Littleton. The self-hosted collection matrix routes each source through its jurisdiction's config; source IDs remain unique across the private ledger. This is configured discovery coverage for those calendars, not a claim that every Colorado jurisdiction has been monitored or every source reviewed. The default collector archives official HTML calendars and writes private meeting/document link candidates. Five official API event feeds run conditionally when their HTML calendar fails, and each API requires its own current source review. A private bounded candidate queue and source-checked proposal builder exist, but neither is a public release adapter.

Direct-document archive profiles now cover 39 fixed official HTML, PDF, and JSON URLs across Commerce City, Castle Rock, Denver, and Arapahoe County. Twelve release-cited endpoints have current source-level reviews and live captures; other profiles still require their own reviews. The calendar parser supplies new private URL candidates. Fourteen configured calendar host/path policies and five unscheduled town index/PDF policy pairs allow bounded capture after source review. The candidate queue's activation matrix is empty. A source-checked proposal can enter the static release only when its exact decision chain passes the verified bridge; no newly discovered proposal has done so yet. The public site reads only a locally published static snapshot and geography file; it has no runtime dependency on municipal APIs.

The X component accepts a state-derived alert and canonical base URL. It does not know Colorado-specific detection logic. A draft:

- identifies itself as automated;
- names the jurisdiction and monitored matter;
- states the current public-record state without claiming that an ordinance proves a vendor purchase;
- gives the detected change;
- links the local alert page, which contains citations and hashes;
- fits 280 Unicode scalar values per post and emits a short numbered thread when necessary.

Drafting is dry-run by default and produces zero network posts. Approval stores a digest over alert ID and every exact post. Posting checks that digest, local approval, explicit `--confirm`, a non-placeholder canonical URL, and runtime credentials. The trait-backed transport is replaced by a fake in tests; no test, fixture, or demo constructs a live X transport.

## X thread safety and reconciliation

Uncertain X attempts are never blindly retried. Attempts are recorded append-only with per-segment state, and operator reconciliation is required before any further attempt.

Commands:

- `pnull x attempts` — list recorded attempts.
- `pnull x status <alert-id>` — show the current status and segments of an attempt.
- `pnull x reconcile <attempt-id> --decision <...> --operator <name> --note <text> [--remote_id <id>]` — record an append-only operator decision.

Reconciliation decisions cover the possible states of an uncertain attempt:

- confirm a segment exists and record its remote ID/URL;
- confirm none was posted;
- mark the attempt partially posted;
- abandon the attempt;
- authorize a new attempt only after confirming that none posted.

There is no blind retry of uncertain attempts, no deletion of audit history, and no way for a reconciliation to rewrite a prior decision. Partial and abandoned attempts retain their posting reservation and block new attempts. `confirm_posted` requires a remote ID for every segment; it can fill the sole missing ID during reconciliation.

## Publication gate

The X pipeline fails closed on pending, rejected, stale, or mismatched citation-review decisions, and obeys the same publication gates as the site and Atom feed. The demo produces only dry-run X drafts and zero network posts.

A future state feed should add one reviewed configuration and source adapter fixtures. It must not copy the publication or transport logic. Account creation, registration, scheduling, and credential provisioning are outside this project and are never automated.
