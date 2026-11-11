# Colorado production release — November 10, 2026

This release is prepared for the owner's Namecheap Magnetar VPS. No server IP,
SSH connection, DNS cutover, or live HTTPS test has been performed. The website
is a partial Colorado public-record atlas, not a completed statewide investigation.

## Published evidence

The production exporter passed against the existing private source receipts and
review store for **five decisions across four jurisdictions**: Colorado Springs
Ordinance 25-93; Commerce City Ordinances 2755 and 2756; Denver's **failed**
Resolution 25-0413; and Castle Rock Resolution 2025-071. The snapshot contains
54 entities, 246 evidence entries, 15 cited sources, and 47 recorded vote rows.
These are 38 scoped recorded voters; the nine Commerce City voters appear on
two different ordinances. Portraits identify recorded voters and do not prove
their terms of office or appearance on the vote date.

Castle Rock adds six Yes votes and one Abstain on its Flock drone-services
agreement authorization. The town's execution, payments, and deployment remain
not established. Its previously held HTML revision was reconciled by comparing
the exact preserved byte streams. Only two ASP.NET state values and eleven
resource-cache timestamps differ; all other bytes match. Both originals and a
[revision record](../fixtures/co-castlerock/detail-revision-2026-11-10.json) remain
preserved. The [revision check](../web/panopticon-terminal/scripts/check-castle-rock-revision.py)
rejects any other difference. It is an audit of these two snapshots, not an
automatic normalization rule for future sources. Every published Castle Rock
fact has an independent event, matter, or complete roll-call JSON proof, so the
volatile HTML is retained as historical context rather than a mandatory release
citation. The seven original town portraits pass the provenance validator.

## Coverage still missing

The geography index contains 4,111 entries: one state, 64 counties, 273 cities
and towns, 178 school districts, and 3,595 provisional DOLA district entries.
**4,107 roster entries have no published decision.** An entry without a case
means records not indexed; it does not establish absence of surveillance.

The operator worklist still has 4,099 entries at `not_started`: one state,
62 counties, 263 municipalities, 178 school districts, and all 3,595 provisional
districts. Seven official calendar profiles and five small-town document-index
profiles exist. The municipal directory provides 241 website leads, not reviewed
source coverage. Four consent-case manifests remain previews: two Arapahoe
County items and two Denver Axon items. Their consent-block positions cannot be
presented as separate item votes. State agencies, universities, authorities,
tribal governments, historical lookback, and cross-term official identities
remain incomplete.

Rebuild the complete missing-work list locally:

```sh
python3 web/panopticon-terminal/scripts/build-colorado-worklist.py \
  --output /tmp/colorado-worklist.csv
```

The generated `/api/v1/coverage.json` reports every geography row against the
actual released snapshot. It reports partial coverage and no established
lookback period. Collection health and source permissions are separate facts.

## Updates and consumers

Use the [self-host deployment guide](../deploy/selfhost/README.md) for the
collection, candidate, publication, health, and recovery services. Scheduled
work remains bounded by source review, origin, path, rate, and byte limits.
Previously reviewed archive endpoints are scheduled for recurring checks under
the owner's November 10 instruction to enable self-updates. Their recorded
attestations and expiration remain unchanged: **December 9, 2026 at 22:30:23 UTC**.
Expired or changed policies stop that source and report a degraded run.

The public alerts API is generated from the same sanitized release. Its data
can support a later X bot, but publication time is not a fresh source check.
Consumers must retain deduplication state and handle updates and withdrawals;
the API itself does not post to X. See the API contract included in this release.

The default 12-endpoint schedule refreshes the four v2 decisions in Commerce
City, Castle Rock, and Denver. Colorado Springs remains a legacy approved
static case until its exact acquisition sources have current source reviews.
Healthy collection means the configured schedule succeeded, not that every
published source or every Colorado jurisdiction was checked.

Newly discovered documents are private candidates. Automatically finding a
meeting does not establish a vote or create a publishable case. The existing
proposal bridge checks explicit source-backed claims; broader source-specific
verifiers and reviewed source profiles are still needed for statewide expansion.

Before public cutover, test the actual VPS firewall and SSH access, systemd
sandbox, Caddy TLS, scheduled jobs, monitoring destination, encrypted off-host
backup and restore, and release rollback. Local code verification cannot prove
those host properties or guarantee protection from a determined attacker.

## Integrated verification

The final local pass completed 413 Rust tests and 38 JavaScript tests, plus
the separately invoked real Bubblewrap/Poppler integration test. Formatting,
workspace Clippy with warnings denied, and dependency advisory/license policy
passed. Source-profile, source-review, collection, candidate-queue, health,
lock, backup, and staged publication tests passed. The review packet tests
retain the original attestation context and reject replay after a registry
revision rather than rewriting the owner's approval.

A real five-case export using the existing private receipts passed the complete
publisher. Caddy configuration validation and 14 local route/header checks
passed, including blocked private paths, API JSON, rejected writes, and degraded
health for collection that has not run. Chromium rendered the Castle Rock
share page with all seven portraits, the recorded abstention, and the execution
gap. These were local HTTP checks; live domain TLS and the VPS remain untested.

See the [security findings and remaining conditions](production-security-review-2026-11-10.md),
[operations handoff](production-operations.md), and [alerts API contract](alerts-api.md).
