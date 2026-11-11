# Colorado production deployment

The owner purchased a Namecheap **Magnetar** VPS for `panopticon.fail`. The host is still provisioning and no IP, SSH access, DNS cutover, or live deployment has been verified. This directory is the deployment package; [production commissioning](../../docs/production-operations.md) records the host work that remains.

The public server serves a static atlas, per-decision evidence pages, and the versioned [alerts/news API](../../docs/alerts-api.md). SQLite, original evidence, source review notes, candidate documents, and proposal inputs stay in `/var/lib/panopticon`, outside Caddy's roots. Application code is a reviewed, root-owned checkout at `/srv/panopticon/repo`. Scheduled jobs never pull or execute new GitHub code automatically.

## Automatic updates

Enable `panopticon-collect.timer` after commissioning. It starts ten minutes after boot and 25 hours after the previous collection finishes. Each collection completion, including a degraded run, starts the candidate stage; each candidate completion starts publication. All stages independently enforce their source and publication gates. A failure in one source does not hide other successfully checked sources. Publication failure preserves the last complete release.

The production environment defaults to [`collection-release-sources.tsv`](collection-release-sources.tsv): **12 exact previously reviewed endpoints** across Commerce City, Castle Rock, and Denver. These support four version 2 decisions in the five-case release. Colorado Springs is a legacy approved static case; its original sources do not yet have the recurring capture reviews needed to assert live freshness. The existing endpoint reviews expire **2026-12-09 at 22:30:23 UTC**. The owner's November 10 request for automatic updates authorizes recurring retrieval within those same reviewed exact endpoints; it does not broaden the attested source scope or change the recorded expiry.

The broader [`collection-sources.tsv`](collection-sources.tsv) has seven calendar sources, two procurement sources, and the same twelve archive endpoints. Switch to it and `collection-fallbacks.tsv` only after the additional source reviews exist. Its five API fallbacks are independently gated and cannot replace exact release evidence. Missing/expired/mismatched review records prevent retrieval. No review is renewed automatically.

The five configured town document indexes can be selected through the opt-in [`collection-small-town-sources.tsv`](collection-small-town-sources.tsv) and the `discover-index` adapter after their source reviews exist. The entire schedule and referenced source definitions are checked before any CLI retrieval call. Selecting the matrix does not create reviews or enable timers. See [the discovery guide](../../docs/document-first-discovery.md).

The candidate activation matrix is empty. Its stage reports **disabled**, rather than claiming new areas are being discovered. Adding reviewed candidate queues and verified proposal inputs is required for automatic new-case expansion. Publication accepts only configured manifests and original private proposal inputs that pass the Rust proof verifier. It does not publish arbitrary fetched records or invent missing votes, contract execution, or portraits.

`publish.sh` locks against concurrent capture, validates the complete staged bundle and portraits, creates decision pages, alerts/feed and coverage APIs, and switches one symlink atomically. Alert revision IDs stay stable on repeat exports; explicit corrections/withdrawals are retained, while a silently disappearing case blocks publication. All lock opens are nontruncating and use `O_NOFOLLOW`; directory provisioning uses directory FDs to avoid following service-planted symlinks during root installation.

## Health and recovery

The stage wrapper records private operational state and writes a sanitized `/api/v1/health.json` from `/srv/panopticon/status`. Publication failure can update health without mutating the last good static release. The hourly health timer reports failed stages, stale success timestamps, clock errors, missing/expired source review records, and disabled candidate collection. It contains no private reviewer notes or filesystem paths. Green health describes the configured schedule; it does not prove all published sources were fetched recently or that Colorado coverage is complete.

`panopticon-research.timer` is a separate six-hour preliminary discovery and
screening queue. After commissioning and testing the actual host sandbox, enable
it alongside the reviewed-source timer. It checks directory-linked websites,
preserves source bytes and network receipts in `/var/lib/panopticon/research`,
screens supported official matter titles and linked agenda/minutes PDFs, and
triggers publication. Its labeled references appear at `/research.html` and
`/api/v1/research.json`; they are not verified votes or approvals. The exporter
reopens and checks source digests, receipts, anchors, extracted lines, and
where available the meeting action and recorded vote rows before including a
reference. Vote scope stays explicit. Failed extraction is not a no-match result. The existing
12-source review schedule and publication gates remain separate. Move the
private research directory with the other encrypted state when provisioning a
new VPS; the checked-in static report alone does not contain the raw archive.

The health endpoint includes the research stage's last run and failure state.
A successful stage does not mean every site or PDF succeeded; individual
screening errors and pending work are counted in the preliminary dataset.

`backup.sh` uses SQLite's online backup API while holding the collection/publication lock, then invokes encrypted restic storage for coherent copies of both databases, evidence, canonical records, discovery and research archives, configured private proposal inputs, and the current public release including API history. Configure an actual off-server backend and password file before enabling the optional backup timer. The accepted backend prefix is not proof of physical separation; verify the storage account and restoration independently. Retention deletion is deliberately an operator action. Original secrets and deployment environment files require their own encrypted recovery copy.

## Verification

Run from the repository root:

```console
bash deploy/selfhost/test-collect.sh
bash deploy/selfhost/test-candidate-queue.sh
bash deploy/selfhost/test-publish.sh
python3 deploy/selfhost/test-health.py
python3 deploy/selfhost/test-backup.py
python3 deploy/selfhost/test-locks.py
python3 deploy/selfhost/test-worklist-schedules.py
```

These tests perform no network capture and do not change production state. They cover source routing, failed-review isolation, publication rollback, repeated export, real staged withdrawals and retained correction pages, source expiry and stale health, private-data exclusion, coherent SQLite backup, and planted lock/directory symlinks. Host TLS, service isolation, PDF sandbox execution, off-server backup restoration, and external alert delivery remain commissioning checks.
